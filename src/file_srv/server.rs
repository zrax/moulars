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

use std::io::{BufRead, Cursor, Result};
use std::path::Path;
use std::sync::Arc;

use byteorder::{LittleEndian, ReadBytesExt};
use tokio::io::{AsyncReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::general_error;
use crate::config::ServerConfig;
use crate::netcli::NetResultCode;
use super::messages::{CliToFile, FileToCli};
use super::manifest::Manifest;

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
        return Err(general_error!("[File] Invalid connection header size {}", header_size));
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

fn fetch_manifest(manifest_name: &String, data_path: &Path) -> Option<Manifest> {
    if manifest_name.contains(|ch| ch == '/' || ch == '\\' || ch == ':' || ch == '.') {
        // Reject anything that looks like a path
        return None;
    }

    let manifest_path = data_path.join(manifest_name.to_owned() + ".mfs_cache");
    if manifest_path.exists() {
        match Manifest::from_cache(&manifest_path) {
            Ok(manifest) => Some(manifest),
            Err(err) => {
                eprintln!("[File] Failed to load manifest '{}': {:?}",
                          manifest_name, err);
                None
            }
        }
    } else {
        None
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

    // This monotonic ID is unique for each client, so we always start at 0
    let mut client_reader_id = 0;

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
                if build_id != 0 && build_id != server_config.build_id {
                    eprintln!("[File] Client {} has an unexpected build ID {}",
                              stream.get_ref().peer_addr().unwrap(), build_id);
                    let reply = FileToCli::ManifestReply {
                        trans_id,
                        result: NetResultCode::NetOldBuildId as i32,
                        reader_id: 0,
                        manifest: Manifest::new()
                    };
                    send_message!(stream, reply);
                    continue;
                }

                let reply =
                    if let Some(manifest) = fetch_manifest(&manifest_name,
                                                           &server_config.file_data_root)
                {
                    println!("[File] Client {} requested manifest '{}'",
                             stream.get_ref().peer_addr().unwrap(), manifest_name);

                    client_reader_id += 1;
                    FileToCli::ManifestReply {
                        trans_id,
                        result: NetResultCode::NetSuccess as i32,
                        reader_id: client_reader_id,
                        manifest
                    }
                } else {
                    eprintln!("[File] Client {} requested invalid/unknown manifest '{}'",
                              stream.get_ref().peer_addr().unwrap(), manifest_name);

                    FileToCli::ManifestReply {
                        trans_id,
                        result: NetResultCode::NetFileNotFound as i32,
                        reader_id: 0,
                        manifest: Manifest::new(),
                    }
                };

                send_message!(stream, reply);
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
