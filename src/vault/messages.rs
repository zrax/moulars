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

use tokio::sync::oneshot;
use uuid::Uuid;

use crate::hashes::ShaDigest;
use crate::netcli::NetResultCode;
use super::db_interface::PlayerInfo;

pub enum VaultMessage {
    LoginRequest {
        client_challenge: u32,
        account_name: String,
        pass_hash: ShaDigest,
        response_send: oneshot::Sender<LoginReply>
    },
}

pub struct LoginReply {
    pub result: NetResultCode,
    pub account_id: Uuid,
    pub players: Vec<PlayerInfo>,
    pub account_flags: u32,
    pub billing_type: u32,
}
