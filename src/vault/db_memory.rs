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

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use log::warn;
use unicase::UniCase;
use uuid::Uuid;

use crate::auth_srv::auth_hash::create_pass_hash;
use crate::netcli::{NetResult, NetResultCode};
use crate::vault::{VaultNode, NodeType, StandardNode, NodeRef};
use super::db_interface::{DbInterface, AccountInfo, PlayerInfo, GameServer};

// An ephemeral vault backend that vanishes once the server exits.
pub struct DbMemory {
    accounts: HashMap<UniCase<String>, AccountInfo>,
    players: HashMap<Uuid, Vec<PlayerInfo>>,
    game_servers: HashMap<u32, GameServer>,
    game_index: AtomicU32,
    vault: HashMap<u32, Arc<VaultNode>>,
    node_refs: HashSet<NodeRef>,
    node_index: AtomicU32,
}

impl DbMemory {
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
            players: HashMap::new(),
            game_servers: HashMap::new(),
            game_index: AtomicU32::new(1),
            vault: HashMap::new(),
            node_refs: HashSet::new(),
            node_index: AtomicU32::new(1000),
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

    fn get_players(&self, account_id: &Uuid) -> NetResult<Vec<PlayerInfo>> {
        if let Some(players) = self.players.get(account_id) {
            Ok(players.clone())
        } else {
            Ok(Vec::new())
        }
    }

    fn count_players(&self, account_id: &Uuid) -> NetResult<u64> {
        if let Some(players) = self.players.get(account_id) {
            Ok(players.len() as u64)
        } else {
            Ok(0)
        }
    }

    fn get_player_by_name(&self, player_name: &str) -> NetResult<Option<PlayerInfo>> {
        for player_list in self.players.values() {
            for player in player_list {
                if player.player_name == player_name {
                    return Ok(Some(player.clone()));
                }
            }
        }
        Ok(None)
    }

    fn create_player(&mut self, account_id: &Uuid, player: PlayerInfo) -> NetResult<()> {
        self.players.entry(*account_id)
                .or_insert(Vec::new())
                .push(player);
        Ok(())
    }

    fn add_game_server(&mut self, server: GameServer) -> NetResult<()> {
        let server_id = self.game_index.fetch_add(1, Ordering::Relaxed);
        if self.game_servers.insert(server_id, server).is_some() {
            warn!("Created duplicate game server ID {}!", server_id);
            Err(NetResultCode::NetInternalError)
        } else {
            Ok(())
        }
    }

    fn create_node(&mut self, node: Arc<VaultNode>) -> NetResult<u32> {
        let node_id = self.node_index.fetch_add(1, Ordering::Relaxed);
        if self.vault.insert(node_id, node).is_some() {
            warn!("Created duplicate node ID {}!", node_id);
            Err(NetResultCode::NetInternalError)
        } else {
            Ok(node_id)
        }
    }

    fn fetch_node(&self, node_id: u32) -> NetResult<Arc<VaultNode>> {
        match self.vault.get(&node_id) {
            Some(node) => Ok(node.clone()),
            None => Err(NetResultCode::NetVaultNodeNotFound),
        }
    }

    fn get_system_node(&self) -> NetResult<u32> {
        for (node_id, node) in &self.vault {
            if node.node_type() == NodeType::System as i32 {
                return Ok(*node_id);
            }
        }
        Err(NetResultCode::NetVaultNodeNotFound)
    }

    fn get_all_players_node(&self) -> NetResult<u32> {
        for (node_id, node) in &self.vault {
            if node.node_type() == NodeType::PlayerInfoList as i32
                    && node.int32_1() == StandardNode::AllPlayersFolder as i32
            {
                return Ok(*node_id);
            }
        }
        Err(NetResultCode::NetVaultNodeNotFound)
    }

    fn ref_node(&mut self, parent: u32, child: u32, owner: u32) -> NetResult<()> {
        self.node_refs.insert(NodeRef::new(parent, child, owner));
        Ok(())
    }

    fn get_children(&self, parent: u32, recursive: bool) -> NetResult<Vec<NodeRef>> {
        let mut children = Vec::new();
        for node_ref in &self.node_refs {
            if node_ref.parent() == parent {
                children.push(*node_ref);
                if recursive {
                    children.extend_from_slice(&self.get_children(node_ref.child(), true)?);
                }
            }
        }
        Ok(children)
    }
}
