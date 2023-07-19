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

use std::io::{BufRead, Cursor, Result, ErrorKind};
use std::sync::Arc;

use byteorder::{LittleEndian, ReadBytesExt};
use log::{error, warn};
use tokio::io::{AsyncReadExt, BufReader};
use tokio::sync::mpsc;
use tokio::net::TcpStream;
use uuid::Uuid;

use crate::general_error;
use crate::config::ServerConfig;
use crate::crypt::CryptStream;
use crate::plasma::{StreamRead, StreamWrite, BitVector};
use super::messages::{CliToAuth, AuthToCli};

pub struct AuthServer {
    incoming_send: mpsc::Sender<TcpStream>,
}

const CONN_HEADER_SIZE: usize = 20;

enum ServerCaps {
    ScoreLeaderBoards,
}

fn read_conn_header<S>(stream: &mut S) -> Result<()>
    where S: BufRead
{
    // Everything here is discarded...
    let header_size = stream.read_u32::<LittleEndian>()?;
    if header_size != CONN_HEADER_SIZE as u32 {
        return Err(general_error!("Invalid connection header size {}", header_size));
    }
    // Null UUID
    let _ = Uuid::stream_read(stream)?;

    Ok(())
}

async fn send_message(stream: &mut CryptStream, reply: AuthToCli) -> bool {
    let mut reply_buf = Cursor::new(Vec::new());
    if let Err(err) = reply.stream_write(&mut reply_buf) {
        warn!("Failed to write reply stream: {}", err);
        return false;
    }
    if let Err(err) = stream.write_all(reply_buf.get_ref()).await {
        warn!("Failed to send reply: {}", err);
        false
    } else {
        true
    }
}

async fn init_client(mut sock: TcpStream, server_config: &ServerConfig)
    -> Result<BufReader<CryptStream>>
{
    let mut header = [0u8; CONN_HEADER_SIZE];
    sock.read_exact(&mut header).await?;
    read_conn_header(&mut Cursor::new(header))?;

    let mut crypt_sock = crate::crypt::init_crypt(sock, &server_config.auth_n_key,
                                                  &server_config.auth_k_key).await?;

    /* Shard Capabilities */
    let mut caps = BitVector::new();
    caps.set(ServerCaps::ScoreLeaderBoards as usize, true);
    let mut caps_buffer = Cursor::new(Vec::new());
    caps.stream_write(&mut caps_buffer)?;
    let caps_msg = AuthToCli::ServerCaps { caps_buffer: caps_buffer.into_inner() };
    if !send_message(crypt_sock.get_mut(), caps_msg).await {
        return Err(general_error!("Failed to send ServerCaps message"));
    }

    Ok(crypt_sock)
}

async fn auth_client(client_sock: TcpStream, server_config: Arc<ServerConfig>) {
    let mut stream = match init_client(client_sock, &server_config).await {
        Ok(cipher) => cipher,
        Err(err) => {
            warn!("Failed to initialize client: {}", err);
            return;
        }
    };

    loop {
        match CliToAuth::read(&mut stream).await {
            Ok(CliToAuth::PingRequest { trans_id, ping_time, payload }) => {
                let reply = AuthToCli::PingReply {
                    trans_id, ping_time, payload
                };
                if !send_message(stream.get_mut(), reply).await {
                    return;
                }
            }
            Err(err) => {
                if !matches!(err.kind(), ErrorKind::ConnectionReset | ErrorKind::UnexpectedEof) {
                    warn!("Error reading message from client: {}", err);
                }
                return;
            }
            _ => todo!(),
        }
    }
}

impl AuthServer {
    pub fn start(server_config: Arc<ServerConfig>) -> AuthServer {
        let (incoming_send, mut incoming_recv) = mpsc::channel(5);

        tokio::spawn(async move {
            while let Some(sock) = incoming_recv.recv().await {
                let server_config = server_config.clone();
                tokio::spawn(async move {
                    auth_client(sock, server_config).await;
                });
            }
        });
        AuthServer { incoming_send }
    }

    pub async fn add(&mut self, sock: TcpStream) {
        if let Err(err) = self.incoming_send.send(sock).await {
            error!("Failed to add client: {}", err);
            std::process::exit(1);
        }
    }
}
