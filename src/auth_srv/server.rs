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
use std::path::{Path, PathBuf};
use std::sync::Arc;

use byteorder::{LittleEndian, ReadBytesExt};
use log::{error, warn, info, debug};
use rand::Rng;
use tokio::io::{AsyncReadExt, BufReader};
use tokio::sync::{mpsc, oneshot};
use tokio::net::TcpStream;
use uuid::Uuid;

use crate::general_error;
use crate::config::ServerConfig;
use crate::hashes::ShaDigest;
use crate::net_crypt::CryptTcpStream;
use crate::netcli::NetResultCode;
use crate::path_utils;
use crate::plasma::{StreamRead, StreamWrite, BitVector};
use crate::vault::{VaultMessage, VaultServer};
use super::auth_hash::{hash_password_challenge, use_email_auth};
use super::manifest::Manifest;
use super::messages::{CliToAuth, AuthToCli};

pub struct AuthServer {
    incoming_send: mpsc::Sender<TcpStream>,
}

const CONN_HEADER_SIZE: usize = 20;
const FILE_CHUNK_SIZE: usize = 64 * 1024;

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

async fn send_message(stream: &mut CryptTcpStream, reply: AuthToCli) -> bool {
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

async fn vault_recv<T>(recv: oneshot::Receiver<T>) -> Option<T> {
    match recv.await {
        Ok(response) => Some(response),
        Err(err) => {
            warn!("Failed to recieve response from Vault: {}", err);
            None
        }
    }
}

async fn init_client(mut sock: TcpStream, server_config: &ServerConfig)
    -> Result<BufReader<CryptTcpStream>>
{
    let mut header = [0u8; CONN_HEADER_SIZE];
    sock.read_exact(&mut header).await?;
    read_conn_header(&mut Cursor::new(header))?;

    let mut crypt_sock = crate::net_crypt::init_crypt(sock, &server_config.auth_n_key,
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

fn check_file_request(dir_name: &str, ext: &str) -> bool {
    (dir_name == "Python" && ext == "pak")
        || (dir_name == "SDL" && ext == "sdl")
}

fn fetch_list(dir_name: &str, ext: &str, data_root: &Path) -> Option<Manifest> {
    // Whitelist what the client is allowed to request.
    if !check_file_request(dir_name, ext) {
        return None;
    }

    match Manifest::from_dir(data_root, dir_name, ext) {
        Ok(manifest) => Some(manifest),
        Err(err) => {
            warn!("Failed to fetch directory list for {}\\*.{}: {}", dir_name, ext, err);
            None
        }
    }
}

async fn do_manifest(stream: &mut CryptTcpStream, trans_id: u32, dir_name: &str,
                     ext: &str, data_root: &Path) -> bool
{
    let reply =
        if let Some(manifest) = fetch_list(dir_name, ext, data_root) {
            debug!("Client {} requested list '{}\\*.{}'",
                   stream.peer_addr().unwrap(), dir_name, ext);

            AuthToCli::FileListReply {
                trans_id,
                result: NetResultCode::NetSuccess as i32,
                manifest
            }
        } else {
            warn!("Client {} requested invalid list '{}\\*.{}'",
                  stream.peer_addr().unwrap(), dir_name, ext);
            AuthToCli::FileListReply {
                trans_id,
                result: NetResultCode::NetFileNotFound as i32,
                manifest: Manifest::new()
            }
        };

    send_message(stream, reply).await
}

async fn open_server_file(filename: &str, data_root: &Path)
    -> Option<(tokio::fs::File, std::fs::Metadata, PathBuf)>
{
    let path_parts: Vec<&str> = filename.split('\\').collect();
    if path_parts.len() != 2 {
        // The requested path should be exactly "<dir>\<file>.<ext>"
        return None;
    }
    let native_path = path_utils::to_native(filename);
    let download_path = data_root.join(native_path);

    let ext = download_path.extension().unwrap_or_default();
    if !check_file_request(path_parts[0], &ext.to_string_lossy())
        || path_parts[1].starts_with('.')
    {
        // Ensure the requested file is whitelisted
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
            warn!("Could not read file metadata for {}: {}",download_path.display(), err);
            return None;
        }
    };

    Some((file, metadata, download_path))
}

async fn do_download(stream: &mut CryptTcpStream, trans_id: u32, filename: &str,
                     data_root: &Path) -> bool
{
    if let Some((mut file, metadata, download_path))
                = open_server_file(filename, data_root).await
    {
        debug!("Client {} requested file '{}'", stream.peer_addr().unwrap(), filename);

        if metadata.len() > u32::MAX as u64 {
            debug!("File {} too large for 32-bit stream", filename);
            let reply = AuthToCli::download_error(trans_id, NetResultCode::NetInternalError);
            return send_message(stream, reply).await;
        }

        let mut buffer = [0u8; FILE_CHUNK_SIZE];
        let mut offset = 0;
        loop {
            match file.read(&mut buffer).await {
                Ok(count) => {
                    if count == 0 {
                        // End of file reached
                        return true;
                    }
                    let reply = AuthToCli::FileDownloadChunk {
                        trans_id,
                        result: NetResultCode::NetSuccess as i32,
                        total_size: metadata.len() as u32,
                        offset,
                        file_data: Vec::from(&buffer[..count]),
                    };
                    if !send_message(stream, reply).await {
                        return false;
                    }
                    offset += count as u32;
                }
                Err(err) => {
                    warn!("Could not read from {}: {}", download_path.display(), err);
                    let reply = AuthToCli::download_error(trans_id, NetResultCode::NetInternalError);
                    return send_message(stream, reply).await;
                }
            }
        }
    } else {
        warn!("Client {} requested invalid path '{}'", stream.peer_addr().unwrap(), filename);
        let reply = AuthToCli::download_error(trans_id, NetResultCode::NetFileNotFound);
        send_message(stream, reply).await
    }
}

#[allow(clippy::too_many_arguments)]
async fn do_login_request(stream: &mut CryptTcpStream, trans_id: u32,
                          client_challenge: u32, server_challenge: u32,
                          account_name: &str, pass_hash: ShaDigest,
                          server_config: &ServerConfig, vault: &VaultServer) -> bool
{
    let (response_send, response_recv) = oneshot::channel();
    let request = VaultMessage::GetAccount {
        account_name: account_name.to_string(),
        response_send
    };
    vault.send(request).await;

    if let Some(response) = vault_recv(response_recv).await {
        let account = match response {
            Some(account) => account,
            None => {
                info!("{}: Account {} was not found",
                      stream.peer_addr().unwrap(), account_name);

                // Don't leak to the client that the account doesn't exist...
                let reply = AuthToCli::login_error(trans_id,
                                NetResultCode::NetAuthenticationFailed);
                return send_message(stream, reply).await;
            }
        };

        // NOTE: Neither of these is good or secure, but they are what the
        // client expects.  To fix these, we'd have to break compatibility
        // with older clients.
        if use_email_auth(account_name) {
            // Use broken LE Sha0 hash mechanism
            let challenge_hash = match hash_password_challenge(
                        client_challenge, server_challenge, account.pass_hash)
            {
                Ok(digest) => digest,
                Err(err) => {
                    warn!("Failed to generate challenge hash: {}", err);
                    let reply = AuthToCli::login_error(trans_id,
                                    NetResultCode::NetInternalError);
                    return send_message(stream, reply).await;
                }
            };
            if challenge_hash != pass_hash {
                info!("{}: Login failure for account {}",
                      stream.peer_addr().unwrap(), account_name);
                let reply = AuthToCli::login_error(trans_id,
                                NetResultCode::NetAuthenticationFailed);
                return send_message(stream, reply).await;
            }
        } else {
            // Directly compare the BE Sha1 hash
            // NOTE: The client sends its hash as Little Endian...
            if account.pass_hash != pass_hash.endian_swap() {
                info!("{}: Login failure for account {}",
                      stream.peer_addr().unwrap(), account_name);
                let reply = AuthToCli::login_error(trans_id,
                                NetResultCode::NetAuthenticationFailed);
                return send_message(stream, reply).await;
            }
        }

        if account.is_banned() {
            info!("{}: Account {} is banned", stream.peer_addr().unwrap(), account_name);
            let reply = AuthToCli::login_error(trans_id, NetResultCode::NetAccountBanned);
            return send_message(stream, reply).await;
        }
        if server_config.restrict_logins && !account.can_login_restricted() {
            info!("{}: Account {} login is restricted", stream.peer_addr().unwrap(),
                  account_name);
            let reply = AuthToCli::login_error(trans_id, NetResultCode::NetLoginDenied);
            return send_message(stream, reply).await;
        }

        let ntd_key = match server_config.get_ntd_key() {
            Ok(key) => key,
            Err(err) => {
                warn!("Failed to get encryption key: {}", err);
                let reply = AuthToCli::login_error(trans_id, NetResultCode::NetInternalError);
                return send_message(stream, reply).await;
            }
        };

        info!("{}: Logged in as {} {}", stream.peer_addr().unwrap(),
              account_name, account.account_id);

        if !fetch_account_players(stream, trans_id, &account.account_id, vault).await {
            return false;
        }

        // Send the final reply after all players are sent
        let reply = AuthToCli::AcctLoginReply {
            trans_id,
            result: NetResultCode::NetSuccess as i32,
            account_id: account.account_id,
            account_flags: account.account_flags,
            billing_type: account.billing_type,
            encryption_key: ntd_key,
        };
        send_message(stream, reply).await
    } else {
        false
    }
}

async fn fetch_account_players(stream: &mut CryptTcpStream, trans_id: u32,
                               account_id: &Uuid, vault: &VaultServer) -> bool
{
    let (response_send, response_recv) = oneshot::channel();
    let request = VaultMessage::GetPlayers {
        account_id: *account_id,
        response_send
    };
    vault.send(request).await;

    if let Some(response) = vault_recv(response_recv).await {
        for player in response {
            let msg = AuthToCli::AcctPlayerInfo {
                trans_id,
                player_id: player.player_id,
                player_name: player.player_name,
                avatar_shape: player.avatar_shape,
                explorer: player.explorer,
            };
            if !send_message(stream, msg).await {
                return false;
            }
        }
        true
    } else {
        false
    }
}

async fn auth_client(client_sock: TcpStream, server_config: Arc<ServerConfig>,
                     vault: Arc<VaultServer>)
{
    let mut stream = match init_client(client_sock, &server_config).await {
        Ok(cipher) => cipher,
        Err(err) => {
            warn!("Failed to initialize client: {}", err);
            return;
        }
    };

    let server_challenge = rand::thread_rng().gen::<u32>();

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
            Ok(CliToAuth::ClientRegisterRequest { build_id }) => {
                if build_id != 0 && build_id != server_config.build_id {
                    warn!("Client {} has an unexpected build ID {}",
                          stream.get_ref().peer_addr().unwrap(), build_id);
                    // The client isn't listening for anything other than a
                    // ClientRegisterReply, which doesn't have a result field,
                    // so we can't notify them that their build is invalid...
                    return;
                }
                let reply = AuthToCli::ClientRegisterReply { server_challenge };
                if !send_message(stream.get_mut(), reply).await {
                    return;
                }
            }
            Ok(CliToAuth::ClientSetCCRLevel { .. }) => {
                warn!("Ignoring CCR level set request from {}",
                      stream.get_ref().peer_addr().unwrap());
            }
            Ok(CliToAuth::AcctLoginRequest { trans_id, client_challenge, account_name,
                                             pass_hash, auth_token, os }) => {
                debug!("Login Request U:{} P:{} T:{} O:{}", account_name,
                       pass_hash.as_hex(), auth_token, os);
                if !do_login_request(stream.get_mut(), trans_id, client_challenge,
                                     server_challenge, &account_name, pass_hash,
                                     server_config.as_ref(), vault.as_ref()).await
                {
                    return;
                }
            }
            Ok(CliToAuth::AcctSetPlayerRequest { trans_id, player_id }) => {
                // Setting no player (player_id = 0) is always successful
                if player_id == 0 {
                    let reply = AuthToCli::AcctSetPlayerReply {
                        trans_id,
                        result: NetResultCode::NetSuccess as i32,
                    };
                    if !send_message(stream.get_mut(), reply).await {
                        return;
                    }
                } else {
                    todo!()
                }
            }
            Ok(CliToAuth::AcctCreateRequest { trans_id, .. }) => {
                let reply = AuthToCli::AcctCreateReply {
                    trans_id,
                    result: NetResultCode::NetNotSupported as i32,
                    account_id: Uuid::nil(),
                };
                if !send_message(stream.get_mut(), reply).await {
                    return;
                }
            }
            Ok(CliToAuth::AcctChangePasswordRequest { trans_id, .. }) => {
                let reply = AuthToCli::AcctChangePasswordReply {
                    trans_id,
                    result: NetResultCode::NetNotSupported as i32,
                };
                if !send_message(stream.get_mut(), reply).await {
                    return;
                }
            }
            Ok(CliToAuth::AcctSetRolesRequest { trans_id, .. }) => {
                let reply = AuthToCli::AcctSetRolesReply {
                    trans_id,
                    result: NetResultCode::NetNotSupported as i32,
                };
                if !send_message(stream.get_mut(), reply).await {
                    return;
                }
            }
            Ok(CliToAuth::AcctSetBillingTypeRequest { trans_id, .. }) => {
                let reply = AuthToCli::AcctSetRolesReply {
                    trans_id,
                    result: NetResultCode::NetNotSupported as i32,
                };
                if !send_message(stream.get_mut(), reply).await {
                    return;
                }
            }
            Ok(CliToAuth::AcctActivateRequest { trans_id, .. }) => {
                let reply = AuthToCli::AcctActivateReply {
                    trans_id,
                    result: NetResultCode::NetNotSupported as i32,
                };
                if !send_message(stream.get_mut(), reply).await {
                    return;
                }
            }
            Ok(CliToAuth::AcctCreateFromKeyRequest { trans_id, .. }) => {
                let reply = AuthToCli::AcctCreateFromKeyReply {
                    trans_id,
                    result: NetResultCode::NetNotSupported as i32,
                    account_id: Uuid::nil(),
                    activation_key: Uuid::nil(),
                };
                if !send_message(stream.get_mut(), reply).await {
                    return;
                }
            }
            Ok(CliToAuth::PlayerDeleteRequest { .. }) => {
                todo!()
            }
            Ok(CliToAuth::PlayerCreateRequest { .. }) => {
                todo!()
            }
            Ok(CliToAuth::UpgradeVisitorRequest { trans_id, .. }) => {
                let reply = AuthToCli::UpgradeVisitorReply {
                    trans_id,
                    result: NetResultCode::NetNotSupported as i32,
                };
                if !send_message(stream.get_mut(), reply).await {
                    return;
                }
            }
            Ok(CliToAuth::SetPlayerBanStatusRequest { trans_id, .. }) => {
                warn!("Rejecting ban request from {}",
                      stream.get_ref().peer_addr().unwrap());
                let reply = AuthToCli::SetPlayerBanStatusReply {
                    trans_id,
                    result: NetResultCode::NetServiceForbidden as i32,
                };
                if !send_message(stream.get_mut(), reply).await {
                    return;
                }
            }
            Ok(CliToAuth::KickPlayer { .. }) => {
                warn!("Ignoring kick player request from {}",
                      stream.get_ref().peer_addr().unwrap());
            }
            Ok(CliToAuth::ChangePlayerNameRequest { .. }) => {
                todo!()
            }
            Ok(CliToAuth::SendFriendInviteRequest { .. }) => {
                todo!()
            }
            Ok(CliToAuth::VaultNodeCreate { .. }) => {
                todo!()
            }
            Ok(CliToAuth::VaultNodeFetch { .. }) => {
                todo!()
            }
            Ok(CliToAuth::VaultNodeSave { .. }) => {
                todo!()
            }
            Ok(CliToAuth::VaultNodeDelete { .. }) => {
                todo!()
            }
            Ok(CliToAuth::VaultNodeAdd { .. }) => {
                todo!()
            }
            Ok(CliToAuth::VaultNodeRemove { .. }) => {
                todo!()
            }
            Ok(CliToAuth::VaultFetchNodeRefs { .. }) => {
                todo!()
            }
            Ok(CliToAuth::VaultInitAgeRequest { .. }) => {
                todo!()
            }
            Ok(CliToAuth::VaultNodeFind { .. }) => {
                todo!()
            }
            Ok(CliToAuth::VaultSetSeen { .. }) => {
                todo!()
            }
            Ok(CliToAuth::VaultSendNode { .. }) => {
                todo!()
            }
            Ok(CliToAuth::AgeRequest { .. }) => {
                todo!()
            }
            Ok(CliToAuth::FileListRequest { trans_id, directory, ext }) => {
                if !do_manifest(stream.get_mut(), trans_id, &directory, &ext,
                                &server_config.data_root).await
                {
                    return;
                }
            }
            Ok(CliToAuth::FileDownloadRequest { trans_id, filename }) => {
                if !do_download(stream.get_mut(), trans_id, &filename,
                                &server_config.data_root).await
                {
                    return;
                }
            }
            Ok(CliToAuth::FileDownloadChunkAck { .. }) => (),   // Ignored
            Ok(CliToAuth::PropagateBuffer { .. }) => {
                warn!("Ignoring propagate buffer from {}",
                      stream.get_ref().peer_addr().unwrap());
            }
            Ok(CliToAuth::GetPublicAgeList { .. }) => {
                todo!()
            }
            Ok(CliToAuth::SetAgePublic { .. }) => {
                todo!()
            }
            Ok(CliToAuth::LogPythonTraceback { traceback }) => {
                warn!("Python Traceback from {}:\n{}",
                      stream.get_ref().peer_addr().unwrap(), traceback);
            }
            Ok(CliToAuth::LogStackDump { stackdump }) => {
                warn!("Stack Dump from {}:\n{}",
                      stream.get_ref().peer_addr().unwrap(), stackdump);
            }
            Ok(CliToAuth::LogClientDebuggerConnect { .. }) => (),   // Ignored
            Ok(CliToAuth::ScoreCreate { .. }) => {
                todo!()
            }
            Ok(CliToAuth::ScoreDelete { .. }) => {
                todo!()
            }
            Ok(CliToAuth::ScoreGetScores { .. }) => {
                todo!()
            }
            Ok(CliToAuth::ScoreAddPoints { .. }) => {
                todo!()
            }
            Ok(CliToAuth::ScoreTransferPoints { .. }) => {
                todo!()
            }
            Ok(CliToAuth::ScoreSetPoints { .. }) => {
                todo!()
            }
            Ok(CliToAuth::ScoreGetRanks { .. }) => {
                todo!()
            }
            Ok(CliToAuth::AccountExistsRequest { .. }) => {
                todo!()
            }
            Ok(CliToAuth::ScoreGetHighScores { .. }) => {
                todo!()
            }
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

impl AuthServer {
    pub fn start(server_config: Arc<ServerConfig>, vault: Arc<VaultServer>)
        -> AuthServer
    {
        let (incoming_send, mut incoming_recv) = mpsc::channel(5);

        tokio::spawn(async move {
            while let Some(sock) = incoming_recv.recv().await {
                let server_config = server_config.clone();
                let vault = vault.clone();
                tokio::spawn(async move {
                    auth_client(sock, server_config, vault).await;
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
