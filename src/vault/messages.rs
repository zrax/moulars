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
use super::VaultNode;

pub(super) enum VaultMessage {
    GetAccount {
        account_name: String,
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
        response_send: oneshot::Sender<NetResult<()>>,
    },
    CreateNode {
        node: Arc<VaultNode>,
        response_send: oneshot::Sender<NetResult<u32>>,
    },
    FetchNode {
        node_id: u32,
        response_send: oneshot::Sender<NetResult<Arc<VaultNode>>>,
    },
    GetSystemNode {
        response_send: oneshot::Sender<NetResult<u32>>,
    },
    RefNode {
        parent: u32,
        child: u32,
        owner: u32,
        response_send: oneshot::Sender<NetResult<()>>,
    },
}
