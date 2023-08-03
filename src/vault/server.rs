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
use super::db_interface::{DbInterface, AccountInfo, PlayerInfo};
use super::db_memory::DbMemory;
use super::messages::VaultMessage;

pub struct VaultServer {
    msg_send: mpsc::Sender<VaultMessage>,
}

fn check_send<T>(sender: oneshot::Sender<NetResult<T>>, reply: NetResult<T>)
{
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
    }
}

impl VaultServer {
    pub fn start(server_config: Arc<ServerConfig>) -> Arc<VaultServer> {
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
        Arc::new(VaultServer { msg_send })
    }

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
}
