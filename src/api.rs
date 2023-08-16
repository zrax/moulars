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
use std::sync::Arc;

use hyper::service::{make_service_fn, service_fn};
use hyper::{Server, Request, Response, Method, Body, StatusCode};
use hyper::header::CONTENT_TYPE;
use log::{warn, info};
use serde_derive::Serialize;
use tokio::sync::broadcast;

use crate::config::ServerConfig;
use crate::netcli::NetResult;
use crate::vault::{VaultServer, VaultNode};

// Returns the name of the account that matched the API token
async fn check_api_token(query: &HashMap<String, String>, vault: &VaultServer) -> Option<String> {
    if let Some(api_token) = query.get("token") {
        if let Ok(Some(account)) = vault.get_account_for_token(api_token).await {
            // Currently, only Admin accounts are allowed to use APIs
            if account.is_admin() {
                return Some(account.account_name);
            }
        }
    }
    None
}

async fn query_online_players(vault: &VaultServer) -> NetResult<Vec<OnlinePlayer>> {
    let template = VaultNode::player_info_lookup(Some(1));
    let player_list = vault.find_nodes(template).await?;
    let mut players = Vec::with_capacity(player_list.len());
    for player_info in player_list {
        let node = vault.fetch_node(player_info).await?.as_player_info_node().unwrap();
        players.push(OnlinePlayer {
            name: node.player_name_ci().clone(),
            location: node.age_instance_name().clone(),
        });
    }
    Ok(players)
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

async fn api_router(request: Request<Body>, shutdown_send: broadcast::Sender<()>,
                    vault: Arc<VaultServer>) -> Result<Response<Body>, hyper::Error>
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
        (&Method::GET, "/online") => {
            // Return JSON object containing the names and locations of online players
            let online_players = match query_online_players(&vault).await {
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
            if let Some(admin) = check_api_token(&query_params, &vault).await {
                info!("Shutdown requested by {}", admin);
                let _ = shutdown_send.send(());
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
    let mut shutdown_recv = shutdown_send.subscribe();
    tokio::spawn(async move {
        let api_service = make_service_fn(move |_| {
            let vault = vault.clone();
            let shutdown_send = shutdown_send.clone();
            async {
                Ok::<_, hyper::Error>(service_fn(move |request| {
                    api_router(request, shutdown_send.clone(), vault.clone())
                }))
            }
        });

        let api_address = match server_config.api_address.parse() {
            Ok(addr) => addr,
            Err(err) => {
                warn!("Failed to parse API address: {}", err);
                return;
            }
        };
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
