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

use moulars::config::ServerConfig;
use moulars::lobby::LobbyServer;
use moulars::file_srv::cache_clients;

fn main() {
    let config = ServerConfig::dummy_config();

    if let Err(err) = cache_clients(config.as_ref()) {
        eprintln!("Warning: Failed to update file server cache: {}", err);
        // Try to continue anyway...  The file server may be useless in this
        // case though.
    }

    server_main(config);
}

#[tokio::main]
async fn server_main(server_config: Arc<ServerConfig>) {
    LobbyServer::start(server_config).await;
}
