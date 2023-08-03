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

use log::warn;
use unicase::UniCase;
use uuid::Uuid;

use crate::auth_srv::auth_hash::create_pass_hash;
use crate::netcli::{NetResult, NetResultCode};
use super::db_interface::{DbInterface, AccountInfo, PlayerInfo};

// An ephemeral vault backend that vanishes once the server exits.
pub struct DbMemory {
    accounts: HashMap<UniCase<String>, AccountInfo>,
    players: HashMap<Uuid, Vec<PlayerInfo>>,
}

impl DbMemory {
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
            players: HashMap::new(),
        }
    }
}

impl DbInterface for DbMemory {
    fn get_account(&mut self, account_name: &str) -> NetResult<Option<AccountInfo>> {
        // In this backend, account logins always succeed.  The password is
        // assumed to be blank, and any attempt to log into an account that
        // isn't already created will automatically create a new account.
        let pass_hash = create_pass_hash(account_name, "").map_err(|err| {
                            warn!("Failed to create password hash: {}", err);
                            NetResultCode::NetInternalError
                        })?;
        let account = self.accounts.entry(UniCase::new(account_name.to_string()))
                        .or_insert(AccountInfo {
                            account_name: account_name.to_string(),
                            pass_hash,
                            account_id: Uuid::new_v4(),
                            account_flags: 0,
                            billing_type: 1,
                        });
        Ok(Some(account.clone()))
    }

    fn get_players(&mut self, account_id: &Uuid) -> NetResult<Vec<PlayerInfo>> {
        if let Some(players) = self.players.get(account_id) {
            Ok(players.clone())
        } else {
            Ok(Vec::new())
        }
    }
}
