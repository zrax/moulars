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

use std::io::{self, BufRead, Cursor};
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use byteorder::{LittleEndian, ReadBytesExt};
use log::{error, warn, debug};
use tokio::io::{AsyncReadExt, BufReader};
use tokio::sync::mpsc;
use tokio::net::TcpStream;
use uuid::Uuid;

use crate::config::ServerConfig;
use crate::net_crypt::CryptTcpStream;
use crate::plasma::{StreamRead, StreamWrite};
use super::messages::{CliToGateKeeper, GateKeeperToCli};

pub struct GateKeeper {
    incoming_send: mpsc::Sender<TcpStream>,
}

struct GateKeeperWorker {
    stream: BufReader<CryptTcpStream>,
    server_config: Arc<ServerConfig>,
}

const CONN_HEADER_SIZE: u32 = 20;

fn read_conn_header<S>(stream: &mut S) -> Result<()>
    where S: BufRead
{
    // Everything here is discarded...
    let header_size = stream.read_u32::<LittleEndian>()?;
    if header_size != CONN_HEADER_SIZE {
        return Err(anyhow!("Invalid connection header size {header_size}"));
    }
    // Null UUID
    let _ = Uuid::stream_read(stream)?;

    Ok(())
}

async fn init_client(mut sock: TcpStream, server_config: &ServerConfig)
    -> Result<BufReader<CryptTcpStream>>
{
    let mut header = [0u8; CONN_HEADER_SIZE as usize];
    sock.read_exact(&mut header).await?;
    read_conn_header(&mut Cursor::new(header))?;

    crate::net_crypt::init_crypt(sock, &server_config.gate_n_key,
                                 &server_config.gate_k_key).await
}

impl GateKeeper {
    pub fn start(server_config: Arc<ServerConfig>) -> GateKeeper {
        let (incoming_send, mut incoming_recv) = mpsc::channel(5);

        tokio::spawn(async move {
            while let Some(sock) = incoming_recv.recv().await {
                GateKeeperWorker::start(sock, server_config.clone());
            }
        });
        GateKeeper { incoming_send }
    }

    pub async fn add(&mut self, sock: TcpStream) {
        if let Err(err) = self.incoming_send.send(sock).await {
            error!("Failed to add client: {err}");
        }
    }
}

impl GateKeeperWorker {
    pub fn start(sock: TcpStream, server_config: Arc<ServerConfig>) {
        tokio::spawn(async move {
            let stream = match init_client(sock, &server_config).await {
                Ok(cipher) => cipher,
                Err(err) => {
                    warn!("Failed to initialize client: {err}");
                    return;
                }
            };

            let mut worker = GateKeeperWorker { stream, server_config };
            worker.run().await;
        });
    }

    fn peer_addr(&self) -> io::Result<SocketAddr> { self.stream.get_ref().peer_addr() }

    async fn run(&mut self) {
        loop {
            match CliToGateKeeper::read(&mut self.stream).await {
                Ok(message) => {
                    if !self.handle_message(message).await {
                        break;
                    }
                }
                Err(err) => {
                    match err.downcast_ref::<io::Error>() {
                        Some(io_err) if matches!(io_err.kind(), io::ErrorKind::ConnectionReset
                                                                | io::ErrorKind::UnexpectedEof) => {
                            debug!("Client {} disconnected", self.peer_addr().unwrap());
                        }
                        _ => warn!("Error reading message from client: {err}"),
                    }
                    return;
                }
            }
        }
        warn!("Dropping client {}", self.peer_addr().unwrap());
    }

    async fn handle_message(&mut self, message: CliToGateKeeper) -> bool {
        match message {
            CliToGateKeeper::PingRequest { trans_id, ping_time, payload } => {
                self.send_message(GateKeeperToCli::PingReply {
                    trans_id, ping_time, payload
                }).await
            }
            CliToGateKeeper::FileServIpAddressRequest { trans_id, from_patcher } => {
                // Currently unused
                let _ = from_patcher;

                self.send_message(GateKeeperToCli::FileServIpAddressReply {
                    trans_id,
                    ip_addr: self.server_config.file_serv_ip.clone(),
                }).await
            }
            CliToGateKeeper::AuthServIpAddressRequest { trans_id } => {
                self.send_message(GateKeeperToCli::AuthServIpAddressReply {
                    trans_id,
                    ip_addr: self.server_config.auth_serv_ip.clone(),
                }).await
            }
        }
    }

    async fn send_message(&mut self, reply: GateKeeperToCli) -> bool {
        let mut reply_buf = Cursor::new(Vec::new());
        if let Err(err) = reply.stream_write(&mut reply_buf) {
            warn!("Failed to write reply stream: {err}");
            return false;
        }
        if let Err(err) = self.stream.get_mut().write_all(reply_buf.get_ref()).await {
            warn!("Failed to send reply: {err}");
            false
        } else {
            true
        }
    }
}
