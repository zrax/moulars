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

use std::sync::Arc;

use tokio::sync::oneshot;
use uuid::Uuid;

use crate::netcli::NetResult;
use super::db_interface::{AccountInfo, PlayerInfo, GameServer};
use super::{VaultNode, NodeRef};

pub(super) enum VaultMessage {
    GetAccount {
        account_name: String,
        response_send: oneshot::Sender<NetResult<Option<AccountInfo>>>,
    },
    GetAccountForToken {
        api_token: String,
        response_send: oneshot::Sender<NetResult<Option<AccountInfo>>>,
    },
    GetPlayers {
        account_id: Uuid,
        response_send: oneshot::Sender<NetResult<Vec<PlayerInfo>>>,
    },
    CreatePlayer {
        account_id: Uuid,
        player_name: String,
        avatar_shape: String,
        response_send: oneshot::Sender<NetResult<PlayerInfo>>,
    },
    AddGameServer {
        game_server: GameServer,
        response_send: oneshot::Sender<NetResult<u32>>,
    },
    FindGameServer {
        age_instance_id: Uuid,
        response_send: oneshot::Sender<NetResult<Option<GameServer>>>,
    },
    CreateNode {
        node: Box<VaultNode>,
        response_send: oneshot::Sender<NetResult<u32>>,
    },
    FetchNode {
        node_id: u32,
        response_send: oneshot::Sender<NetResult<Arc<VaultNode>>>,
    },
    UpdateNode {
        node: Box<VaultNode>,
        revision: Uuid,
        response_send: oneshot::Sender<NetResult<()>>,
    },
    FindNodes {
        template: Box<VaultNode>,
        response_send: oneshot::Sender<NetResult<Vec<u32>>>,
    },
    GetSystemNode {
        response_send: oneshot::Sender<NetResult<u32>>,
    },
    GetAllPlayersNode {
        response_send: oneshot::Sender<NetResult<u32>>,
    },
    GetPlayerInfoNode {
        player_id: u32,
        response_send: oneshot::Sender<NetResult<Arc<VaultNode>>>,
    },
    RefNode {
        parent_id: u32,
        child_id: u32,
        owner_id: u32,
        broadcast: bool,
        response_send: oneshot::Sender<NetResult<()>>,
    },
    FetchRefs {
        parent: u32,
        recursive: bool,
        response_send: oneshot::Sender<NetResult<Vec<NodeRef>>>,
    }
}

#[derive(Clone, Debug)]
pub enum VaultBroadcast {
    NodeChanged {
        node_id: u32,
        revision: Uuid,
    },
    NodeAdded {
        parent_id: u32,
        child_id: u32,
        owner_id: u32,
    },
}
