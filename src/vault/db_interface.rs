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

use uuid::Uuid;

use crate::hashes::ShaDigest;
use crate::netcli::NetResult;
use super::{VaultNode, NodeRef};

pub trait DbInterface: Sync + Send {
    fn get_account(&mut self, account_name: &str) -> NetResult<Option<AccountInfo>>;

    fn set_all_players_offline(&mut self) -> NetResult<()>;
    fn get_players(&self, account_id: &Uuid) -> NetResult<Vec<PlayerInfo>>;
    fn count_players(&self, account_id: &Uuid) -> NetResult<u64>;
    fn player_exists(&self, player_name: &str) -> NetResult<bool>;
    fn create_player(&mut self, account_id: &Uuid, player: PlayerInfo) -> NetResult<()>;

    fn add_game_server(&mut self, server: GameServer) -> NetResult<()>;

    fn create_node(&mut self, node: Arc<VaultNode>) -> NetResult<u32>;
    fn fetch_node(&self, node_id: u32) -> NetResult<Arc<VaultNode>>;
    fn update_node(&mut self, node: Arc<VaultNode>) -> NetResult<Vec<u32>>;
    fn find_nodes(&self, template: Arc<VaultNode>) -> NetResult<Vec<u32>>;
    fn get_system_node(&self) -> NetResult<u32>;
    fn get_all_players_node(&self) -> NetResult<u32>;
    fn get_player_info_node(&self, player_id: u32) -> NetResult<Arc<VaultNode>>;

    fn ref_node(&mut self, parent: u32, child: u32, owner: u32) -> NetResult<()>;
    fn fetch_refs(&self, parent: u32, recursive: bool) -> NetResult<Vec<NodeRef>>;
}

#[derive(Clone)]
pub struct AccountInfo {
    pub account_name: String,
    pub pass_hash: ShaDigest,
    pub account_id: Uuid,
    pub account_flags: u32,
    pub billing_type: u32,
}

impl AccountInfo {
    // Account flags
    const ADMIN: u32        = 1 << 0;
    const BETA_TESTER: u32  = 1 << 1;
    const BANNED: u32       = 1 << 16;

    pub fn is_banned(&self) -> bool { (self.account_flags & Self::BANNED) != 0 }

    pub fn can_login_restricted(&self) -> bool {
        (self.account_flags & (Self::ADMIN | Self::BETA_TESTER)) != 0
    }
}

#[derive(Clone)]
pub struct PlayerInfo {
    pub player_id: u32,
    pub player_name: String,
    pub avatar_shape: String,
    pub explorer: i32,
}

#[derive(Clone)]
pub struct GameServer {
    pub instance_id: Uuid,
    pub age_filename: String,
    pub display_name: String,
    pub age_id: u32,
    pub sdl_id: u32,
    pub temporary: bool,
}
