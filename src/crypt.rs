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

use crate::plasma::StreamRead;

use std::io::{BufRead, Cursor, Result, Error, ErrorKind, Write};

use byteorder::{ReadBytesExt, WriteBytesExt};
use tokio::net::TcpStream;

pub const CLI_TO_SRV_CONNECT: u8 = 0;
pub const SRV_TO_CLI_ENCRYPT: u8 = 1;
pub const SRV_TO_CLI_ERROR: u8 = 2;

struct CryptConnectHeader {
    msg_id: u8,
    msg_size: u8,
    key_seed: [u8; 64],
}

impl CryptConnectHeader {
    const FIXED_SIZE: usize = 2;
    const CLIENT_KEY_SIZE: usize = 64;
    const MAX_SIZE: usize = CryptConnectHeader::FIXED_SIZE
                            + CryptConnectHeader::CLIENT_KEY_SIZE;

    const SERVER_SEED_SIZE: usize = 7;
    const ENCRYPT_REPLY_SIZE: usize = CryptConnectHeader::FIXED_SIZE
                                      + CryptConnectHeader::SERVER_SEED_SIZE;
}

impl StreamRead for CryptConnectHeader {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        // Only reads the fixed parts of the message
        let msg_id = stream.read_u8()?;
        let msg_size = stream.read_u8()?;
        Ok(Self { msg_id, msg_size, key_seed: [0u8; 64] })
    }
}

fn create_error_reply() -> Result<Vec<u8>> {
    let mut stream = Cursor::new(Vec::new());
    stream.get_mut().reserve(CryptConnectHeader::FIXED_SIZE);
    stream.write_u8(SRV_TO_CLI_ERROR)?;
    stream.write_u8(CryptConnectHeader::FIXED_SIZE as u8)?;
    Ok(stream.into_inner())
}

fn create_crypt_reply(server_seed: &[u8]) -> Result<Vec<u8>> {
    let mut stream = Cursor::new(Vec::new());
    stream.get_mut().reserve(CryptConnectHeader::ENCRYPT_REPLY_SIZE);
    stream.write_u8(SRV_TO_CLI_ENCRYPT)?;
    stream.write_u8(CryptConnectHeader::ENCRYPT_REPLY_SIZE as u8)?;
    stream.write_all(server_seed)?;
    Ok(stream.into_inner())
}

pub async fn init_crypt(sock: &mut TcpStream) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut buffer = [0u8; CryptConnectHeader::FIXED_SIZE];
    sock.read_exact(&mut buffer).await?;
    let mut stream = Cursor::new(buffer);
    let mut crypt_header = CryptConnectHeader::stream_read(&mut stream)?;
    if crypt_header.msg_size > (CryptConnectHeader::FIXED_SIZE as u8)
        && crypt_header.msg_size <= (CryptConnectHeader::MAX_SIZE as u8)
    {
        // Header contains an encrypt client key
        let key_size = (crypt_header.msg_size as usize) - CryptConnectHeader::FIXED_SIZE;
        sock.read_exact(&mut crypt_header.key_seed[0..key_size]).await?;
        // TODO: Do we need to swap the endianness of the whole buffer?
    } else if crypt_header.msg_size != (CryptConnectHeader::FIXED_SIZE as u8) {
        let reply = create_error_reply()?;
        sock.write_all(&reply).await?;
        return Err(Error::new(ErrorKind::Other,
                   format!("Invalid encryption header size {}", crypt_header.msg_size)));
    }

    if crypt_header.msg_id != CLI_TO_SRV_CONNECT {
        let reply = create_error_reply()?;
        sock.write_all(&reply).await?;
        return Err(Error::new(ErrorKind::Other,
                   format!("Invalid encrypt message type {}", crypt_header.msg_id)));
    }

    let mut server_seed = [0u8; 7];
    // TODO: generate server_seed
    let reply = create_crypt_reply(&server_seed)?;
    sock.write_all(&reply).await?;

    Ok(())
}
