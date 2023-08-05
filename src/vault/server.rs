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

use log::{warn, error};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::config::{ServerConfig, VaultDbBackend};
use crate::netcli::{NetResult, NetResultCode};
use crate::sdl::DescriptorDb;
use super::db_interface::{DbInterface, AccountInfo, PlayerInfo, GameServer};
use super::db_memory::DbMemory;
use super::messages::VaultMessage;
use super::VaultNode;

pub struct VaultServer {
    msg_send: mpsc::Sender<VaultMessage>,
    sdl_db: DescriptorDb,
}

const MAX_PLAYERS: u64 = 5;

fn check_send<T>(sender: oneshot::Sender<NetResult<T>>, reply: NetResult<T>) {
    match sender.send(reply) {
        Ok(()) => (),
        Err(_) => warn!("Failed to send vault reply to client"),
    }
}

fn process_vault_message(msg: VaultMessage, db: &mut Box<dyn DbInterface>) {
    match msg {
        VaultMessage::GetAccount { account_name, response_send } => {
            let reply = db.get_account(&account_name);
            check_send(response_send, reply);
        }
        VaultMessage::GetPlayers { account_id, response_send } => {
            let reply = db.get_players(&account_id);
            check_send(response_send, reply);
        }
        VaultMessage::CreatePlayer { account_id, player_name, avatar_shape,
                                     response_send } => {
            match db.get_player_by_name(&player_name) {
                Ok(None) => (),
                Ok(Some(_)) => {
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

            let node = VaultNode::new_player(&account_id, &player_name, &avatar_shape, 1);
            let player_id = match db.create_node(Box::new(node)) {
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
            let reply = db.add_game_server(game_server);
            check_send(response_send, reply);
        }
        VaultMessage::CreateNode { node, response_send } => {
            let reply = db.create_node(node);
            check_send(response_send, reply);
        }
        VaultMessage::RefNode { parent, child, owner, response_send } => {
            let reply = db.ref_node(parent, child, owner);
            check_send(response_send, reply);
        }
    }
}

impl VaultServer {
    pub fn start(server_config: Arc<ServerConfig>, sdl_db: DescriptorDb)
            -> Arc<VaultServer>
    {
        let (msg_send, mut msg_recv) = mpsc::channel(20);

        tokio::spawn(async move {
            let mut db: Box<dyn DbInterface> =  match server_config.db_type {
                VaultDbBackend::None => Box::new(DbMemory::new()),
                VaultDbBackend::Sqlite => todo!(),
                VaultDbBackend::Postgres => todo!(),
            };

            while let Some(msg) = msg_recv.recv().await {
                process_vault_message(msg, &mut db);
            }
        });
        Arc::new(VaultServer { msg_send, sdl_db })
    }

    pub fn sdl_db(&self) -> &DescriptorDb { &self.sdl_db }

    async fn request<T>(&self, msg: VaultMessage, recv: oneshot::Receiver<NetResult<T>>)
        -> NetResult<T>
    {
        if let Err(err) = self.msg_send.send(msg).await {
            error!("Failed to send message to vault: {}", err);
            std::process::exit(1);
        }

        match recv.await {
            Ok(response) => response,
            Err(err) => {
                warn!("Failed to recieve response from Vault: {}", err);
                Err(NetResultCode::NetInternalError)
            }
        }
    }

    pub async fn get_account(&self, account_name: &str) -> NetResult<Option<AccountInfo>> {
        let (response_send, response_recv) = oneshot::channel();
        let request = VaultMessage::GetAccount {
            account_name: account_name.to_string(),
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

    pub async fn ref_node(&self, parent: u32, child: u32, owner: u32) -> NetResult<()> {
        let (response_send, response_recv) = oneshot::channel();
        let request = VaultMessage::RefNode { parent, child, owner, response_send };
        self.request(request, response_recv).await
    }
}
