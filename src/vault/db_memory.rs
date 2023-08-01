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

use std::collections::HashMap;

use unicase::UniCase;
use uuid::Uuid;

use crate::hashes::ShaDigest;
use crate::netcli::NetResultCode;
use crate::vault::messages::LoginReply;
use super::db_interface::{DbInterface, PlayerInfo};

// An ephemeral vault backend that vanishes once the server exits.
pub struct DbMemory {
    accounts: HashMap<UniCase<String>, Account>,
}

struct Account {
    account_name: String,
    account_uuid: Uuid,
    players: Vec<PlayerInfo>
}

impl DbMemory {
    pub fn new() -> Self {
        Self { accounts: HashMap::new() }
    }
}

impl DbInterface for DbMemory {
    fn login_request(&mut self, _client_challenge: u32, account_name: String,
                     _pass_hash: ShaDigest) -> LoginReply
    {
        // In this backend, account logins always succeed.  The password is
        // ignored, and any attempt to log into an account that isn't already
        // created will automatically create a new account.
        let account = self.accounts.entry(UniCase::new(account_name.clone()))
                        .or_insert(Account {
                            account_name,
                            account_uuid: Uuid::new_v4(),
                            players: Vec::new()
                        });

        LoginReply {
            result: NetResultCode::NetSuccess,
            account_id: account.account_uuid,
            players: account.players.clone(),
            account_flags: 0,
            billing_type: 1,
        }
    }
}
