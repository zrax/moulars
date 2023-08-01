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

use std::io::{Write, Result};

use byteorder::{LittleEndian, WriteBytesExt};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use tokio::io::{AsyncReadExt, BufReader};
use uuid::Uuid;

use crate::general_error;
use crate::hashes::ShaDigest;
use crate::net_crypt::CryptTcpStream;
use crate::netcli::NetResultCode;
use crate::plasma::{StreamWrite, net_io};
use crate::vault::NodeRef;
use super::age_info::NetAgeInfo;
use super::manifest::Manifest;

pub enum CliToAuth {
    PingRequest {
        ping_time: u32,
        trans_id: u32,
        payload: Vec<u8>,
    },
    ClientRegisterRequest {
        build_id: u32,
    },
    ClientSetCCRLevel {
        ccr_level: u32,
    },
    AcctLoginRequest {
        trans_id: u32,
        client_challenge: u32,
        account_name: String,
        pass_hash: ShaDigest,
        auth_token: String,
        os: String,
    },
    AcctSetPlayerRequest {
        trans_id: u32,
        player_id: u32,
    },
    AcctCreateRequest {
        trans_id: u32,
        account_name: String,
        auth_hash: ShaDigest,
        account_flags: u32,
        billing_type: u32,
    },
    AcctChangePasswordRequest {
        trans_id: u32,
        account_name: String,
        auth_hash: ShaDigest,
    },
    AcctSetRolesRequest {
        trans_id: u32,
        account_name: String,
        account_flags: u32,
    },
    AcctSetBillingTypeRequest {
        trans_id: u32,
        account_name: String,
        billing_type: u32,
    },
    AcctActivateRequest {
        trans_id: u32,
        activation_key: Uuid,
    },
    AcctCreateFromKeyRequest {
        trans_id: u32,
        account_name: String,
        auth_hash: ShaDigest,
        key: Uuid,
        billing_type: u32,
    },
    PlayerDeleteRequest {
        trans_id: u32,
        player_id: u32,
    },
    PlayerCreateRequest {
        trans_id: u32,
        player_name: String,
        avatar_shape: String,
        friend_invite: String,
    },
    UpgradeVisitorRequest {
        trans_id: u32,
        player_id: u32,
    },
    SetPlayerBanStatusRequest {
        trans_id: u32,
        player_id: u32,
        banned: u32,
    },
    KickPlayer {
        player_id: u32,
    },
    ChangePlayerNameRequest {
        trans_id: u32,
        player_id: u32,
        new_name: String,
    },
    SendFriendInviteRequest {
        trans_id: u32,
        invite_id: Uuid,
        email_address: String,
        to_player: String,
    },
    VaultNodeCreate {
        trans_id: u32,
        node_buffer: Vec<u8>,
    },
    VaultNodeFetch {
        trans_id: u32,
        node_id: u32,
    },
    VaultNodeSave {
        trans_id: u32,
        node_id: u32,
        revision: Uuid,
        node_buffer: Vec<u8>,
    },
    VaultNodeDelete {
        node_id: u32,
    },
    VaultNodeAdd {
        trans_id: u32,
        parent_id: u32,
        child_id: u32,
        owner_id: u32,
    },
    VaultNodeRemove {
        trans_id: u32,
        parent_id: u32,
        child_id: u32,
    },
    VaultFetchNodeRefs {
        trans_id: u32,
        node_id: u32,
    },
    VaultInitAgeRequest {
        trans_id: u32,
        age_instance_id: Uuid,
        parent_age_instance_id: Uuid,
        age_filename: String,
        age_instance_name: String,
        age_user_name: String,
        age_description: String,
        age_sequence: u32,
        age_language: u32,
    },
    VaultNodeFind {
        trans_id: u32,
        node_buffer: Vec<u8>,
    },
    VaultSetSeen {
        parent_id: u32,
        child_id: u32,
        seen: u8,
    },
    VaultSendNode {
        src_node_id: u32,
        dest_player_id: u32,
    },
    AgeRequest {
        trans_id: u32,
        age_name: String,
        age_instance_id: Uuid,
    },
    FileListRequest {
        trans_id: u32,
        directory: String,
        ext: String,
    },
    FileDownloadRequest {
        trans_id: u32,
        filename: String,
    },
    FileDownloadChunkAck {
        trans_id: u32,
    },
    PropagateBuffer {
        type_id: u32,
        buffer: Vec<u8>,
    },
    GetPublicAgeList {
        trans_id: u32,
        age_filename: String,
    },
    SetAgePublic {
        age_info_id: u32,
        public: u8,
    },
    LogPythonTraceback {
        traceback: String,
    },
    LogStackDump {
        stackdump: String,
    },
    LogClientDebuggerConnect {
        dummy: u32,
    },
    ScoreCreate {
        trans_id: u32,
        owner_id: u32,
        game_name: String,
        game_type: u32,
        value: u32,
    },
    ScoreDelete {
        trans_id: u32,
        score_id: u32,
    },
    ScoreGetScores {
        trans_id: u32,
        owner_id: u32,
        game_name: String,
    },
    ScoreAddPoints {
        trans_id: u32,
        score_id: u32,
        points: u32,
    },
    ScoreTransferPoints {
        trans_id: u32,
        src_score_id: u32,
        dest_score_id: u32,
        points: u32,
    },
    ScoreSetPoints {
        trans_id: u32,
        score_id: u32,
        points: u32,
    },
    ScoreGetRanks {
        trans_id: u32,
        owner_id: u32,
        score_group: u32,
        parent_folder_id: u32,
        game_name: String,
        time_period: u32,
        num_results: u32,
        page_number: u32,
        sort_desc: u32,
    },
    AccountExistsRequest {
        trans_id: u32,
        account_name: String,
    },
    ScoreGetHighScores {
        trans_id: u32,
        age_id: u32,
        max_scores: u32,
        game_name: String,
    },
}

pub enum AuthToCli {
    PingReply {
        ping_time: u32,
        trans_id: u32,
        payload: Vec<u8>,
    },
    ServerAddr {
        // Limits us to IPv4, unfortunately :(
        server_addr: u32,
        token: Uuid,
    },
    NotifyNewBuild {
        dummy: u32,
    },
    ClientRegisterReply {
        server_challenge: u32,
    },
    AcctLoginReply {
        trans_id: u32,
        result: i32,
        account_id: Uuid,
        account_flags: u32,
        billing_type: u32,
        encryption_key: [u32; 4],
    },
    AcctPlayerInfo {
        trans_id: u32,
        player_id: u32,
        player_name: String,
        avatar_shape: String,
        explorer: u32,
    },
    AcctSetPlayerReply {
        trans_id: u32,
        result: i32,
    },
    AcctCreateReply {
        trans_id: u32,
        result: i32,
        account_id: Uuid,
    },
    AcctChangePasswordReply {
        trans_id: u32,
        result: i32,
    },
    AcctSetRolesReply {
        trans_id: u32,
        result: i32,
    },
    AcctSetBillingTypeReply {
        trans_id: u32,
        result: i32,
    },
    AcctActivateReply {
        trans_id: u32,
        result: i32,
    },
    AcctCreateFromKeyReply {
        trans_id: u32,
        result: i32,
        account_id: Uuid,
        activation_key: Uuid,
    },
    PlayerCreateReply {
        trans_id: u32,
        result: i32,
        player_id: u32,
        explorer: u32,
        player_name: String,
        avatar_shape: String,
    },
    PlayerDeleteReply {
        trans_id: u32,
        result: i32,
    },
    UpgradeVisitorReply {
        trans_id: u32,
        result: i32,
    },
    SetPlayerBanStatusReply {
        trans_id: u32,
        result: i32,
    },
    ChangePlayerNameReply {
        trans_id: u32,
        result: i32,
    },
    SendFriendInviteReply {
        trans_id: u32,
        result: i32,
    },
    VaultNodeCreated {
        trans_id: u32,
        result: i32,
        node_id: u32,
    },
    VaultNodeFetched {
        trans_id: u32,
        result: i32,
        node_buffer: Vec<u8>,
    },
    VaultNodeChanged {
        node_id: u32,
        revision_id: Uuid,
    },
    VaultNodeDeleted {
        node_id: u32,
    },
    VaultNodeAdded {
        parent_id: u32,
        child_id: u32,
        owner_id: u32,
    },
    VaultNodeRemoved {
        parent_id: u32,
        child_id: u32,
    },
    VaultNodeRefsFetched {
        trans_id: u32,
        result: i32,
        refs: Vec<NodeRef>,
    },
    VaultInitAgeReply {
        trans_id: u32,
        result: i32,
        age_vault_id: u32,
        age_info_vault_id: u32,
    },
    VaultNodeFindReply {
        trans_id: u32,
        result: i32,
        node_ids: Vec<u32>,
    },
    VaultSaveNodeReply {
        trans_id: u32,
        result: i32,
    },
    VaultAddNodeReply {
        trans_id: u32,
        result: i32,
    },
    VaultRemoveNodeReply {
        trans_id: u32,
        result: i32,
    },
    AgeReply {
        trans_id: u32,
        result: i32,
        age_mcp_id: u32,
        age_instance_id: Uuid,
        age_vault_id: u32,
        // Limits us to IPv4, unfortunately :(
        game_server_node: u32,
    },
    FileListReply {
        trans_id: u32,
        result: i32,
        manifest: Manifest,
    },
    FileDownloadChunk {
        trans_id: u32,
        result: i32,
        total_size: u32,
        offset: u32,
        file_data: Vec<u8>,
    },
    PropagateBuffer {
        type_id: u32,
        buffer: Vec<u8>,
    },
    KickedOff {
        reason: i32,
    },
    PublicAgeList {
        trans_id: u32,
        result: i32,
        ages: Vec<NetAgeInfo>,
    },
    ScoreCreateReply {
        trans_id: u32,
        result: i32,
        score_id: u32,
        created_time: u32,
    },
    ScoreDeleteReply {
        trans_id: u32,
        result: i32,
    },
    ScoreGetScoresReply {
        trans_id: u32,
        result: i32,
        score_count: u32,
        score_buffer: Vec<u8>,
    },
    ScoreAddPointsReply {
        trans_id: u32,
        result: i32,
    },
    ScoreTransferPointsReply {
        trans_id: u32,
        result: i32,
    },
    ScoreSetPointsReply {
        trans_id: u32,
        result: i32,
    },
    ScoreGetRanksReply {
        trans_id: u32,
        result: i32,
        rank_count: u32,
        rank_buffer: Vec<u8>,
    },
    AccountExistsReply {
        trans_id: u32,
        result: i32,
        exists: u8,
    },
    ScoreGetHighScoresReply {
        trans_id: u32,
        result: i32,
        score_count: u32,
        score_buffer: Vec<u8>,
    },
    ServerCaps {
        caps_buffer: Vec<u8>,
    },
}

#[repr(u16)]
#[derive(FromPrimitive)]
enum ClientMsgId {
    PingRequest = 0,
    ClientRegisterRequest,
    ClientSetCCRLevel,
    AcctLoginRequest,
    AcctSetEulaVersion,
    AcctSetDataRequest,
    AcctSetPlayerRequest,
    AcctCreateRequest,
    AcctChangePasswordRequest,
    AcctSetRolesRequest,
    AcctSetBillingTypeRequest,
    AcctActivateRequest,
    AcctCreateFromKeyRequest,
    PlayerDeleteRequest,
    PlayerUndeleteRequest,
    PlayerSelectRequest,
    PlayerRenameRequest,
    PlayerCreateRequest,
    PlayerSetStatus,
    PlayerChat,
    UpgradeVisitorRequest,
    SetPlayerBanStatusRequest,
    KickPlayer,
    ChangePlayerNameRequest,
    SendFriendInviteRequest,
    VaultNodeCreate,
    VaultNodeFetch,
    VaultNodeSave,
    VaultNodeDelete,
    VaultNodeAdd,
    VaultNodeRemove,
    VaultFetchNodeRefs,
    VaultInitAgeRequest,
    VaultNodeFind,
    VaultSetSeen,
    VaultSendNode,
    AgeRequest,
    FileListRequest,
    FileDownloadRequest,
    FileDownloadChunkAck,
    PropagateBuffer,
    GetPublicAgeList,
    SetAgePublic,
    LogPythonTraceback,
    LogStackDump,
    LogClientDebuggerConnect,
    ScoreCreate,
    ScoreDelete,
    ScoreGetScores,
    ScoreAddPoints,
    ScoreTransferPoints,
    ScoreSetPoints,
    ScoreGetRanks,
    AccountExistsRequest,

    // DirtSand extended messages
    AgeRequestEx = 0x1000,
    ScoreGetHighScores,
}

#[repr(u16)]
enum ServerMsgId {
    PingReply = 0,
    ServerAddr,
    NotifyNewBuild,
    ClientRegisterReply,
    AcctLoginReply,
    #[allow(unused)] AcctData,
    AcctPlayerInfo,
    AcctSetPlayerReply,
    AcctCreateReply,
    AcctChangePasswordReply,
    AcctSetRolesReply,
    AcctSetBillingTypeReply,
    AcctActivateReply,
    AcctCreateFromKeyReply,
    #[allow(unused)] PlayerList,
    #[allow(unused)] PlayerChat,
    PlayerCreateReply,
    PlayerDeleteReply,
    UpgradeVisitorReply,
    SetPlayerBanStatusReply,
    ChangePlayerNameReply,
    SendFriendInviteReply,
    #[allow(unused)] FriendNotify,
    VaultNodeCreated,
    VaultNodeFetched,
    VaultNodeChanged,
    VaultNodeDeleted,
    VaultNodeAdded,
    VaultNodeRemoved,
    VaultNodeRefsFetched,
    VaultInitAgeReply,
    VaultNodeFindReply,
    VaultSaveNodeReply,
    VaultAddNodeReply,
    VaultRemoveNodeReply,
    AgeReply,
    FileListReply,
    FileDownloadChunk,
    PropagateBuffer,
    KickedOff,
    PublicAgeList,
    ScoreCreateReply,
    ScoreDeleteReply,
    ScoreGetScoresReply,
    ScoreAddPointsReply,
    ScoreTransferPointsReply,
    ScoreSetPointsReply,
    ScoreGetRanksReply,
    AccountExistsReply,

    // DirtSand extended messages
    #[allow(unused)] AgeReplyEx = 0x1000,
    ScoreGetHighScoresReply,
    ServerCaps,
}

const MAX_NODE_BUFFER_SIZE: u32 = 1024 * 1024;
const MAX_PING_PAYLOAD: u32 = 64 * 1024;
const MAX_PROPAGATE_BUFFER_SIZE: u32 = 1024 * 1024;

impl CliToAuth {
    pub async fn read(stream: &mut BufReader<CryptTcpStream>) -> Result<Self> {
        let msg_id = stream.read_u16_le().await?;
        match ClientMsgId::from_u16(msg_id) {
            Some(ClientMsgId::PingRequest) => {
                let ping_time = stream.read_u32_le().await?;
                let trans_id = stream.read_u32_le().await?;
                let payload = net_io::read_sized_buffer(stream, MAX_PING_PAYLOAD).await?;
                Ok(CliToAuth::PingRequest { trans_id, ping_time, payload })
            }
            Some(ClientMsgId::ClientRegisterRequest) => {
                let build_id = stream.read_u32_le().await?;
                Ok(CliToAuth::ClientRegisterRequest { build_id })
            }
            Some(ClientMsgId::ClientSetCCRLevel) => {
                let ccr_level = stream.read_u32_le().await?;
                Ok(CliToAuth::ClientSetCCRLevel { ccr_level })
            }
            Some(ClientMsgId::AcctLoginRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let client_challenge = stream.read_u32_le().await?;
                let account_name = net_io::read_utf16_str(stream).await?;
                let pass_hash = ShaDigest::read(stream).await?;
                let auth_token = net_io::read_utf16_str(stream).await?;
                let os = net_io::read_utf16_str(stream).await?;
                Ok(CliToAuth::AcctLoginRequest {
                    trans_id, client_challenge, account_name, pass_hash,
                    auth_token, os
                })
            }
            Some(ClientMsgId::AcctSetEulaVersion) => {
                // This message is never defined in the client
                Err(general_error!("Unsupported message AcctSetEulaVersion"))
            }
            Some(ClientMsgId::AcctSetDataRequest) => {
                // This message is never defined in the client
                Err(general_error!("Unsupported message AcctSetDataRequest"))
            }
            Some(ClientMsgId::AcctSetPlayerRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let player_id = stream.read_u32_le().await?;
                Ok(CliToAuth::AcctSetPlayerRequest { trans_id, player_id })
            }
            Some(ClientMsgId::AcctCreateRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let account_name = net_io::read_utf16_str(stream).await?;
                let auth_hash = ShaDigest::read(stream).await?;
                let account_flags = stream.read_u32_le().await?;
                let billing_type = stream.read_u32_le().await?;
                Ok(CliToAuth::AcctCreateRequest {
                    trans_id, account_name, auth_hash, account_flags,
                    billing_type
                })
            }
            Some(ClientMsgId::AcctChangePasswordRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let account_name = net_io::read_utf16_str(stream).await?;
                let auth_hash = ShaDigest::read(stream).await?;
                Ok(CliToAuth::AcctChangePasswordRequest {
                    trans_id, account_name, auth_hash
                })
            }
            Some(ClientMsgId::AcctSetRolesRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let account_name = net_io::read_utf16_str(stream).await?;
                let account_flags = stream.read_u32_le().await?;
                Ok(CliToAuth::AcctSetRolesRequest {
                    trans_id, account_name, account_flags
                })
            }
            Some(ClientMsgId::AcctSetBillingTypeRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let account_name = net_io::read_utf16_str(stream).await?;
                let billing_type = stream.read_u32_le().await?;
                Ok(CliToAuth::AcctSetBillingTypeRequest {
                    trans_id, account_name, billing_type
                })
            }
            Some(ClientMsgId::AcctActivateRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let activation_key = net_io::read_uuid(stream).await?;
                Ok(CliToAuth::AcctActivateRequest { trans_id, activation_key })
            }
            Some(ClientMsgId::AcctCreateFromKeyRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let account_name = net_io::read_utf16_str(stream).await?;
                let auth_hash = ShaDigest::read(stream).await?;
                let key = net_io::read_uuid(stream).await?;
                let billing_type = stream.read_u32_le().await?;
                Ok(CliToAuth::AcctCreateFromKeyRequest {
                    trans_id, account_name, auth_hash, key, billing_type
                })
            }
            Some(ClientMsgId::PlayerDeleteRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let player_id = stream.read_u32_le().await?;
                Ok(CliToAuth::PlayerDeleteRequest { trans_id, player_id })
            }
            Some(ClientMsgId::PlayerUndeleteRequest) => {
                // This message is never defined in the client
                Err(general_error!("Unsupported message PlayerUndeleteRequest"))
            }
            Some(ClientMsgId::PlayerSelectRequest) => {
                // This message is never defined in the client
                Err(general_error!("Unsupported message PlayerSelectRequest"))
            }
            Some(ClientMsgId::PlayerRenameRequest) => {
                // This message is never defined in the client
                Err(general_error!("Unsupported message PlayerRenameRequest"))
            }
            Some(ClientMsgId::PlayerCreateRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let player_name = net_io::read_utf16_str(stream).await?;
                let avatar_shape = net_io::read_utf16_str(stream).await?;
                let friend_invite = net_io::read_utf16_str(stream).await?;
                Ok(CliToAuth::PlayerCreateRequest {
                    trans_id, player_name, avatar_shape, friend_invite
                })
            }
            Some(ClientMsgId::PlayerSetStatus) => {
                // This message is never defined in the client
                Err(general_error!("Unsupported message PlayerSetStatus"))
            }
            Some(ClientMsgId::PlayerChat) => {
                // This message is never defined in the client
                Err(general_error!("Unsupported message PlayerChat"))
            }
            Some(ClientMsgId::UpgradeVisitorRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let player_id = stream.read_u32_le().await?;
                Ok(CliToAuth::UpgradeVisitorRequest { trans_id, player_id })
            }
            Some(ClientMsgId::SetPlayerBanStatusRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let player_id = stream.read_u32_le().await?;
                let banned = stream.read_u32_le().await?;
                Ok(CliToAuth::SetPlayerBanStatusRequest {
                    trans_id, player_id, banned
                })
            }
            Some(ClientMsgId::KickPlayer) => {
                let player_id = stream.read_u32_le().await?;
                Ok(CliToAuth::KickPlayer { player_id })
            }
            Some(ClientMsgId::ChangePlayerNameRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let player_id = stream.read_u32_le().await?;
                let new_name = net_io::read_utf16_str(stream).await?;
                Ok(CliToAuth::ChangePlayerNameRequest {
                    trans_id, player_id, new_name
                })
            }
            Some(ClientMsgId::SendFriendInviteRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let invite_id = net_io::read_uuid(stream).await?;
                let email_address = net_io::read_utf16_str(stream).await?;
                let to_player = net_io::read_utf16_str(stream).await?;
                Ok(CliToAuth::SendFriendInviteRequest {
                    trans_id, invite_id, email_address, to_player
                })
            }
            Some(ClientMsgId::VaultNodeCreate) => {
                let trans_id = stream.read_u32_le().await?;
                let node_buffer = net_io::read_sized_buffer(stream, MAX_NODE_BUFFER_SIZE).await?;
                Ok(CliToAuth::VaultNodeCreate { trans_id, node_buffer })
            }
            Some(ClientMsgId::VaultNodeFetch) => {
                let trans_id = stream.read_u32_le().await?;
                let node_id = stream.read_u32_le().await?;
                Ok(CliToAuth::VaultNodeFetch { trans_id, node_id })
            }
            Some(ClientMsgId::VaultNodeSave) => {
                let trans_id = stream.read_u32_le().await?;
                let node_id = stream.read_u32_le().await?;
                let revision = net_io::read_uuid(stream).await?;
                let node_buffer = net_io::read_sized_buffer(stream, MAX_NODE_BUFFER_SIZE).await?;
                Ok(CliToAuth::VaultNodeSave {
                    trans_id, node_id, revision, node_buffer
                })
            }
            Some(ClientMsgId::VaultNodeDelete) => {
                let node_id = stream.read_u32_le().await?;
                Ok(CliToAuth::VaultNodeDelete { node_id })
            }
            Some(ClientMsgId::VaultNodeAdd) => {
                let trans_id = stream.read_u32_le().await?;
                let parent_id = stream.read_u32_le().await?;
                let child_id = stream.read_u32_le().await?;
                let owner_id = stream.read_u32_le().await?;
                Ok(CliToAuth::VaultNodeAdd {
                    trans_id, parent_id, child_id, owner_id
                })
            }
            Some(ClientMsgId::VaultNodeRemove) => {
                let trans_id = stream.read_u32_le().await?;
                let parent_id = stream.read_u32_le().await?;
                let child_id = stream.read_u32_le().await?;
                Ok(CliToAuth::VaultNodeRemove { trans_id, parent_id, child_id })
            }
            Some(ClientMsgId::VaultFetchNodeRefs) => {
                let trans_id = stream.read_u32_le().await?;
                let node_id = stream.read_u32_le().await?;
                Ok(CliToAuth::VaultFetchNodeRefs { trans_id, node_id })
            }
            Some(ClientMsgId::VaultInitAgeRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let age_instance_id = net_io::read_uuid(stream).await?;
                let parent_age_instance_id = net_io::read_uuid(stream).await?;
                let age_filename = net_io::read_utf16_str(stream).await?;
                let age_instance_name = net_io::read_utf16_str(stream).await?;
                let age_user_name = net_io::read_utf16_str(stream).await?;
                let age_description = net_io::read_utf16_str(stream).await?;
                let age_sequence = stream.read_u32_le().await?;
                let age_language = stream.read_u32_le().await?;
                Ok(CliToAuth::VaultInitAgeRequest {
                    trans_id, age_instance_id, parent_age_instance_id,
                    age_filename, age_instance_name, age_user_name,
                    age_description, age_sequence, age_language
                })
            }
            Some(ClientMsgId::VaultNodeFind) => {
                let trans_id = stream.read_u32_le().await?;
                let node_buffer = net_io::read_sized_buffer(stream, MAX_NODE_BUFFER_SIZE).await?;
                Ok(CliToAuth::VaultNodeFind { trans_id, node_buffer })
            }
            Some(ClientMsgId::VaultSetSeen) => {
                let parent_id = stream.read_u32_le().await?;
                let child_id = stream.read_u32_le().await?;
                let seen = stream.read_u8().await?;
                Ok(CliToAuth::VaultSetSeen { parent_id, child_id, seen })
            }
            Some(ClientMsgId::VaultSendNode) => {
                let src_node_id = stream.read_u32_le().await?;
                let dest_player_id = stream.read_u32_le().await?;
                Ok(CliToAuth::VaultSendNode { src_node_id, dest_player_id })
            }
            Some(ClientMsgId::AgeRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let age_name = net_io::read_utf16_str(stream).await?;
                let age_instance_id = net_io::read_uuid(stream).await?;
                Ok(CliToAuth::AgeRequest { trans_id, age_name, age_instance_id })
            }
            Some(ClientMsgId::FileListRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let directory = net_io::read_utf16_str(stream).await?;
                let ext = net_io::read_utf16_str(stream).await?;
                Ok(CliToAuth::FileListRequest { trans_id, directory, ext })
            }
            Some(ClientMsgId::FileDownloadRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let filename = net_io::read_utf16_str(stream).await?;
                Ok(CliToAuth::FileDownloadRequest { trans_id, filename })
            }
            Some(ClientMsgId::FileDownloadChunkAck) => {
                let trans_id = stream.read_u32_le().await?;
                Ok(CliToAuth::FileDownloadChunkAck { trans_id })
            }
            Some(ClientMsgId::PropagateBuffer) => {
                let type_id = stream.read_u32_le().await?;
                let buffer = net_io::read_sized_buffer(stream, MAX_PROPAGATE_BUFFER_SIZE).await?;
                Ok(CliToAuth::PropagateBuffer { type_id, buffer })
            }
            Some(ClientMsgId::GetPublicAgeList) => {
                let trans_id = stream.read_u32_le().await?;
                let age_filename = net_io::read_utf16_str(stream).await?;
                Ok(CliToAuth::GetPublicAgeList { trans_id, age_filename })
            }
            Some(ClientMsgId::SetAgePublic) => {
                let age_info_id = stream.read_u32_le().await?;
                let public = stream.read_u8().await?;
                Ok(CliToAuth::SetAgePublic { age_info_id, public })
            }
            Some(ClientMsgId::LogPythonTraceback) => {
                let traceback = net_io::read_utf16_str(stream).await?;
                Ok(CliToAuth::LogPythonTraceback { traceback })
            }
            Some(ClientMsgId::LogStackDump) => {
                let stackdump = net_io::read_utf16_str(stream).await?;
                Ok(CliToAuth::LogStackDump { stackdump })
            }
            Some(ClientMsgId::LogClientDebuggerConnect) => {
                let dummy = stream.read_u32_le().await?;
                Ok(CliToAuth::LogClientDebuggerConnect { dummy })
            }
            Some(ClientMsgId::ScoreCreate) => {
                let trans_id = stream.read_u32_le().await?;
                let owner_id = stream.read_u32_le().await?;
                let game_name = net_io::read_utf16_str(stream).await?;
                let game_type = stream.read_u32_le().await?;
                let value = stream.read_u32_le().await?;
                Ok(CliToAuth::ScoreCreate {
                    trans_id, owner_id, game_name, game_type, value
                })
            }
            Some(ClientMsgId::ScoreDelete) => {
                let trans_id = stream.read_u32_le().await?;
                let score_id = stream.read_u32_le().await?;
                Ok(CliToAuth::ScoreDelete { trans_id, score_id })
            }
            Some(ClientMsgId::ScoreGetScores) => {
                let trans_id = stream.read_u32_le().await?;
                let owner_id = stream.read_u32_le().await?;
                let game_name = net_io::read_utf16_str(stream).await?;
                Ok(CliToAuth::ScoreGetScores { trans_id, owner_id, game_name })
            }
            Some(ClientMsgId::ScoreAddPoints) => {
                let trans_id = stream.read_u32_le().await?;
                let score_id = stream.read_u32_le().await?;
                let points = stream.read_u32_le().await?;
                Ok(CliToAuth::ScoreAddPoints { trans_id, score_id, points })
            }
            Some(ClientMsgId::ScoreTransferPoints) => {
                let trans_id = stream.read_u32_le().await?;
                let src_score_id = stream.read_u32_le().await?;
                let dest_score_id = stream.read_u32_le().await?;
                let points = stream.read_u32_le().await?;
                Ok(CliToAuth::ScoreTransferPoints {
                    trans_id, src_score_id, dest_score_id, points
                })
            }
            Some(ClientMsgId::ScoreSetPoints) => {
                let trans_id = stream.read_u32_le().await?;
                let score_id = stream.read_u32_le().await?;
                let points = stream.read_u32_le().await?;
                Ok(CliToAuth::ScoreSetPoints { trans_id, score_id, points })
            }
            Some(ClientMsgId::ScoreGetRanks) => {
                let trans_id = stream.read_u32_le().await?;
                let owner_id = stream.read_u32_le().await?;
                let score_group = stream.read_u32_le().await?;
                let parent_folder_id = stream.read_u32_le().await?;
                let game_name = net_io::read_utf16_str(stream).await?;
                let time_period = stream.read_u32_le().await?;
                let num_results = stream.read_u32_le().await?;
                let page_number = stream.read_u32_le().await?;
                let sort_desc = stream.read_u32_le().await?;
                Ok(CliToAuth::ScoreGetRanks {
                    trans_id, owner_id, score_group, parent_folder_id,
                    game_name, time_period, num_results, page_number,
                    sort_desc
                })
            }
            Some(ClientMsgId::AccountExistsRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let account_name = net_io::read_utf16_str(stream).await?;
                Ok(CliToAuth::AccountExistsRequest { trans_id, account_name })
            }
            Some(ClientMsgId::AgeRequestEx) => {
                // This message is never defined in the client
                Err(general_error!("Unsupported message AgeRequestEx"))
            }
            Some(ClientMsgId::ScoreGetHighScores) => {
                let trans_id = stream.read_u32_le().await?;
                let age_id = stream.read_u32_le().await?;
                let max_scores = stream.read_u32_le().await?;
                let game_name = net_io::read_utf16_str(stream).await?;
                Ok(CliToAuth::ScoreGetHighScores {
                    trans_id, age_id, max_scores, game_name
                })
            }
            None => Err(general_error!("Bad message ID {}", msg_id))
        }
    }
}

impl AuthToCli {
    pub fn download_error(trans_id: u32, result: NetResultCode) -> Self {
        Self::FileDownloadChunk {
            trans_id,
            result: result as i32,
            total_size: 0,
            offset: 0,
            file_data: Vec::new(),
        }
    }

    pub fn login_error(trans_id: u32, result: NetResultCode) -> Self {
        Self::AcctLoginReply {
            trans_id,
            result: result as i32,
            account_id: Uuid::nil(),
            account_flags: 0,
            billing_type: 0,
            encryption_key: [0; 4]
        }
    }
}

impl StreamWrite for AuthToCli {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        match self {
            AuthToCli::PingReply { trans_id, ping_time, payload } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::PingReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_u32::<LittleEndian>(*ping_time)?;
                net_io::write_sized_buffer(stream, payload)?;
            }
            AuthToCli::ServerAddr { server_addr, token } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::ServerAddr as u16)?;
                stream.write_u32::<LittleEndian>(*server_addr)?;
                token.stream_write(stream)?;
            }
            AuthToCli::NotifyNewBuild { dummy } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::NotifyNewBuild as u16)?;
                stream.write_u32::<LittleEndian>(*dummy)?;
            }
            AuthToCli::ClientRegisterReply { server_challenge } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::ClientRegisterReply as u16)?;
                stream.write_u32::<LittleEndian>(*server_challenge)?;
            }
            AuthToCli::AcctLoginReply { trans_id, result, account_id, account_flags,
                                        billing_type, encryption_key } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::AcctLoginReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                account_id.stream_write(stream)?;
                stream.write_u32::<LittleEndian>(*account_flags)?;
                stream.write_u32::<LittleEndian>(*billing_type)?;
                for key_word in encryption_key {
                    stream.write_u32::<LittleEndian>(*key_word)?;
                }
            }
            AuthToCli::AcctPlayerInfo { trans_id, player_id, player_name,
                                        avatar_shape, explorer } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::AcctPlayerInfo as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_u32::<LittleEndian>(*player_id)?;
                net_io::write_utf16_str(stream, player_name)?;
                net_io::write_utf16_str(stream, avatar_shape)?;
                stream.write_u32::<LittleEndian>(*explorer)?;
            }
            AuthToCli::AcctSetPlayerReply { trans_id, result } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::AcctSetPlayerReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
            }
            AuthToCli::AcctCreateReply { trans_id, result, account_id } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::AcctCreateReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                account_id.stream_write(stream)?;
            }
            AuthToCli::AcctChangePasswordReply { trans_id, result } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::AcctChangePasswordReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
            }
            AuthToCli::AcctSetRolesReply { trans_id, result } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::AcctSetRolesReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
            }
            AuthToCli::AcctSetBillingTypeReply { trans_id, result } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::AcctSetBillingTypeReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
            }
            AuthToCli::AcctActivateReply { trans_id, result } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::AcctActivateReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
            }
            AuthToCli::AcctCreateFromKeyReply { trans_id, result, account_id,
                                                activation_key } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::AcctCreateFromKeyReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                account_id.stream_write(stream)?;
                activation_key.stream_write(stream)?;
            }
            AuthToCli::PlayerCreateReply { trans_id, result, player_id, explorer,
                                           player_name, avatar_shape } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::PlayerCreateReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                stream.write_u32::<LittleEndian>(*player_id)?;
                stream.write_u32::<LittleEndian>(*explorer)?;
                net_io::write_utf16_str(stream, player_name)?;
                net_io::write_utf16_str(stream, avatar_shape)?;
            }
            AuthToCli::PlayerDeleteReply { trans_id, result } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::PlayerDeleteReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
            }
            AuthToCli::UpgradeVisitorReply { trans_id, result } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::UpgradeVisitorReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
            }
            AuthToCli::SetPlayerBanStatusReply { trans_id, result } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::SetPlayerBanStatusReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
            }
            AuthToCli::ChangePlayerNameReply { trans_id, result } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::ChangePlayerNameReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
            }
            AuthToCli::SendFriendInviteReply { trans_id, result } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::SendFriendInviteReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
            }
            AuthToCli::VaultNodeCreated { trans_id, result, node_id } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::VaultNodeCreated as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                stream.write_u32::<LittleEndian>(*node_id)?;
            }
            AuthToCli::VaultNodeFetched { trans_id, result, node_buffer } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::VaultNodeFetched as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                net_io::write_sized_buffer(stream, node_buffer)?;
            }
            AuthToCli::VaultNodeChanged { node_id, revision_id } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::VaultNodeChanged as u16)?;
                stream.write_u32::<LittleEndian>(*node_id)?;
                revision_id.stream_write(stream)?;
            }
            AuthToCli::VaultNodeDeleted { node_id } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::VaultNodeDeleted as u16)?;
                stream.write_u32::<LittleEndian>(*node_id)?;
            }
            AuthToCli::VaultNodeAdded { parent_id, child_id, owner_id } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::VaultNodeAdded as u16)?;
                stream.write_u32::<LittleEndian>(*parent_id)?;
                stream.write_u32::<LittleEndian>(*child_id)?;
                stream.write_u32::<LittleEndian>(*owner_id)?;
            }
            AuthToCli::VaultNodeRemoved { parent_id, child_id } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::VaultNodeRemoved as u16)?;
                stream.write_u32::<LittleEndian>(*parent_id)?;
                stream.write_u32::<LittleEndian>(*child_id)?;
            }
            AuthToCli::VaultNodeRefsFetched { trans_id, result, refs } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::VaultNodeRefsFetched as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                stream.write_u32::<LittleEndian>(refs.len() as u32)?;
                for node_ref in refs {
                    node_ref.stream_write(stream)?;
                }
            }
            AuthToCli::VaultInitAgeReply { trans_id, result, age_vault_id,
                                           age_info_vault_id } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::VaultInitAgeReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                stream.write_u32::<LittleEndian>(*age_vault_id)?;
                stream.write_u32::<LittleEndian>(*age_info_vault_id)?;
            }
            AuthToCli::VaultNodeFindReply { trans_id, result, node_ids } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::VaultNodeFindReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                stream.write_u32::<LittleEndian>(node_ids.len() as u32)?;
                for id in node_ids {
                    stream.write_u32::<LittleEndian>(*id)?;
                }
            }
            AuthToCli::VaultSaveNodeReply { trans_id, result } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::VaultSaveNodeReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
            }
            AuthToCli::VaultAddNodeReply { trans_id, result } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::VaultAddNodeReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
            }
            AuthToCli::VaultRemoveNodeReply { trans_id, result } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::VaultRemoveNodeReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
            }
            AuthToCli::AgeReply { trans_id, result, age_mcp_id, age_instance_id,
                                  age_vault_id, game_server_node } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::AgeReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                stream.write_u32::<LittleEndian>(*age_mcp_id)?;
                age_instance_id.stream_write(stream)?;
                stream.write_u32::<LittleEndian>(*age_vault_id)?;
                stream.write_u32::<LittleEndian>(*game_server_node)?;
            }
            AuthToCli::FileListReply { trans_id, result, manifest } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::FileListReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                manifest.stream_write(stream)?;
            }
            AuthToCli::FileDownloadChunk { trans_id, result, total_size,
                                           offset, file_data } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::FileDownloadChunk as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                stream.write_u32::<LittleEndian>(*total_size)?;
                stream.write_u32::<LittleEndian>(*offset)?;
                net_io::write_sized_buffer(stream, file_data)?;
            }
            AuthToCli::PropagateBuffer { type_id, buffer } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::PropagateBuffer as u16)?;
                stream.write_u32::<LittleEndian>(*type_id)?;
                net_io::write_sized_buffer(stream, buffer)?;
            }
            AuthToCli::KickedOff { reason } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::KickedOff as u16)?;
                stream.write_i32::<LittleEndian>(*reason)?;
            }
            AuthToCli::PublicAgeList { trans_id, result, ages } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::PublicAgeList as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                stream.write_u32::<LittleEndian>(ages.len() as u32)?;
                for age in ages {
                    age.stream_write(stream)?;
                }
            }
            AuthToCli::ScoreCreateReply { trans_id, result, score_id, created_time } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::ScoreCreateReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                stream.write_u32::<LittleEndian>(*score_id)?;
                stream.write_u32::<LittleEndian>(*created_time)?;
            }
            AuthToCli::ScoreDeleteReply { trans_id, result } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::ScoreDeleteReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
            }
            AuthToCli::ScoreGetScoresReply { trans_id, result, score_count, score_buffer } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::ScoreGetScoresReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                stream.write_u32::<LittleEndian>(*score_count)?;
                net_io::write_sized_buffer(stream, score_buffer)?;
            }
            AuthToCli::ScoreAddPointsReply { trans_id, result } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::ScoreAddPointsReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
            }
            AuthToCli::ScoreTransferPointsReply { trans_id, result } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::ScoreTransferPointsReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
            }
            AuthToCli::ScoreSetPointsReply { trans_id, result } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::ScoreSetPointsReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
            }
            AuthToCli::ScoreGetRanksReply { trans_id, result, rank_count, rank_buffer } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::ScoreGetRanksReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                stream.write_u32::<LittleEndian>(*rank_count)?;
                net_io::write_sized_buffer(stream, rank_buffer)?;
            }
            AuthToCli::AccountExistsReply { trans_id, result, exists } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::AccountExistsReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                stream.write_u8(*exists)?;
            }
            AuthToCli::ScoreGetHighScoresReply { trans_id, result, score_count, score_buffer } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::ScoreGetHighScoresReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                stream.write_u32::<LittleEndian>(*score_count)?;
                net_io::write_sized_buffer(stream, score_buffer)?;
            }
            AuthToCli::ServerCaps { caps_buffer } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::ServerCaps as u16)?;
                net_io::write_sized_buffer(stream, caps_buffer)?;
            }
        }

        Ok(())
    }
}
