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

    fn set_all_players_offline(&mut self) -> NetResult<()> {
        // This doesn't have to do anything here -- we always start in a clean
        // state with all players offline.
        Ok(())
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

    fn player_exists(&self, player_name: &str) -> NetResult<bool> {
        let player_name_ci = UniCase::new(player_name);
        Ok(self.players.iter().any(|(_, player_list)| {
            player_list.iter().any(|player| {
                UniCase::new(&player.player_name) == player_name_ci
            })
        }))
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
        let mut node = (*node).clone();
        node.set_node_id(node_id);
        if self.vault.insert(node_id, Arc::new(node)).is_some() {
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

    fn update_node(&mut self, node: Arc<VaultNode>) -> NetResult<Vec<u32>> {
        let node_id = node.node_id();
        let old_node = match self.vault.get(&node_id) {
            Some(node) => node,
            None => return Err(NetResultCode::NetVaultNodeNotFound),
        };
        let new_node = update_node(old_node, &node);
        self.vault.insert(node_id, new_node);
        Ok(vec![node_id])
    }

    fn find_nodes(&self, template: Arc<VaultNode>) -> NetResult<Vec<u32>> {
        Ok(self.vault.values().filter_map(|node| {
            if node_match(template.as_ref(), node.as_ref()) {
                Some(node.node_id())
            } else {
                None
            }
        }).collect())
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

    fn get_player_info_node(&self, player_id: u32) -> NetResult<Arc<VaultNode>> {
        // Obviously this can be a bit more efficient in SQL...
        for node_ref in self.fetch_refs(player_id, false)? {
            if let Some(node) = self.vault.get(&node_ref.child()) {
                if let Some(player_info) = node.as_player_info_node() {
                    if player_info.player_id() == player_id {
                        return Ok(node.clone());
                    }
                }
            }
        }
        Err(NetResultCode::NetVaultNodeNotFound)
    }

    fn ref_node(&mut self, parent: u32, child: u32, owner: u32) -> NetResult<()> {
        self.node_refs.insert(NodeRef::new(parent, child, owner));
        Ok(())
    }

    fn fetch_refs(&self, parent: u32, recursive: bool) -> NetResult<Vec<NodeRef>> {
        let mut refs = Vec::new();
        for node_ref in &self.node_refs {
            if node_ref.parent() == parent {
                refs.push(*node_ref);
                if recursive {
                    refs.extend_from_slice(&self.fetch_refs(node_ref.child(), true)?);
                }
            }
        }
        Ok(refs)
    }
}

fn node_match(template: &VaultNode, node: &VaultNode) -> bool {
    if template.has_create_time() && node.create_time() != template.create_time() {
        return false;
    }
    if template.has_modify_time() && node.modify_time() != template.modify_time() {
        return false;
    }
    if template.has_create_age_name() && node.create_age_name() != template.create_age_name() {
        return false;
    }
    if template.has_create_age_uuid() && node.create_age_uuid() != template.create_age_uuid() {
        return false;
    }
    if template.has_creator_uuid() && node.creator_uuid() != template.creator_uuid() {
        return false;
    }
    if template.has_creator_id() && node.creator_id() != template.creator_id() {
        return false;
    }
    if template.has_node_type() && node.node_type() != template.node_type() {
        return false;
    }
    if template.has_int32_1() && node.int32_1() != template.int32_1() {
        return false;
    }
    if template.has_int32_2() && node.int32_2() != template.int32_2() {
        return false;
    }
    if template.has_int32_3() && node.int32_3() != template.int32_3() {
        return false;
    }
    if template.has_int32_4() && node.int32_4() != template.int32_4() {
        return false;
    }
    if template.has_uint32_1() && node.uint32_1() != template.uint32_1() {
        return false;
    }
    if template.has_uint32_2() && node.uint32_2() != template.uint32_2() {
        return false;
    }
    if template.has_uint32_3() && node.uint32_3() != template.uint32_3() {
        return false;
    }
    if template.has_uint32_4() && node.uint32_4() != template.uint32_4() {
        return false;
    }
    if template.has_uuid_1() && node.uuid_1() != template.uuid_1() {
        return false;
    }
    if template.has_uuid_2() && node.uuid_2() != template.uuid_2() {
        return false;
    }
    if template.has_uuid_3() && node.uuid_3() != template.uuid_3() {
        return false;
    }
    if template.has_uuid_4() && node.uuid_4() != template.uuid_4() {
        return false;
    }
    if template.has_string64_1() && node.string64_1() != template.string64_1() {
        return false;
    }
    if template.has_string64_2() && node.string64_2() != template.string64_2() {
        return false;
    }
    if template.has_string64_3() && node.string64_3() != template.string64_3() {
        return false;
    }
    if template.has_string64_4() && node.string64_4() != template.string64_4() {
        return false;
    }
    if template.has_string64_5() && node.string64_5() != template.string64_5() {
        return false;
    }
    if template.has_string64_6() && node.string64_6() != template.string64_6() {
        return false;
    }
    if template.has_istring64_1() && UniCase::new(node.istring64_1()) != UniCase::new(template.istring64_1()) {
        return false;
    }
    if template.has_istring64_2() && UniCase::new(node.istring64_2()) != UniCase::new(template.istring64_2()) {
        return false;
    }
    if template.has_text_1() && node.text_1() != template.text_1() {
        return false;
    }
    if template.has_text_2() && node.text_2() != template.text_2() {
        return false;
    }
    if template.has_blob_1() && node.blob_1() != template.blob_1() {
        return false;
    }
    if template.has_blob_2() && node.blob_2() != template.blob_2() {
        return false;
    }
    true
}

fn update_node(old_node: &VaultNode, new_node: &VaultNode) -> Arc<VaultNode> {
    let mut node = old_node.clone();
    if new_node.has_create_time() {
        node.set_create_time(new_node.create_time());
    }
    if new_node.has_modify_time() {
        node.set_modify_time(new_node.modify_time());
    }
    if new_node.has_create_age_name() {
        node.set_create_age_name(new_node.create_age_name());
    }
    if new_node.has_create_age_uuid() {
        node.set_create_age_uuid(new_node.create_age_uuid());
    }
    if new_node.has_creator_uuid() {
        node.set_creator_uuid(new_node.creator_uuid());
    }
    if new_node.has_creator_id() {
        node.set_creator_id(new_node.creator_id());
    }
    if new_node.has_node_type() {
        node.set_node_type(new_node.node_type());
    }
    if new_node.has_int32_1() {
        node.set_int32_1(new_node.int32_1());
    }
    if new_node.has_int32_2() {
        node.set_int32_2(new_node.int32_2());
    }
    if new_node.has_int32_3() {
        node.set_int32_3(new_node.int32_3());
    }
    if new_node.has_int32_4() {
        node.set_int32_4(new_node.int32_4());
    }
    if new_node.has_uint32_1() {
        node.set_uint32_1(new_node.uint32_1());
    }
    if new_node.has_uint32_2() {
        node.set_uint32_2(new_node.uint32_2());
    }
    if new_node.has_uint32_3() {
        node.set_uint32_3(new_node.uint32_3());
    }
    if new_node.has_uint32_4() {
        node.set_uint32_4(new_node.uint32_4());
    }
    if new_node.has_uuid_1() {
        node.set_uuid_1(new_node.uuid_1());
    }
    if new_node.has_uuid_2() {
        node.set_uuid_2(new_node.uuid_2());
    }
    if new_node.has_uuid_3() {
        node.set_uuid_3(new_node.uuid_3());
    }
    if new_node.has_uuid_4() {
        node.set_uuid_4(new_node.uuid_4());
    }
    if new_node.has_string64_1() {
        node.set_string64_1(new_node.string64_1());
    }
    if new_node.has_string64_2() {
        node.set_string64_2(new_node.string64_2());
    }
    if new_node.has_string64_3() {
        node.set_string64_3(new_node.string64_3());
    }
    if new_node.has_string64_4() {
        node.set_string64_4(new_node.string64_4());
    }
    if new_node.has_string64_5() {
        node.set_string64_5(new_node.string64_5());
    }
    if new_node.has_string64_6() {
        node.set_string64_6(new_node.string64_6());
    }
    if new_node.has_istring64_1() {
        node.set_istring64_1(new_node.istring64_1());
    }
    if new_node.has_istring64_2() {
        node.set_istring64_2(new_node.istring64_2());
    }
    if new_node.has_text_1() {
        node.set_text_1(new_node.text_1());
    }
    if new_node.has_text_2() {
        node.set_text_2(new_node.text_2());
    }
    if new_node.has_blob_1() {
        node.set_blob_1(new_node.blob_1());
    }
    if new_node.has_blob_2() {
        node.set_blob_2(new_node.blob_2());
    }

    Arc::new(node)
}
