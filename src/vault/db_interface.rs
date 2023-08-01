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

use uuid::Uuid;

use crate::hashes::ShaDigest;

pub trait DbInterface: Sync + Send {
    fn get_account(&mut self, account_name: &str) -> Option<AccountInfo>;
    fn get_players(&mut self, account_id: &Uuid) -> Vec<PlayerInfo>;
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
    pub explorer: u32,
}
