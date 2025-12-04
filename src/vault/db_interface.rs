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

use async_trait::async_trait;
use serde_derive::Serialize;
use uuid::Uuid;

use crate::hashes::ShaDigest;
use crate::netcli::NetResult;
use super::{VaultNode, NodeRef};

#[async_trait]
pub trait DbInterface: Send + Sync {
    async fn get_account(&self, account_name: &str) -> NetResult<Option<AccountInfo>>;
    async fn get_account_by_id(&self, account_id: &Uuid) -> NetResult<Option<AccountInfo>>;
    async fn get_account_for_token(&self, api_token: &str) -> NetResult<Option<AccountInfo>>;
    async fn create_account(&self, account_name: &str, pass_hash: ShaDigest,
                            account_flags: u32) -> NetResult<AccountInfo>;
    async fn update_account(&self, account_id: &Uuid, pass_hash: Option<ShaDigest>,
                            account_flags: Option<u32>) -> NetResult<()>;
    async fn create_api_token(&self, account_id: &Uuid, comment: &str) -> NetResult<String>;
    async fn get_api_tokens(&self, account_id: &Uuid) -> NetResult<Vec<ApiToken>>;

    async fn set_all_players_offline(&self) -> NetResult<()>;
    async fn get_players(&self, account_id: &Uuid) -> NetResult<Vec<PlayerInfo>>;
    async fn count_players(&self, account_id: &Uuid) -> NetResult<u64>;
    async fn player_exists(&self, player_name: &str) -> NetResult<bool>;

    async fn add_game_server(&self, server: GameServer) -> NetResult<u32>;
    async fn find_game_server(&self, age_instance_id: &Uuid) -> NetResult<Option<GameServer>>;

    async fn create_node(&self, node: VaultNode) -> NetResult<u32>;
    async fn fetch_node(&self, node_id: u32) -> NetResult<Arc<VaultNode>>;
    async fn update_node(&self, node: VaultNode) -> NetResult<Vec<u32>>;
    async fn find_nodes(&self, template: VaultNode) -> NetResult<Vec<u32>>;
    async fn get_system_node(&self) -> NetResult<u32>;
    async fn get_all_players_node(&self) -> NetResult<u32>;
    async fn get_player_info_node(&self, player_id: u32) -> NetResult<Arc<VaultNode>>;

    async fn ref_node(&self, parent: u32, child: u32, owner: u32) -> NetResult<()>;
    async fn fetch_refs(&self, parent: u32, recursive: bool) -> NetResult<Vec<NodeRef>>;
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
    pub const ADMIN: u32        = 1 << 0;
    pub const BETA_TESTER: u32  = 1 << 1;
    pub const BANNED: u32       = 1 << 16;

    pub fn is_admin(&self) -> bool { (self.account_flags & Self::ADMIN) != 0 }
    pub fn is_beta_tester(&self) -> bool { (self.account_flags & Self::BETA_TESTER) != 0 }
    pub fn is_banned(&self) -> bool { (self.account_flags & Self::BANNED) != 0 }

    pub fn can_login_restricted(&self) -> bool {
        (self.account_flags & (Self::ADMIN | Self::BETA_TESTER)) != 0
    }
}

#[derive(Clone, Serialize)]
pub struct ApiToken {
    pub token: String,
    pub comment: String,
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
    pub mcp_id: Option<u32>,
    pub instance_id: Uuid,
    pub age_filename: String,
    pub display_name: String,
    pub age_id: u32,
    pub sdl_id: u32,
    pub temporary: bool,
}
