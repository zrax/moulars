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
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use byteorder::{LittleEndian, ReadBytesExt};
use log::{error, warn, info, debug};
use tokio::io::{AsyncReadExt, BufReader};
use tokio::sync::{mpsc, broadcast};
use tokio::net::TcpStream;
use uuid::Uuid;

use crate::config::ServerConfig;
use crate::hashes::ShaDigest;
use crate::net_crypt::CryptTcpStream;
use crate::netcli::NetResultCode;
use crate::path_utils;
use crate::plasma::{StreamRead, StreamWrite, BitVector};
use crate::vault::{VaultServer, VaultNode, VaultPlayerInfoNode};
use crate::vault::messages::VaultBroadcast;
use super::auth_hash::{hash_password_challenge, use_email_auth};
use super::manifest::Manifest;
use super::messages::{CliToAuth, AuthToCli};
use super::vault_helpers::{create_player_nodes, find_age_instance};

pub struct AuthServer {
    incoming_send: mpsc::Sender<TcpStream>,
}

struct AuthServerWorker {
    stream: BufReader<CryptTcpStream>,
    server_config: Arc<ServerConfig>,
    vault: Arc<VaultServer>,
    vault_bcast: broadcast::Receiver<VaultBroadcast>,
    server_challenge: u32,
    account_id: Option<Uuid>,
    player_id: Option<u32>,
}

const CONN_HEADER_SIZE: u32 = 20;
const FILE_CHUNK_SIZE: usize = 64 * 1024;

enum ServerCaps {
    ScoreLeaderBoards,
}

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

    crate::net_crypt::init_crypt(sock, &server_config.auth_n_key,
                                 &server_config.auth_k_key).await
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
            warn!("Failed to fetch directory list for {dir_name}\\*.{ext}: {err}");
            None
        }
    }
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

impl AuthServer {
    pub fn start(server_config: Arc<ServerConfig>, vault: Arc<VaultServer>)
        -> AuthServer
    {
        let (incoming_send, mut incoming_recv) = mpsc::channel(5);

        tokio::spawn(async move {
            while let Some(sock) = incoming_recv.recv().await {
                AuthServerWorker::start(sock, server_config.clone(), vault.clone());
            }
        });
        AuthServer { incoming_send }
    }

    pub async fn add(&mut self, sock: TcpStream) {
        if let Err(err) = self.incoming_send.send(sock).await {
            error!("Failed to add client: {err}");
        }
    }
}

impl AuthServerWorker {
    pub fn start(sock: TcpStream, server_config: Arc<ServerConfig>,
                 vault: Arc<VaultServer>)
    {
        tokio::spawn(async move {
            let stream = match init_client(sock, &server_config).await {
                Ok(cipher) => cipher,
                Err(err) => {
                    warn!("Failed to initialize client: {err}");
                    return;
                }
            };

            let vault_bcast = vault.subscribe();
            let mut worker = AuthServerWorker {
                stream,
                server_config,
                vault,
                vault_bcast,
                server_challenge: rand::random::<u32>(),
                account_id: None,
                player_id: None,
            };
            worker.run().await;
            worker.handle_disconnect().await;
        });
    }

    fn peer_addr(&self) -> io::Result<SocketAddr> { self.stream.get_ref().peer_addr() }

    async fn send_caps(&mut self) -> Result<()> {
        let mut caps = BitVector::new();
        caps.set(ServerCaps::ScoreLeaderBoards as usize, true);
        let mut caps_buffer = Cursor::new(Vec::new());
        caps.stream_write(&mut caps_buffer)?;
        let caps_msg = AuthToCli::ServerCaps {
            caps_buffer: caps_buffer.into_inner()
        };
        if !self.send_message(caps_msg).await {
            return Err(anyhow!("Failed to send message to client"));
        }
        Ok(())
    }

    async fn run(&mut self) {
        /* Send Server Capabilities */
        if let Err(err) = self.send_caps().await {
            warn!("Failed to send ServerCaps message: {err}");
        }

        loop {
            tokio::select! {
                // Drain any broadcast messages first, to avoid the broadcast
                // queue from filling up and starving.
                biased;

                bcast_msg = self.vault_bcast.recv() => match bcast_msg {
                    Ok(msg) => {
                        if !self.handle_bcast_msg(msg).await {
                            break;
                        }
                    }
                    Err(err) => warn!("Failed to receive broadcast message: {err}"),
                },

                client_msg = CliToAuth::read(&mut self.stream) => match client_msg {
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
                },
            }
        }
        warn!("Dropping client {}", self.peer_addr().unwrap());
    }

    async fn handle_bcast_msg(&mut self, bcast_msg: VaultBroadcast) -> bool {
        match bcast_msg {
            VaultBroadcast::NodeChanged { node_id, revision_id } => {
                // TODO: Only if we care about this node...
                self.send_message(AuthToCli::VaultNodeChanged {
                    node_id, revision_id,
                }).await
            }
            VaultBroadcast::NodeAdded { parent_id, child_id, owner_id } => {
                // TODO: Only if we care about this node...
                self.send_message(AuthToCli::VaultNodeAdded {
                    parent_id, child_id, owner_id
                }).await
            }
        }
    }

    async fn handle_message(&mut self, message: CliToAuth) -> bool {
        match message {
            CliToAuth::PingRequest { trans_id, ping_time, payload } => {
                self.send_message(AuthToCli::PingReply {
                    trans_id, ping_time, payload
                }).await
            }
            CliToAuth::ClientRegisterRequest { build_id } => {
                if build_id != 0 && build_id != self.server_config.build_id {
                    warn!("Client {} has an unexpected build ID {}",
                          self.peer_addr().unwrap(), build_id);
                    // The client isn't listening for anything other than a
                    // ClientRegisterReply, which doesn't have a result field,
                    // so we can't notify them that their build is invalid...
                    return false;
                }
                self.send_message(AuthToCli::ClientRegisterReply {
                    server_challenge: self.server_challenge,
                }).await
            }
            CliToAuth::ClientSetCCRLevel { .. } => {
                warn!("Ignoring CCR level set request from {}", self.peer_addr().unwrap());
                true
            }
            CliToAuth::AcctLoginRequest { trans_id, client_challenge, account_name,
                                          pass_hash, auth_token, os } => {
                debug!("Login Request U:{} P:{} T:{} O:{}", account_name,
                       pass_hash.as_hex(), auth_token, os);
                self.do_login_request(trans_id, client_challenge, &account_name,
                                      pass_hash).await
            }
            CliToAuth::AcctSetPlayerRequest { trans_id, player_id } => {
                if player_id == 0 {
                    // Setting no player (player_id = 0) is always successful
                    return self.send_message(AuthToCli::AcctSetPlayerReply {
                        trans_id,
                        result: NetResultCode::NetSuccess as i32,
                    }).await;
                }
                self.do_set_player(trans_id, player_id).await
            }
            CliToAuth::AcctCreateRequest { trans_id, .. } => {
                self.send_message(AuthToCli::AcctCreateReply {
                    trans_id,
                    result: NetResultCode::NetNotSupported as i32,
                    account_id: Uuid::nil(),
                }).await
            }
            CliToAuth::AcctChangePasswordRequest { trans_id, .. } => {
                self.send_message(AuthToCli::AcctChangePasswordReply {
                    trans_id,
                    result: NetResultCode::NetNotSupported as i32,
                }).await
            }
            CliToAuth::AcctSetRolesRequest { trans_id, .. } => {
                self.send_message(AuthToCli::AcctSetRolesReply {
                    trans_id,
                    result: NetResultCode::NetNotSupported as i32,
                }).await
            }
            CliToAuth::AcctSetBillingTypeRequest { trans_id, .. } => {
                self.send_message(AuthToCli::AcctSetBillingTypeReply {
                    trans_id,
                    result: NetResultCode::NetNotSupported as i32,
                }).await
            }
            CliToAuth::AcctActivateRequest { trans_id, .. } => {
                self.send_message(AuthToCli::AcctActivateReply {
                    trans_id,
                    result: NetResultCode::NetNotSupported as i32,
                }).await
            }
            CliToAuth::AcctCreateFromKeyRequest { trans_id, .. } => {
                self.send_message(AuthToCli::AcctCreateFromKeyReply {
                    trans_id,
                    result: NetResultCode::NetNotSupported as i32,
                    account_id: Uuid::nil(),
                    activation_key: Uuid::nil(),
                }).await
            }
            CliToAuth::PlayerDeleteRequest { .. } => {
                todo!()
            }
            CliToAuth::PlayerCreateRequest { trans_id, player_name, avatar_shape, .. } => {
                self.player_create(trans_id, &player_name, &avatar_shape).await
            }
            CliToAuth::UpgradeVisitorRequest { trans_id, .. } => {
                self.send_message(AuthToCli::UpgradeVisitorReply {
                    trans_id,
                    result: NetResultCode::NetNotSupported as i32,
                }).await
            }
            CliToAuth::SetPlayerBanStatusRequest { trans_id, .. } => {
                warn!("Rejecting ban request from {}", self.peer_addr().unwrap());
                self.send_message(AuthToCli::SetPlayerBanStatusReply {
                    trans_id,
                    result: NetResultCode::NetServiceForbidden as i32,
                }).await
            }
            CliToAuth::KickPlayer { .. } => {
                warn!("Ignoring kick player request from {}", self.peer_addr().unwrap());
                true
            }
            CliToAuth::ChangePlayerNameRequest { .. } => {
                todo!()
            }
            CliToAuth::SendFriendInviteRequest { .. } => {
                todo!()
            }
            CliToAuth::VaultNodeCreate { trans_id, node_buffer } => {
                let reply = match VaultNode::from_blob(&node_buffer) {
                    Ok(node) => match self.vault.create_node(node).await {
                        Ok(node_id) => AuthToCli::VaultNodeCreated {
                            trans_id,
                            result: NetResultCode::NetSuccess as i32,
                            node_id
                        },
                        Err(err) => AuthToCli::VaultNodeCreated {
                            trans_id,
                            result: err as i32,
                            node_id: 0
                        },
                    }
                    Err(err) => {
                        warn!("Failed to read vault node from blob: {err}");
                        AuthToCli::VaultNodeCreated {
                            trans_id,
                            result: NetResultCode::NetInternalError as i32,
                            node_id: 0
                        }
                    }
                };
                self.send_message(reply).await
            }
            CliToAuth::VaultNodeFetch { trans_id, node_id } => {
                let reply = match self.vault.fetch_node(node_id).await {
                    Ok(node) => match node.to_blob() {
                        Ok(node_buffer) => AuthToCli::VaultNodeFetched {
                            trans_id,
                            result: NetResultCode::NetSuccess as i32,
                            node_buffer
                        },
                        Err(err) => {
                            warn!("Failed to write vault node to blob: {err}");
                            AuthToCli::VaultNodeFetched {
                                trans_id,
                                result: NetResultCode::NetInternalError as i32,
                                node_buffer: Vec::new()
                            }
                        }
                    },
                    Err(err) => AuthToCli::VaultNodeFetched {
                        trans_id,
                        result: err as i32,
                        node_buffer: Vec::new()
                    },
                };
                self.send_message(reply).await
            }
            CliToAuth::VaultNodeSave { .. } => {
                todo!()
            }
            CliToAuth::VaultNodeDelete { .. } => {
                todo!()
            }
            CliToAuth::VaultNodeAdd { trans_id, parent_id, child_id, owner_id } => {
                let reply = match self.vault.ref_node(parent_id, child_id, owner_id, true).await {
                    Ok(()) => AuthToCli::VaultAddNodeReply {
                        trans_id,
                        result: NetResultCode::NetSuccess as i32
                    },
                    Err(err) => AuthToCli::VaultAddNodeReply {
                        trans_id,
                        result: err as i32
                    },
                };
                self.send_message(reply).await
            }
            CliToAuth::VaultNodeRemove { .. } => {
                todo!()
            }
            CliToAuth::VaultFetchNodeRefs { trans_id, node_id } => {
                let reply = match self.vault.fetch_refs(node_id, true).await {
                    Ok(refs) => AuthToCli::VaultNodeRefsFetched {
                        trans_id,
                        result: NetResultCode::NetSuccess as i32,
                        refs
                    },
                    Err(err) => AuthToCli::VaultNodeRefsFetched {
                        trans_id,
                        result: err as i32,
                        refs: Vec::new()
                    },
                };
                self.send_message(reply).await
            }
            CliToAuth::VaultInitAgeRequest { trans_id, age_instance_id, parent_age_instance_id,
                                             age_filename, age_instance_name, age_user_name,
                                             age_description, age_sequence, age_language } => {
                let reply = match find_age_instance(&age_instance_id, &parent_age_instance_id,
                                        &age_filename, &age_instance_name, &age_user_name,
                                        &age_description, age_sequence, age_language,
                                        &self.vault).await {
                    Ok((age_id, age_info)) => AuthToCli::VaultInitAgeReply {
                        trans_id,
                        result: NetResultCode::NetSuccess as i32,
                        age_vault_id: age_id,
                        age_info_vault_id: age_info,
                    },
                    Err(err) => AuthToCli::VaultInitAgeReply {
                        trans_id,
                        result: err as i32,
                        age_vault_id: 0,
                        age_info_vault_id: 0
                    },
                };
                self.send_message(reply).await
            }
            CliToAuth::VaultNodeFind { trans_id, node_buffer } => {
                let node_template = match VaultNode::from_blob(&node_buffer) {
                    Ok(node) => node,
                    Err(err) => {
                        warn!("Failed to parse blob: {err}");
                        return self.send_message(AuthToCli::VaultNodeFindReply {
                            trans_id,
                            result: NetResultCode::NetInternalError as i32,
                            node_ids: Vec::new(),
                        }).await;
                    }
                };
                let reply = match self.vault.find_nodes(node_template).await {
                    Ok(node_ids) => AuthToCli::VaultNodeFindReply {
                        trans_id,
                        result: NetResultCode::NetSuccess as i32,
                        node_ids,
                    },
                    Err(err) => AuthToCli::VaultNodeFindReply {
                        trans_id,
                        result: err as i32,
                        node_ids: Vec::new(),
                    }
                };
                self.send_message(reply).await
            }
            CliToAuth::VaultSetSeen { .. } => {
                todo!()
            }
            CliToAuth::VaultSendNode { .. } => {
                todo!()
            }
            CliToAuth::AgeRequest { .. } => {
                todo!()
            }
            CliToAuth::FileListRequest { trans_id, directory, ext } => {
                self.do_manifest(trans_id, &directory, &ext).await
            }
            CliToAuth::FileDownloadRequest { trans_id, filename } => {
                Box::pin(self.do_download(trans_id, &filename)).await
            }
            CliToAuth::PropagateBuffer { .. } => {
                warn!("Ignoring propagate buffer from {}", self.peer_addr().unwrap());
                true
            }
            CliToAuth::GetPublicAgeList { .. } => {
                todo!()
            }
            CliToAuth::SetAgePublic { .. } => {
                todo!()
            }
            CliToAuth::LogPythonTraceback { traceback } => {
                warn!("Python Traceback from {}:\n{}", self.peer_addr().unwrap(), traceback);
                true
            }
            CliToAuth::LogStackDump { stackdump } => {
                warn!("Stack Dump from {}:\n{}", self.peer_addr().unwrap(), stackdump);
                true
            }
            CliToAuth::ScoreCreate { .. } => {
                todo!()
            }
            CliToAuth::ScoreDelete { .. } => {
                todo!()
            }
            CliToAuth::ScoreGetScores { .. } => {
                todo!()
            }
            CliToAuth::ScoreAddPoints { .. } => {
                todo!()
            }
            CliToAuth::ScoreTransferPoints { .. } => {
                todo!()
            }
            CliToAuth::ScoreSetPoints { .. } => {
                todo!()
            }
            CliToAuth::ScoreGetRanks { .. } => {
                todo!()
            }
            CliToAuth::AccountExistsRequest { .. } => {
                todo!()
            }
            CliToAuth::ScoreGetHighScores { .. } => {
                todo!()
            }
            CliToAuth::FileDownloadChunkAck { .. }
                | CliToAuth::LogClientDebuggerConnect { .. } => true, // Ignored
        }
    }

    async fn send_message(&mut self, reply: AuthToCli) -> bool {
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

    async fn do_manifest(&mut self, trans_id: u32, dir_name: &str, ext: &str) -> bool {
        let reply = if let Some(manifest)
                            = fetch_list(dir_name, ext, &self.server_config.data_root)
        {
            debug!("Client {} requested list '{dir_name}\\*.{ext}'",
                   self.peer_addr().unwrap());

            AuthToCli::FileListReply {
                trans_id,
                result: NetResultCode::NetSuccess as i32,
                manifest
            }
        } else {
            warn!("Client {} requested invalid list '{dir_name}\\*.{ext}'",
                  self.peer_addr().unwrap());
            AuthToCli::FileListReply {
                trans_id,
                result: NetResultCode::NetFileNotFound as i32,
                manifest: Manifest::new()
            }
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
                return self.send_message(AuthToCli::download_error(trans_id,
                                            NetResultCode::NetInternalError)).await;
            };

            #[allow(clippy::large_stack_arrays)]
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
                            total_size,
                            offset,
                            file_data: Vec::from(&buffer[..count]),
                        };
                        if !self.send_message(reply).await {
                            return false;
                        }
                        // The total size was already validated above
                        #[allow(clippy::cast_possible_truncation)] {
                            offset += count as u32;
                        }
                    }
                    Err(err) => {
                        warn!("Could not read from {}: {}", download_path.display(), err);
                        return self.send_message(AuthToCli::download_error(trans_id,
                                                    NetResultCode::NetInternalError)).await;
                    }
                }
            }
        } else {
            warn!("Client {} requested invalid path '{}'", self.peer_addr().unwrap(),
                  filename);
            self.send_message(AuthToCli::download_error(trans_id,
                                NetResultCode::NetFileNotFound)).await
        }
    }

    async fn do_login_request(&mut self, trans_id: u32, client_challenge: u32,
                              account_name: &str, pass_hash: ShaDigest) -> bool
    {
        let account = match self.vault.get_account(account_name).await {
            Ok(Some(account)) => account,
            Ok(None) => {
                info!("{}: Account {} was not found", self.peer_addr().unwrap(),
                      account_name);

                // Don't leak to the client that the account doesn't exist...
                return self.send_message(AuthToCli::login_error(trans_id,
                                            NetResultCode::NetAuthenticationFailed)).await;
            }
            Err(err) => {
                return self.send_message(AuthToCli::login_error(trans_id, err)).await;
            }
        };

        // NOTE: Neither of these is good or secure, but they are what the
        // client expects.  To fix these, we'd have to break compatibility
        // with older clients.
        if use_email_auth(account_name) {
            // Use broken LE Sha0 hash mechanism
            let challenge_hash = match hash_password_challenge(
                        client_challenge, self.server_challenge,
                        account.pass_hash)
            {
                Ok(digest) => digest,
                Err(err) => {
                    warn!("Failed to generate challenge hash: {err}");
                    return self.send_message(AuthToCli::login_error(trans_id,
                                                NetResultCode::NetInternalError)).await;
                }
            };
            if challenge_hash != pass_hash {
                info!("{}: Login failure for account {}", self.peer_addr().unwrap(),
                      account_name);
                return self.send_message(AuthToCli::login_error(trans_id,
                                            NetResultCode::NetAuthenticationFailed)).await;
            }
        } else {
            // Directly compare the BE Sha1 hash
            // NOTE: The client sends its hash as Little Endian...
            if account.pass_hash != pass_hash.endian_swap() {
                info!("{}: Login failure for account {}", self.peer_addr().unwrap(),
                      account_name);
                return self.send_message(AuthToCli::login_error(trans_id,
                                            NetResultCode::NetAuthenticationFailed)).await;
            }
        }

        if account.is_banned() {
            info!("{}: Account {} is banned", self.peer_addr().unwrap(), account_name);
            return self.send_message(AuthToCli::login_error(trans_id,
                                        NetResultCode::NetAccountBanned)).await;
        }
        if self.server_config.restrict_logins && !account.can_login_restricted() {
            info!("{}: Account {} login is restricted", self.peer_addr().unwrap(),
                  account_name);
            return self.send_message(AuthToCli::login_error(trans_id,
                                        NetResultCode::NetLoginDenied)).await;
        }

        let ntd_key = match self.server_config.get_ntd_key() {
            Ok(key) => key,
            Err(err) => {
                warn!("Failed to get encryption key: {err}");
                return self.send_message(AuthToCli::login_error(trans_id,
                                            NetResultCode::NetInternalError)).await;
            }
        };

        info!("{}: Logged in as {} {}", self.peer_addr().unwrap(),
              account_name, account.account_id);
        self.account_id = Some(account.account_id);

        match self.fetch_account_players(trans_id, &account.account_id).await {
            Some(NetResultCode::NetSuccess) => (),
            Some(err) => {
                return self.send_message(AuthToCli::login_error(trans_id, err)).await;
            }
            None => return false,
        }

        // Send the final reply after all players are sent
        self.send_message(AuthToCli::AcctLoginReply {
            trans_id,
            result: NetResultCode::NetSuccess as i32,
            account_id: account.account_id,
            account_flags: account.account_flags,
            billing_type: account.billing_type,
            encryption_key: ntd_key,
        }).await
    }

    async fn fetch_account_players(&mut self, trans_id: u32, account_id: &Uuid)
        -> Option<NetResultCode>
    {
        let players = match self.vault.get_players(account_id).await {
            Ok(players) => players,
            Err(err) => return Some(err),
        };
        for player in players {
            let msg = AuthToCli::AcctPlayerInfo {
                trans_id,
                player_id: player.player_id,
                player_name: player.player_name,
                avatar_shape: player.avatar_shape,
                explorer: player.explorer,
            };
            if !self.send_message(msg).await {
                return None;
            }
        }
        Some(NetResultCode::NetSuccess)
    }

    async fn player_create(&mut self, trans_id: u32, player_name: &str,
                           avatar_shape: &str) -> bool
    {
        let Some(account_id) = self.account_id else {
            warn!("{} cannot create player: Not logged in", self.peer_addr().unwrap());
            return self.send_message(AuthToCli::player_create_error(trans_id,
                                        NetResultCode::NetAuthenticationFailed)).await;
        };

        // Disallow arbitrary choices of avatar shape...  Special models can
        // be set by admins when appropriate.
        if avatar_shape != "male" && avatar_shape != "female" {
            warn!("Client {} attempted to use avatar shape '{avatar_shape}'",
                  self.peer_addr().unwrap());
            return self.send_message(AuthToCli::player_create_error(trans_id,
                                        NetResultCode::NetInvalidParameter)).await;
        }

        let player_info = match self.vault.create_player(&account_id, player_name,
                                                         avatar_shape).await
        {
            Ok(player_info) => player_info,
            Err(result) => {
                return self.send_message(AuthToCli::player_create_error(trans_id, result)).await;
            }
        };

        if let Err(err) = create_player_nodes(&account_id, &player_info, &self.vault).await {
            return self.send_message(AuthToCli::player_create_error(trans_id, err)).await;
        }

        info!("{} created new player {} ({})", self.peer_addr().unwrap(),
              player_info.player_name, player_info.player_id);

        self.send_message(AuthToCli::PlayerCreateReply {
            trans_id,
            result: NetResultCode::NetSuccess as i32,
            player_id: player_info.player_id,
            explorer: player_info.explorer,
            player_name: player_info.player_name,
            avatar_shape: player_info.avatar_shape,
        }).await
    }

    async fn do_set_player(&mut self, trans_id: u32, player_id: u32) -> bool {
        let Some(account_id) = self.account_id else {
            warn!("{} cannot set player: Not logged in", self.peer_addr().unwrap());
            return self.send_message(AuthToCli::AcctSetPlayerReply {
                trans_id,
                result: NetResultCode::NetAuthenticationFailed as i32
            }).await;
        };

        let player_node = match self.vault.fetch_node(player_id).await
                                    .map(|node| node.as_player_node())
        {
            Ok(Some(node)) => node,
            Ok(None) => {
                warn!("{} requested invalid Player ID {}", self.peer_addr().unwrap(),
                      player_id);
                return self.send_message(AuthToCli::AcctSetPlayerReply {
                    trans_id,
                    result: NetResultCode::NetPlayerNotFound as i32
                }).await;
            }
            Err(err) => {
                warn!("{}: Failed to fetch Player ID {}", self.peer_addr().unwrap(),
                      player_id);
                return self.send_message(AuthToCli::AcctSetPlayerReply {
                    trans_id,
                    result: err as i32
                }).await;
            }
        };

        if player_node.account_id() != &account_id {
            warn!("{} requested Player {}, which belongs to a different account {}",
                  self.peer_addr().unwrap(), player_id, player_node.account_id());
            return self.send_message(AuthToCli::AcctSetPlayerReply {
                trans_id,
                result: NetResultCode::NetPlayerNotFound as i32
            }).await;
        }

        let player_info = match self.vault.get_player_info_node(player_id).await {
            Ok(node) => node.as_player_info_node().unwrap(),
            Err(err) => {
                warn!("Failed to get Player Info node for Player {player_id}");
                return self.send_message(AuthToCli::AcctSetPlayerReply {
                    trans_id,
                    result: err as i32
                }).await;
            }
        };

        if player_info.online() != 0 {
            warn!("{} requested already-online player {}", self.peer_addr().unwrap(),
                  player_id);
            return self.send_message(AuthToCli::AcctSetPlayerReply {
                trans_id,
                result: NetResultCode::NetLoggedInElsewhere as i32
            }).await;
        }

        let update = VaultPlayerInfoNode::new_update(player_info.node_id(), 1,
                        "Lobby", &Uuid::nil());
        if let Err(err) = self.vault.update_node(update).await {
            warn!("Failed to set player {player_id} online");
            return self.send_message(AuthToCli::AcctSetPlayerReply {
                trans_id,
                result: err as i32
            }).await;
        }

        info!("{} signed in as {} ({})", self.peer_addr().unwrap(),
              player_node.player_name_ci(), player_id);
        self.player_id = Some(player_id);

        self.send_message(AuthToCli::AcctSetPlayerReply {
            trans_id,
            result: NetResultCode::NetSuccess as i32,
        }).await
    }

    async fn set_player_offline(&mut self, player_id: u32) {
        let player_info = match self.vault.get_player_info_node(player_id).await {
            Ok(node) => node.as_player_info_node().unwrap(),
            Err(err) => {
                warn!("Failed to get Player Info node for Player {player_id}: {err:?}");
                return;
            }
        };

        let update = VaultPlayerInfoNode::new_update(player_info.node_id(), 0, "", &Uuid::nil());
        if let Err(err) = self.vault.update_node(update).await {
            warn!("Failed to set player {player_id} offline: {err:?}");
            return;
        }

        info!("Player {} ({player_id}) is now offline", player_info.player_name_ci());
    }

    async fn handle_disconnect(&mut self) {
        if let Some(player_id) = self.player_id {
            self.set_player_offline(player_id).await;
        }
    }
}
