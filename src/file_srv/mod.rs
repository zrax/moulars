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

mod manifest;
mod messages;

use std::io::{BufRead, Cursor, Result, Error, ErrorKind};
use std::sync::Arc;

use byteorder::{LittleEndian, ReadBytesExt};
use tokio::io::{AsyncReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::config::ServerConfig;
use crate::netcli::NetResultCode;
use self::messages::{CliToFile, FileToCli};

pub struct FileServer {
    incoming_send: mpsc::Sender<TcpStream>,
}

const CONN_HEADER_SIZE: usize = 12;

fn read_conn_header<S>(stream: &mut S) -> Result<()>
    where S: BufRead
{
    // Everything here is discarded...
    let header_size = stream.read_u32::<LittleEndian>()?;
    if header_size != CONN_HEADER_SIZE as u32 {
        return Err(Error::new(ErrorKind::Other,
                   format!("[File] Invalid connection header size {}", header_size)));
    }
    // Build ID
    let _ = stream.read_u32::<LittleEndian>()?;
    // Server Type
    let _ = stream.read_u32::<LittleEndian>()?;

    Ok(())
}

async fn init_client(mut sock: TcpStream) -> Result<BufReader<TcpStream>> {
    let mut buffer = [0u8; CONN_HEADER_SIZE];
    sock.read_exact(&mut buffer).await?;
    read_conn_header(&mut Cursor::new(buffer))?;

    Ok(BufReader::new(sock))
}

macro_rules! send_message {
    ($stream:expr, $reply:expr) => {
        if let Err(err) = $reply.write($stream.get_mut()).await {
            eprintln!("[File] Failed to send reply message: {:?}", err);
            return;
        }
    }
}

async fn file_server_client(client_sock: TcpStream, server_config: Arc<ServerConfig>) {
    let mut stream = match init_client(client_sock).await {
        Ok(stream) => stream,
        Err(err) => {
            eprintln!("[File] Failed to initialize client: {:?}", err);
            return;
        }
    };

    loop {
        match CliToFile::read(&mut stream).await {
            Ok(CliToFile::PingRequest { ping_time }) => {
                let reply = FileToCli::PingReply { ping_time };
                send_message!(stream, reply);
            }
            Ok(CliToFile::BuildIdRequest { trans_id }) => {
                let reply = FileToCli::BuildIdReply {
                    trans_id,
                    result: NetResultCode::NetSuccess as i32,
                    build_id: server_config.build_id,
                };
                send_message!(stream, reply);
            }
            Ok(CliToFile::ManifestRequest { trans_id, manifest_name, build_id }) => {
                todo!()
            }
            Ok(CliToFile::DownloadRequest { trans_id, filename, build_id }) => {
                todo!()
            }
            Ok(CliToFile::ManifestEntryAck { trans_id: _, reader_id: _ }) => {
                // Ignored
                continue;
            }
            Ok(CliToFile::DownloadChunkAck { trans_id: _, reader_id: _ }) => {
                // Ignored
                continue;
            }
            Err(err) => {
                eprintln!("[File] Error reading message from client: {:?}", err);
                return;
            }
        }
    }
}

impl FileServer {
    pub fn start(server_config: Arc<ServerConfig>) -> FileServer {
        let (incoming_send, mut incoming_recv) = mpsc::channel(5);

        tokio::spawn(async move {
            while let Some(sock) = incoming_recv.recv().await {
                let server_config = server_config.clone();
                tokio::spawn(async move {
                    file_server_client(sock, server_config).await;
                });
            }
        });
        FileServer { incoming_send }
    }

    pub async fn add(&mut self, sock: TcpStream) {
        if let Err(err) = self.incoming_send.send(sock).await {
            eprintln!("[File] Failed to add client: {:?}", err);
            std::process::exit(1);
        }
    }
}
