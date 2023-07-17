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

#[cfg(debug_assertions)]
const DEFAULT_LOG_LEVEL: &str = "debug";

#[cfg(not(debug_assertions))]
const DEFAULT_LOG_LEVEL: &str = "warn";

fn main() {
    // See https://docs.rs/env_logger/latest/env_logger/index.html for
    // details on fine-tuning logging behavior beyond the defaults.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(DEFAULT_LOG_LEVEL)
    ).init();

    let config = ServerConfig::dummy_config();
    server_main(config);
}

#[tokio::main]
async fn server_main(server_config: Arc<ServerConfig>) {
    LobbyServer::start(server_config).await;
}
