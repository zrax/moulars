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

use std::ffi::OsStr;
use std::io::{self, BufRead, Cursor};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use byteorder::{LittleEndian, ReadBytesExt};
use tokio::io::{AsyncReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tracing::{error, warn, debug};

use crate::config::ServerConfig;
use crate::netcli::NetResultCode;
use crate::path_utils;
use super::messages::{CliToFile, FileToCli};
use super::manifest::Manifest;

pub struct FileServer {
    incoming_send: mpsc::Sender<TcpStream>,
}

struct FileServerWorker {
    stream: BufReader<TcpStream>,
    server_config: Arc<ServerConfig>,
    client_reader_id: u32,
}

const CONN_HEADER_SIZE: u32 = 12;
const FILE_CHUNK_SIZE: usize = 64 * 1024;

fn read_conn_header<S>(stream: &mut S) -> Result<()>
    where S: BufRead
{
    // Everything here is discarded...
    let header_size = stream.read_u32::<LittleEndian>()?;
    if header_size != CONN_HEADER_SIZE {
        return Err(anyhow!("Invalid connection header size {header_size}"));
    }
    // Build ID
    let _ = stream.read_u32::<LittleEndian>()?;
    // Server Type
    let _ = stream.read_u32::<LittleEndian>()?;

    Ok(())
}

async fn init_client(mut sock: TcpStream) -> Result<BufReader<TcpStream>> {
    let mut buffer = [0u8; CONN_HEADER_SIZE as usize];
    sock.read_exact(&mut buffer).await?;
    read_conn_header(&mut Cursor::new(buffer))?;

    Ok(BufReader::new(sock))
}

fn fetch_manifest(manifest_name: &str, data_path: &Path) -> Option<Manifest> {
    if manifest_name.contains(['/', '\\', ':', '.']) {
        // Reject anything that looks like a path
        return None;
    }

    let manifest_path = data_path.join(manifest_name.to_owned() + ".mfs_cache");
    if manifest_path.exists() {
        match Manifest::from_cache(&manifest_path) {
            Ok(manifest) => Some(manifest),
            Err(err) => {
                warn!("Failed to load manifest '{manifest_name}': {err}");
                None
            }
        }
    } else {
        None
    }
}

pub fn ignore_file(path: &Path, allow_compressed: bool) -> bool {
    if !allow_compressed && let Some(ext) = path.extension() && ext == OsStr::new("gz") {
        // We don't send the client .gz files to leave compressed,
        // so this is probably a compressed version of another file
        return true;
    }

    if let Some(file_name) = path.file_name()
        && (file_name == OsStr::new("desktop.ini")
            || file_name.to_string_lossy().starts_with('.'))
    {
        return true;
    }

    false
}

async fn open_server_file(filename: &str, data_root: &Path)
    -> Option<(tokio::fs::File, std::fs::Metadata, PathBuf)>
{
    let native_path = path_utils::to_native(filename);
    let download_path = data_root.join(native_path);

    // Reject path traversal attempts, hidden/ignored files,
    // and check if the file exists.
    if filename.contains("..")
        || !download_path.starts_with(data_root)
        || !download_path.exists()
        || ignore_file(&download_path, true)
    {
        return None;
    }

    let file = match tokio::fs::File::open(&download_path).await {
        Ok(file) => file,
        Err(err) => {
            warn!("Could not open {} for reading: {err}", download_path.display());
            return None;
        }
    };

    let metadata = match file.metadata().await {
        Ok(metadata) => metadata,
        Err(err) => {
            warn!("Could not read file metadata for {}: {err}", download_path.display());
            return None;
        }
    };

    Some((file, metadata, download_path))
}

impl FileServer {
    pub fn start(server_config: Arc<ServerConfig>) -> FileServer {
        let (incoming_send, mut incoming_recv) = mpsc::channel(5);

        tokio::spawn(async move {
            while let Some(sock) = incoming_recv.recv().await {
                FileServerWorker::start(sock, server_config.clone());
            }
        });
        FileServer { incoming_send }
    }

    pub async fn add(&mut self, sock: TcpStream) {
        if let Err(err) = self.incoming_send.send(sock).await {
            error!("Failed to add client: {err}");
        }
    }
}

impl FileServerWorker {
    pub fn start(sock: TcpStream, server_config: Arc<ServerConfig>) {
        tokio::spawn(async move {
            let stream = match init_client(sock).await {
                Ok(stream) => stream,
                Err(err) => {
                    warn!("Failed to initialize client: {err}");
                    return;
                }
            };

            let mut worker = FileServerWorker {
                stream,
                server_config,
                // This monotonic ID is unique for each client, so we always start at 0
                client_reader_id: 0,
            };
            worker.run().await;
        });
    }

    fn peer_addr(&self) -> io::Result<SocketAddr> { self.stream.get_ref().peer_addr() }

    async fn run(&mut self) {
        loop {
            match CliToFile::read(&mut self.stream).await {
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

    async fn handle_message(&mut self, message: CliToFile) -> bool {
        match message {
            CliToFile::PingRequest { ping_time } => {
                self.send_message(FileToCli::PingReply { ping_time }).await
            }
            CliToFile::BuildIdRequest { trans_id } => {
                self.send_message(FileToCli::BuildIdReply {
                    trans_id,
                    result: NetResultCode::NetSuccess as i32,
                    build_id: self.server_config.build_id,
                }).await
            }
            CliToFile::ManifestRequest { trans_id, manifest_name, build_id } => {
                if build_id != 0 && build_id != self.server_config.build_id {
                    warn!("Client {} has an unexpected build ID {build_id}",
                          self.peer_addr().unwrap());
                    return self.send_message(FileToCli::manifest_error(trans_id,
                                                NetResultCode::NetOldBuildId)).await;
                }
                self.do_manifest(trans_id, &manifest_name).await
            }
            CliToFile::DownloadRequest { trans_id, filename, build_id } => {
                if build_id != 0 && build_id != self.server_config.build_id {
                    warn!("Client {} has an unexpected build ID {build_id}",
                          self.peer_addr().unwrap());
                    return self.send_message(FileToCli::download_error(trans_id,
                                                NetResultCode::NetOldBuildId)).await;
                }
                Box::pin(self.do_download(trans_id, &filename)).await
            }
            CliToFile::ManifestEntryAck { trans_id, reader_id }
                    | CliToFile::DownloadChunkAck { trans_id, reader_id } => {
                // Ignored
                let _ = trans_id;
                let _ = reader_id;
                true
            }
        }
    }

    async fn send_message(&mut self, reply: FileToCli) -> bool {
        if let Err(err) = reply.write(self.stream.get_mut()).await {
            warn!("Failed to send reply message: {err}");
            false
        } else {
            true
        }
    }

    async fn do_manifest(&mut self, trans_id: u32, manifest_name: &str) -> bool {
        let reply = if let Some(manifest)
                            = fetch_manifest(manifest_name, &self.server_config.data_root)
        {
            debug!("Client {} requested manifest '{manifest_name}'",
                   self.peer_addr().unwrap());

            self.client_reader_id += 1;
            FileToCli::ManifestReply {
                trans_id,
                result: NetResultCode::NetSuccess as i32,
                reader_id: self.client_reader_id,
                manifest
            }
        } else {
            warn!("Client {} requested invalid/unknown manifest '{manifest_name}'",
                  self.peer_addr().unwrap());
            FileToCli::manifest_error(trans_id, NetResultCode::NetFileNotFound)
        };

        self.send_message(reply).await
    }

    async fn do_download(&mut self, trans_id: u32, filename: &str) -> bool {
        if let Some((mut file, metadata, download_path))
                    = open_server_file(filename, &self.server_config.data_root).await
        {
            debug!("Client {} requested file '{filename}'", self.peer_addr().unwrap());

            let Ok(total_size) = u32::try_from(metadata.len()) else {
                debug!("File {filename} too large for 32-bit stream");
                return self.send_message(FileToCli::download_error(trans_id,
                                            NetResultCode::NetInternalError)).await;
            };

            self.client_reader_id += 1;
            #[allow(clippy::large_stack_arrays)]
            let mut buffer = [0u8; FILE_CHUNK_SIZE];
            loop {
                match file.read(&mut buffer).await {
                    Ok(count) => {
                        if count == 0 {
                            // End of file reached
                            return true;
                        }
                        let reply = FileToCli::FileDownloadReply {
                            trans_id,
                            result: NetResultCode::NetSuccess as i32,
                            reader_id: self.client_reader_id,
                            total_size,
                            file_data: Vec::from(&buffer[..count]),
                        };
                        if !self.send_message(reply).await {
                            return false;
                        }
                    }
                    Err(err) => {
                        warn!("Could not read from {}: {}", download_path.display(), err);
                        return self.send_message(FileToCli::download_error(trans_id,
                                                    NetResultCode::NetInternalError)).await;
                    }
                }
            }
        } else {
            warn!("Client {} requested invalid path '{}'", self.peer_addr().unwrap(),
                  filename);
            self.send_message(FileToCli::download_error(trans_id,
                                NetResultCode::NetFileNotFound)).await
        }
    }
}
