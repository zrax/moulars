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
use log::{error, warn, debug};
use rand::Rng;
use tokio::io::{AsyncReadExt, BufReader};
use tokio::sync::mpsc;
use tokio::net::TcpStream;
use uuid::Uuid;

use crate::general_error;
use crate::config::ServerConfig;
use crate::crypt::CryptStream;
use crate::netcli::NetResultCode;
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
            Ok(CliToAuth::ClientRegisterRequest { build_id }) => {
                if build_id != 0 && build_id != server_config.build_id {
                    warn!("Client {} has an unexpected build ID {}",
                          stream.get_ref().peer_addr().unwrap(), build_id);
                    // The client isn't listening for anything other than a
                    // ClientRegisterReply, which doesn't have a result field,
                    // so we can't notify them that their build is invalid...
                    return;
                }
                let server_challenge = rand::thread_rng().gen::<u32>();
                let reply = AuthToCli::ClientRegisterReply { server_challenge };
                if !send_message(stream.get_mut(), reply).await {
                    return;
                }
            }
            Ok(CliToAuth::ClientSetCCRLevel { .. }) => {
                warn!("Ignoring CCR level set request from {}",
                      stream.get_ref().peer_addr().unwrap());
            }
            Ok(CliToAuth::AcctLoginRequest { .. }) => {
                todo!()
            }
            Ok(CliToAuth::AcctSetPlayerRequest { .. }) => {
                todo!()
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
            Ok(CliToAuth::FileListRequest { .. }) => {
                todo!()
            }
            Ok(CliToAuth::FileDownloadRequest { .. }) => {
                todo!()
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
