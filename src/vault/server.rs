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

use log::error;
use tokio::sync::mpsc;

use crate::config::{ServerConfig, VaultDbBackend};
use super::db_interface::DbInterface;
use super::db_memory::DbMemory;
use super::messages::VaultMessage;

pub struct VaultServer {
    msg_send: mpsc::Sender<VaultMessage>,
}

fn process_vault_message(msg: VaultMessage, db: &mut Box<dyn DbInterface>) {
    match msg {
        VaultMessage::LoginRequest { client_challenge, account_name, pass_hash,
                                     response_send } => {
            let reply = db.login_request(client_challenge, account_name, pass_hash);
            let _ = response_send.send(reply);
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

    pub async fn send(&self, msg: VaultMessage) {
        if let Err(err) = self.msg_send.send(msg).await {
            error!("Failed to send message to vault: {}", err);
            std::process::exit(1);
        }
    }
}
