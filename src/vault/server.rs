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

use tokio::sync::{mpsc, oneshot, broadcast};
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::{ServerConfig, VaultDbBackend};
use crate::netcli::{NetResult, NetResultCode};
use crate::sdl::DescriptorDb;
use super::db_interface::{DbInterface, AccountInfo, PlayerInfo, GameServer};
use super::db_memory::DbMemory;
use super::messages::{VaultMessage, VaultBroadcast};
use super::{
    VaultNode, VaultPlayerNode, VaultFolderNode, VaultSystemNode,
    VaultPlayerInfoListNode, StandardNode, NodeRef
};

pub struct VaultServer {
    msg_send: mpsc::Sender<VaultMessage>,
    broadcast: broadcast::Sender<VaultBroadcast>,
    sdl_db: DescriptorDb,
}

const MAX_PLAYERS: u64 = 5;

fn check_send<T>(sender: oneshot::Sender<NetResult<T>>, reply: NetResult<T>) {
    if sender.send(reply).is_err() {
        warn!("Failed to send vault reply to client");
    }
}

fn check_bcast(sender: &broadcast::Sender<VaultBroadcast>, msg: VaultBroadcast) {
    if let Err(err) = sender.send(msg) {
        warn!("Failed to send broadcast: {err}");
    }
}

fn process_vault_message(msg: VaultMessage, bcast_send: &broadcast::Sender<VaultBroadcast>,
                         db: &dyn DbInterface)
{
    match msg {
        VaultMessage::GetAccount { account_name, response_send } => {
            check_send(response_send, db.get_account(&account_name));
        }
        VaultMessage::GetAccountForToken { api_token, response_send } => {
            check_send(response_send, db.get_account_for_token(&api_token));
        }
        VaultMessage::GetPlayers { account_id, response_send } => {
            check_send(response_send, db.get_players(&account_id));
        }
        VaultMessage::CreatePlayer { account_id, player_name, avatar_shape,
                                     response_send } => {
            match db.player_exists(&player_name) {
                Ok(false) => (),
                Ok(true) => {
                    return check_send(response_send, Err(NetResultCode::NetPlayerAlreadyExists));
                }
                Err(err) => return check_send(response_send, Err(err)),
            }
            match db.count_players(&account_id) {
                Ok(count) if count >= MAX_PLAYERS => {
                    return check_send(response_send, Err(NetResultCode::NetMaxPlayersOnAcct));
                }
                Ok(_) => (),
                Err(err) => return check_send(response_send, Err(err)),
            }

            let node = VaultPlayerNode::new(&account_id, &player_name, &avatar_shape, 1);
            let player_id = match db.create_node(node) {
                Ok(node_id) => node_id,
                Err(err) => return check_send(response_send, Err(err)),
            };

            // The rest of the player initialization is handled by the Auth
            // client task, so we don't hold off the Vault task too long.

            let player = PlayerInfo {
                player_id,
                player_name,
                avatar_shape,
                explorer: 1
            };
            if let Err(err) = db.create_player(&account_id, player.clone()) {
                return check_send(response_send, Err(err));
            }
            check_send(response_send, Ok(player));
        }
        VaultMessage::AddGameServer { game_server, response_send } => {
            check_send(response_send, db.add_game_server(game_server));
        }
        VaultMessage::CreateNode { node, response_send } => {
            check_send(response_send, db.create_node(*node));
        }
        VaultMessage::FetchNode { node_id, response_send } => {
            check_send(response_send, db.fetch_node(node_id));
        }
        VaultMessage::UpdateNode { node, response_send } => {
            let updated = match db.update_node(*node) {
                Ok(nodes) => nodes,
                Err(err) => return check_send(response_send, Err(err)),
            };
            for node_id in updated {
                check_bcast(bcast_send, VaultBroadcast::NodeChanged {
                    node_id,
                    revision_id: Uuid::new_v4(),
                });
            }
            check_send(response_send, Ok(()));
        }
        VaultMessage::FindNodes { template, response_send } => {
            check_send(response_send, db.find_nodes(*template));
        }
        VaultMessage::GetSystemNode { response_send } => {
            check_send(response_send, db.get_system_node());
        }
        VaultMessage::GetAllPlayersNode { response_send } => {
            check_send(response_send, db.get_all_players_node());
        }
        VaultMessage::GetPlayerInfoNode { player_id, response_send } => {
            check_send(response_send, db.get_player_info_node(player_id));
        }
        VaultMessage::RefNode { parent_id, child_id, owner_id, response_send,
                                broadcast } => {
            if let Err(err) = db.ref_node(parent_id, child_id, owner_id) {
                return check_send(response_send, Err(err));
            }
            if broadcast {
                check_bcast(bcast_send, VaultBroadcast::NodeAdded {
                    parent_id, child_id, owner_id
                });
            }
            check_send(response_send, Ok(()));
        }
        VaultMessage::FetchRefs { parent, recursive, response_send } => {
            check_send(response_send, db.fetch_refs(parent, recursive));
        }
    }
}

impl VaultServer {
    pub fn start(server_config: Arc<ServerConfig>, sdl_db: DescriptorDb) -> Self {
        let (msg_send, mut msg_recv) = mpsc::channel(20);
        let (bcast_send, _) = broadcast::channel(100);

        let broadcast = bcast_send.clone();
        tokio::spawn(async move {
            let db: Box<dyn DbInterface> =  match server_config.db_type {
                VaultDbBackend::None => Box::new(DbMemory::new()),
                VaultDbBackend::Sqlite => todo!(),
                VaultDbBackend::Postgres => todo!(),
            };

            assert!(init_vault(db.as_ref()).is_ok(), "Failed to initialize vault.");

            if db.set_all_players_offline().is_err() {
                warn!("Failed to set all players offline.");
            }

            // TODO: Check and update Global SDL
            // TODO: Check and initialize static ages

            while let Some(msg) = msg_recv.recv().await {
                process_vault_message(msg, &bcast_send, db.as_ref());
            }
        });
        Self { msg_send, broadcast, sdl_db }
    }

    pub fn sdl_db(&self) -> &DescriptorDb { &self.sdl_db }

    pub fn subscribe(&self) -> broadcast::Receiver<VaultBroadcast> {
        self.broadcast.subscribe()
    }

    async fn request<T>(&self, msg: VaultMessage, recv: oneshot::Receiver<NetResult<T>>)
        -> NetResult<T>
    {
        if let Err(err) = self.msg_send.send(msg).await {
            panic!("Failed to send message to vault: {err}");
        }

        recv.await.unwrap_or_else(|err| {
            warn!("Failed to recieve response from Vault: {err}");
            Err(NetResultCode::NetInternalError)
        })
    }

    pub async fn get_account(&self, account_name: &str) -> NetResult<Option<AccountInfo>> {
        let (response_send, response_recv) = oneshot::channel();
        let request = VaultMessage::GetAccount {
            account_name: account_name.to_string(),
            response_send
        };
        self.request(request, response_recv).await
    }

    pub async fn get_account_for_token(&self, api_token: &str)
            -> NetResult<Option<AccountInfo>>
    {
        let (response_send, response_recv) = oneshot::channel();
        let request = VaultMessage::GetAccountForToken {
            api_token: api_token.to_string(),
            response_send
        };
        self.request(request, response_recv).await
    }

    pub async fn get_players(&self, account_id: &Uuid) -> NetResult<Vec<PlayerInfo>> {
        let (response_send, response_recv) = oneshot::channel();
        let request = VaultMessage::GetPlayers {
            account_id: *account_id,
            response_send
        };
        self.request(request, response_recv).await
    }

    pub async fn create_player(&self, account_id: &Uuid, player_name: &str,
                               avatar_shape: &str) -> NetResult<PlayerInfo>
    {
        let (response_send, response_recv) = oneshot::channel();
        let request = VaultMessage::CreatePlayer {
            account_id: *account_id,
            player_name: player_name.to_string(),
            avatar_shape: avatar_shape.to_string(),
            response_send
        };
        self.request(request, response_recv).await
    }

    pub async fn add_game_server(&self, game_server: GameServer) -> NetResult<()> {
        let (response_send, response_recv) = oneshot::channel();
        let request = VaultMessage::AddGameServer { game_server, response_send };
        self.request(request, response_recv).await
    }

    pub async fn create_node(&self, node: VaultNode) -> NetResult<u32> {
        let (response_send, response_recv) = oneshot::channel();
        let request = VaultMessage::CreateNode {
            node: Box::new(node),
            response_send
        };
        self.request(request, response_recv).await
    }

    pub async fn fetch_node(&self, node_id: u32) -> NetResult<Arc<VaultNode>> {
        let (response_send, response_recv) = oneshot::channel();
        let request = VaultMessage::FetchNode { node_id, response_send };
        self.request(request, response_recv).await
    }

    pub async fn update_node(&self, node: VaultNode) -> NetResult<()> {
        let (response_send, response_recv) = oneshot::channel();
        let request = VaultMessage::UpdateNode {
            node: Box::new(node),
            response_send
        };
        self.request(request, response_recv).await
    }

    pub async fn find_nodes(&self, template: VaultNode) -> NetResult<Vec<u32>> {
        let (response_send, response_recv) = oneshot::channel();
        let request = VaultMessage::FindNodes {
            template: Box::new(template),
            response_send
        };
        self.request(request, response_recv).await
    }

    pub async fn get_system_node(&self) -> NetResult<u32> {
        let (response_send, response_recv) = oneshot::channel();
        let request = VaultMessage::GetSystemNode { response_send };
        self.request(request, response_recv).await
    }

    pub async fn get_all_players_node(&self) -> NetResult<u32> {
        let (response_send, response_recv) = oneshot::channel();
        let request = VaultMessage::GetAllPlayersNode { response_send };
        self.request(request, response_recv).await
    }

    pub async fn get_player_info_node(&self, player_id: u32) -> NetResult<Arc<VaultNode>> {
        let (response_send, response_recv) = oneshot::channel();
        let request = VaultMessage::GetPlayerInfoNode { player_id, response_send };
        self.request(request, response_recv).await
    }

    pub async fn ref_node(&self, parent_id: u32, child_id: u32, owner_id: u32,
                          broadcast: bool) -> NetResult<()>
    {
        let (response_send, response_recv) = oneshot::channel();
        let request = VaultMessage::RefNode {
            parent_id, child_id, owner_id, response_send, broadcast
        };
        self.request(request, response_recv).await
    }

    pub async fn fetch_refs(&self, parent: u32, recursive: bool) -> NetResult<Vec<NodeRef>> {
        let (response_send, response_recv) = oneshot::channel();
        let request = VaultMessage::FetchRefs { parent, recursive, response_send };
        self.request(request, response_recv).await
    }
}

fn init_vault(db: &dyn DbInterface) -> NetResult<()> {
    if let Err(err) = db.get_system_node() {
        if err != NetResultCode::NetVaultNodeNotFound {
            warn!("Failed to fetch system node");
            return Err(err);
        }

        info!("Initializing empty Vault database");

        let node = VaultSystemNode::new();
        let system_node = db.create_node(node)?;

        let node = VaultFolderNode::new(&Uuid::nil(), 0, StandardNode::GlobalInboxFolder);
        let global_inbox = db.create_node(node)?;
        db.ref_node(system_node, global_inbox, 0)?;

        let node = VaultPlayerInfoListNode::new(&Uuid::nil(), 0,
                                                StandardNode::AllPlayersFolder);
        let _ = db.create_node(node)?;
    }

    Ok(())
}
