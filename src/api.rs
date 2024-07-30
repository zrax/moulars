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
use std::io::Write;
use std::sync::Arc;

use base64::prelude::*;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Server, Request, Response, Method, Body, StatusCode};
use hyper::header::CONTENT_TYPE;
use log::{warn, info};
use num_bigint::ToBigUint;
use serde_derive::Serialize;
use tokio::sync::broadcast;

use crate::config::ServerConfig;
use crate::net_crypt::{CRYPT_BASE_AUTH, CRYPT_BASE_GAME, CRYPT_BASE_GATE_KEEPER};
use crate::netcli::NetResult;
use crate::vault::{VaultServer, VaultNode};

struct ApiInterface {
    server_config: Arc<ServerConfig>,
    shutdown_send: broadcast::Sender<()>,
    vault: Arc<VaultServer>,
}

impl ApiInterface {
    // Returns the name of the account that matched the API token
    async fn check_api_token(&self, query: &HashMap<String, String>) -> Option<String> {
        if let Some(api_token) = query.get("token") {
            if let Ok(Some(account)) = self.vault.get_account_for_token(api_token).await {
                // Currently, only Admin accounts are allowed to use privileged APIs
                if account.is_admin() {
                    return Some(account.account_name);
                }
            }
        }
        None
    }

    async fn query_online_players(&self) -> NetResult<Vec<OnlinePlayer>> {
        let template = VaultNode::player_info_lookup(Some(1));
        let player_list = self.vault.find_nodes(template).await?;
        let mut players = Vec::with_capacity(player_list.len());
        for player_info in player_list {
            let node = self.vault.fetch_node(player_info).await?.as_player_info_node().unwrap();
            players.push(OnlinePlayer {
                name: node.player_name_ci().clone(),
                location: node.age_instance_name().clone(),
            });
        }
        Ok(players)
    }
}

fn gen_unauthorized() -> Response<Body> {
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"error": "Unauthorized"}"#))
        .unwrap()
}

fn gen_server_error() -> Response<Body> {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"error": "Internal Server Error"}"#))
        .unwrap()
}

async fn api_router(request: Request<Body>, api: Arc<ApiInterface>)
        -> Result<Response<Body>, hyper::Error>
{
    let query_params = if let Some(query) = request.uri().query() {
        form_urlencoded::parse(query.as_bytes()).into_owned()
                .collect::<HashMap<String, String>>()
    } else {
        HashMap::new()
    };

    let response = match (request.method(), request.uri().path()) {
        (&Method::GET, "/") => {
            // Basic status check
            Response::builder().body(Body::from("OK")).unwrap()
        }
        (&Method::GET, "/client_keys") => {
            let mut lines = Vec::with_capacity(6 * 105);
            for (stype, key_g, key_k, key_n) in [
                ("Auth", CRYPT_BASE_AUTH, &api.server_config.auth_k_key, &api.server_config.auth_n_key),
                ("Game", CRYPT_BASE_GAME, &api.server_config.game_k_key, &api.server_config.game_n_key),
                ("Gate", CRYPT_BASE_GATE_KEEPER, &api.server_config.gate_k_key, &api.server_config.gate_n_key)]
            {
                let key_x = key_g.to_biguint().unwrap().modpow(key_k, key_n);
                let bytes_n = key_n.to_bytes_be();
                let bytes_x = key_x.to_bytes_be();
                let _ = writeln!(lines, "Server.{}.N \"{}\"", stype, BASE64_STANDARD.encode(bytes_n));
                let _ = writeln!(lines, "Server.{}.X \"{}\"", stype, BASE64_STANDARD.encode(bytes_x));
            }
            Response::builder().body(Body::from(lines)).unwrap()
        }
        (&Method::GET, "/online") => {
            // Return JSON object containing the names and locations of online players
            let online_players = match api.query_online_players().await {
                Ok(response) => response,
                Err(err) => {
                    warn!("Failed to query online players: {:?}", err);
                    return Ok(gen_server_error());
                }
            };
            match serde_json::to_string(&online_players) {
                Ok(json) => Response::builder()
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(json))
                    .unwrap(),
                Err(err) => {
                    warn!("Failed to generate JSON: {}", err);
                    gen_server_error()
                }
            }
        }
        (&Method::POST, "/shutdown") => {
            if let Some(admin) = api.check_api_token(&query_params).await {
                info!("Shutdown requested by {}", admin);
                let _ = api.shutdown_send.send(());
                Response::builder()
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"status": "ok"}"#))
                    .unwrap()
            } else {
                gen_unauthorized()
            }
        }
        _ => {
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"error": "Invalid API Request"}"#))
                .unwrap()
        }
    };
    Ok(response)
}

pub fn start_api(shutdown_send: broadcast::Sender<()>, vault: Arc<VaultServer>,
                 server_config: Arc<ServerConfig>)
{
    tokio::spawn(async move {
        let api_address = match server_config.api_address.parse() {
            Ok(addr) => addr,
            Err(err) => {
                warn!("Failed to parse API address: {}", err);
                return;
            }
        };

        let mut shutdown_recv = shutdown_send.subscribe();
        let api = Arc::new(ApiInterface {
            server_config,
            shutdown_send,
            vault,
        });
        let api_service = make_service_fn(|_| {
            let api = api.clone();
            async {
                Ok::<_, hyper::Error>(service_fn(move |request| {
                    api_router(request, api.clone())
                }))
            }
        });

        let server = Server::bind(&api_address).serve(api_service)
                .with_graceful_shutdown(async { let _ = shutdown_recv.recv().await; });
        info!("Starting API service on http://{}", api_address);

        if let Err(err) = server.await {
            warn!("API service error: {}", err);
        }
        info!("Shutting down API service");
    });
}

#[derive(Serialize)]
struct OnlinePlayer {
    name: String,
    location: String,
}
