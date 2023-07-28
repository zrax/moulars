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
use std::io::{BufRead, Cursor, ErrorKind, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use byteorder::{LittleEndian, ReadBytesExt};
use log::{error, warn, debug};
use tokio::io::{AsyncReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::general_error;
use crate::config::ServerConfig;
use crate::netcli::NetResultCode;
use crate::path_utils;
use super::messages::{CliToFile, FileToCli};
use super::manifest::Manifest;

pub struct FileServer {
    incoming_send: mpsc::Sender<TcpStream>,
}

const CONN_HEADER_SIZE: usize = 12;
const FILE_CHUNK_SIZE: usize = 64 * 1024;

fn read_conn_header<S>(stream: &mut S) -> Result<()>
    where S: BufRead
{
    // Everything here is discarded...
    let header_size = stream.read_u32::<LittleEndian>()?;
    if header_size != CONN_HEADER_SIZE as u32 {
        return Err(general_error!("Invalid connection header size {}", header_size));
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

async fn send_message(stream: &mut TcpStream, reply: FileToCli) -> bool {
    if let Err(err) = reply.write(stream).await {
        warn!("Failed to send reply message: {}", err);
        false
    } else {
        true
    }
}

fn fetch_manifest(manifest_name: &str, data_path: &Path) -> Option<Manifest> {
    if manifest_name.contains(|ch| ch == '/' || ch == '\\' || ch == ':' || ch == '.') {
        // Reject anything that looks like a path
        return None;
    }

    let manifest_path = data_path.join(manifest_name.to_owned() + ".mfs_cache");
    if manifest_path.exists() {
        match Manifest::from_cache(&manifest_path) {
            Ok(manifest) => Some(manifest),
            Err(err) => {
                warn!("Failed to load manifest '{}': {}", manifest_name, err);
                None
            }
        }
    } else {
        None
    }
}

async fn do_manifest(stream: &mut TcpStream, trans_id: u32, manifest_name: &str,
                     client_reader_id: &mut u32, data_root: &Path) -> bool
{
    let reply =
        if let Some(manifest) = fetch_manifest(manifest_name, data_root) {
            debug!("Client {} requested manifest '{}'",
                   stream.peer_addr().unwrap(), manifest_name);

            *client_reader_id += 1;
            FileToCli::ManifestReply {
                trans_id,
                result: NetResultCode::NetSuccess as i32,
                reader_id: *client_reader_id,
                manifest
            }
        } else {
            warn!("Client {} requested invalid/unknown manifest '{}'",
                  stream.peer_addr().unwrap(), manifest_name);
            FileToCli::manifest_error(trans_id, NetResultCode::NetFileNotFound)
        };

    send_message(stream, reply).await
}

pub fn ignore_file(path: &Path, allow_compressed: bool) -> bool {
    if let Some(ext) = path.extension() {
        if !allow_compressed && ext == OsStr::new("gz") {
            // We don't send the client .gz files to leave compressed,
            // so this is probably a compressed version of another file
            return true;
        }
    }

    if let Some(file_name) = path.file_name() {
        if file_name == OsStr::new("desktop.ini")
                || file_name.to_string_lossy().starts_with('.') {
            return true;
        }
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
            warn!("Could not open {} for reading: {}", download_path.display(), err);
            return None;
        }
    };

    let metadata = match file.metadata().await {
        Ok(metadata) => metadata,
        Err(err) => {
            warn!("Could not read file metadata for {}: {}", download_path.display(), err);
            return None;
        }
    };

    Some((file, metadata, download_path))
}

async fn do_download(stream: &mut TcpStream, trans_id: u32, filename: &str,
                     client_reader_id: &mut u32, data_root: &Path) -> bool
{
    if let Some((mut file, metadata, download_path))
                = open_server_file(filename, data_root).await
    {
        debug!("Client {} requested file '{}'", stream.peer_addr().unwrap(), filename);

        if metadata.len() > u32::MAX as u64 {
            debug!("File {} too large for 32-bit stream", filename);
            let reply = FileToCli::download_error(trans_id, NetResultCode::NetInternalError);
            return send_message(stream, reply).await;
        }

        *client_reader_id += 1;
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
                        reader_id: *client_reader_id,
                        total_size: metadata.len() as u32,
                        file_data: Vec::from(&buffer[..count]),
                    };
                    if !send_message(stream, reply).await {
                        return false;
                    }
                }
                Err(err) => {
                    warn!("Could not read from {}: {}", download_path.display(), err);
                    let reply = FileToCli::download_error(trans_id, NetResultCode::NetInternalError);
                    return send_message(stream, reply).await;
                }
            }
        }
    } else {
        warn!("Client {} requested invalid path '{}'", stream.peer_addr().unwrap(), filename);
        let reply = FileToCli::download_error(trans_id, NetResultCode::NetFileNotFound);
        send_message(stream, reply).await
    }
}

async fn file_server_client(client_sock: TcpStream, server_config: Arc<ServerConfig>) {
    let mut stream = match init_client(client_sock).await {
        Ok(stream) => stream,
        Err(err) => {
            warn!("Failed to initialize client: {}", err);
            return;
        }
    };

    // This monotonic ID is unique for each client, so we always start at 0
    let mut client_reader_id = 0;

    loop {
        match CliToFile::read(&mut stream).await {
            Ok(CliToFile::PingRequest { ping_time }) => {
                let reply = FileToCli::PingReply { ping_time };
                if !send_message(stream.get_mut(), reply).await {
                    return;
                }
            }
            Ok(CliToFile::BuildIdRequest { trans_id }) => {
                let reply = FileToCli::BuildIdReply {
                    trans_id,
                    result: NetResultCode::NetSuccess as i32,
                    build_id: server_config.build_id,
                };
                if !send_message(stream.get_mut(), reply).await {
                    return;
                }
            }
            Ok(CliToFile::ManifestRequest { trans_id, manifest_name, build_id }) => {
                if build_id != 0 && build_id != server_config.build_id {
                    warn!("Client {} has an unexpected build ID {}",
                          stream.get_ref().peer_addr().unwrap(), build_id);
                    let reply = FileToCli::manifest_error(trans_id, NetResultCode::NetOldBuildId);
                    if !send_message(stream.get_mut(), reply).await {
                        return;
                    }
                    continue;
                }
                if !do_manifest(stream.get_mut(), trans_id, &manifest_name,
                                &mut client_reader_id, &server_config.data_root).await
                {
                    return;
                }
            }
            Ok(CliToFile::DownloadRequest { trans_id, filename, build_id }) => {
                if build_id != 0 && build_id != server_config.build_id {
                    warn!("Client {} has an unexpected build ID {}",
                          stream.get_ref().peer_addr().unwrap(), build_id);
                    let reply = FileToCli::download_error(trans_id, NetResultCode::NetOldBuildId);
                    if !send_message(stream.get_mut(), reply).await {
                        return;
                    }
                    continue;
                }
                if !do_download(stream.get_mut(), trans_id, &filename,
                                &mut client_reader_id, &server_config.data_root).await
                {
                    return;
                }
            }
            Ok(CliToFile::ManifestEntryAck { .. }) => (),   // Ignored
            Ok(CliToFile::DownloadChunkAck { .. }) => (),   // Ignored
            Err(err) => {
                if matches!(err.kind(), ErrorKind::ConnectionReset | ErrorKind::UnexpectedEof) {
                    debug!("Client {} disconnected", stream.get_ref().peer_addr().unwrap());
                } else {
                    warn!("Error reading message from client: {}", err);
                }
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
            error!("Failed to add client: {}", err);
            std::process::exit(1);
        }
    }
}
