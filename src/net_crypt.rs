/* This file is part of moulars.
 *
 * moulars is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * moulars is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with moulars.  If not, see <http://www.gnu.org/licenses/>.
 */

use std::io::{self, BufRead, Write, Cursor};
use std::net::SocketAddr;
use std::task::{self, Poll};
use std::pin::Pin;

use anyhow::{anyhow, Context, Result};
use byteorder::{ReadBytesExt, WriteBytesExt};
use crypto_bigint::U512;
use rand::TryRngCore;
use tokio::net::TcpStream;
use tokio::io::{AsyncRead, BufReader, ReadBuf};

use crate::plasma::StreamRead;

pub const CRYPT_BASE_AUTH: u32 = 41;
pub const CRYPT_BASE_GAME: u32 = 73;
pub const CRYPT_BASE_GATE_KEEPER: u32 = 4;

type CryptCipher = rc4::Rc4<rc4::consts::U7>;

pub struct CryptTcpStream {
    stream: TcpStream,
    cipher_read: CryptCipher,
    cipher_write: CryptCipher,
}

impl CryptTcpStream {
    pub fn new(stream: TcpStream, key_data: &[u8]) -> Self {
        use rc4::{Key, KeyInit};

        let key = Key::from_slice(key_data);
        CryptTcpStream {
            stream,
            cipher_read: CryptCipher::new(key),
            cipher_write: CryptCipher::new(key),
        }
    }

    // Don't use AsyncWrite, because we'd have to keep track of what bytes
    // were already encrypted separately from the send buffer...
    pub async fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        use rc4::StreamCipher;
        use tokio::io::AsyncWriteExt;

        let mut crypt_buf = buf.to_vec();
        self.cipher_write.apply_keystream(&mut crypt_buf);
        self.stream.write_all(crypt_buf.as_slice()).await
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.stream.peer_addr()
    }
}

impl AsyncRead for CryptTcpStream {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut task::Context, buf: &mut ReadBuf)
        -> Poll<io::Result<()>>
    {
        use rc4::StreamCipher;

        match Pin::new(&mut self.stream).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                self.cipher_read.apply_keystream(buf.filled_mut());
                Poll::Ready(Ok(()))
            }
            result => result
        }
    }
}

const CLI_TO_SRV_CONNECT: u8 = 0;
const SRV_TO_CLI_ENCRYPT: u8 = 1;
const SRV_TO_CLI_ERROR: u8 = 2;

struct CryptConnectHeader {
    msg_id: u8,
    msg_size: u8,
    key_seed: [u8; U512::BYTES],
}

const SERVER_SEED_SIZE: u8 = 7;
const CLIENT_KEY_SIZE: u8 = 64;

impl CryptConnectHeader {
    const FIXED_SIZE: u8 = 2;
    const MAX_SIZE: u8 = CryptConnectHeader::FIXED_SIZE + CLIENT_KEY_SIZE;
    const ENCRYPT_REPLY_SIZE: u8 = CryptConnectHeader::FIXED_SIZE + SERVER_SEED_SIZE;
}

impl StreamRead for CryptConnectHeader {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        // Only reads the fixed parts of the message
        let msg_id = stream.read_u8()?;
        let msg_size = stream.read_u8()?;
        Ok(Self { msg_id, msg_size, key_seed: [0u8; U512::BYTES] })
    }
}

fn create_error_reply() -> Result<Vec<u8>> {
    let mut stream = Cursor::new(Vec::with_capacity(usize::from(CryptConnectHeader::FIXED_SIZE)));
    stream.write_u8(SRV_TO_CLI_ERROR)?;
    stream.write_u8(CryptConnectHeader::FIXED_SIZE)?;
    Ok(stream.into_inner())
}

fn create_crypt_reply(server_seed: &[u8]) -> Result<Vec<u8>> {
    let mut stream = Cursor::new(Vec::with_capacity(usize::from(CryptConnectHeader::ENCRYPT_REPLY_SIZE)));
    stream.write_u8(SRV_TO_CLI_ENCRYPT)?;
    stream.write_u8(CryptConnectHeader::ENCRYPT_REPLY_SIZE)?;
    stream.write_all(server_seed)?;
    Ok(stream.into_inner())
}

const SERVER_SEED_BIT_SIZE: u32 = (SERVER_SEED_SIZE as u32) * 8;
const CLIENT_KEY_BIT_SIZE: u32 = (CLIENT_KEY_SIZE as u32) * 8;

pub fn u512_pow_mod(base: &U512, exponent: &U512, modulus: &U512) -> U512 {
    use crypto_bigint::Odd;
    use crypto_bigint::modular::{MontyForm, MontyParams};

    let modulus = Odd::new(*modulus).expect("Modulus must be odd");
    MontyForm::new(base, MontyParams::new(modulus)).pow(exponent).retrieve()
}

// Returns the server seed and the local rc4 key data
// NOTE: Returned seed and key are little-endian
fn crypt_key_create(key_n: &U512, key_k: &U512, key_y: &U512)
    -> Result<(Vec<u8>, Vec<u8>)>
{
    let mut server_seed = vec![0u8; SERVER_SEED_SIZE as usize];
    rand::rngs::OsRng.try_fill_bytes(&mut server_seed)
        .context("Failed to generate server seed")?;

    let client_seed: U512 = u512_pow_mod(key_y, key_k, key_n);
    assert!(client_seed.bits() >= SERVER_SEED_BIT_SIZE
            && client_seed.bits() <= CLIENT_KEY_BIT_SIZE);

    let key_buffer = client_seed.to_le_bytes();
    let key: Vec<u8> = key_buffer.iter().take(usize::from(SERVER_SEED_SIZE)).enumerate()
                                 .map(|(i, v)| v ^ server_seed[i]).collect();
    assert_eq!(key.len(), usize::from(SERVER_SEED_SIZE));

    Ok((server_seed, key))
}

pub async fn init_crypt(mut sock: TcpStream, key_n: &U512, key_k: &U512)
    -> Result<BufReader<CryptTcpStream>>
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut buffer = [0u8; CryptConnectHeader::FIXED_SIZE as usize];
    sock.read_exact(&mut buffer).await?;
    let mut stream = Cursor::new(buffer);
    let mut crypt_header = CryptConnectHeader::stream_read(&mut stream)?;
    if crypt_header.msg_size > CryptConnectHeader::FIXED_SIZE
            && crypt_header.msg_size <= CryptConnectHeader::MAX_SIZE
    {
        // Header contains an encrypt client key
        let key_size = crypt_header.msg_size - CryptConnectHeader::FIXED_SIZE;
        sock.read_exact(&mut crypt_header.key_seed[0..usize::from(key_size)]).await?;
    } else if crypt_header.msg_size != CryptConnectHeader::FIXED_SIZE {
        let reply = create_error_reply()?;
        sock.write_all(&reply).await?;
        return Err(anyhow!("Invalid encryption header size {}", crypt_header.msg_size));
    }

    if crypt_header.msg_id != CLI_TO_SRV_CONNECT {
        let reply = create_error_reply()?;
        sock.write_all(&reply).await?;
        return Err(anyhow!("Invalid encrypt message type {}", crypt_header.msg_id));
    }

    let key_y = U512::from_le_slice(&crypt_header.key_seed);
    let (server_seed, crypt_key) = crypt_key_create(key_n, key_k, &key_y)?;
    let reply = create_crypt_reply(&server_seed)?;
    sock.write_all(&reply).await?;

    Ok(BufReader::new(CryptTcpStream::new(sock, &crypt_key)))
}
