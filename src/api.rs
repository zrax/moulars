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
use std::convert::Infallible;
use std::io::Write;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use data_encoding::BASE64;
use http_body_util::{BodyExt, Full};
use hyper::body::{Buf, Bytes, Incoming};
use hyper::header::CONTENT_TYPE;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, Method, StatusCode};
use hyper_util::rt::TokioIo;
use hyper_util::server::graceful::GracefulShutdown;
use num_bigint::ToBigUint;
use serde_derive::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tracing::{warn, info};
use uuid::Uuid;

use crate::auth_srv::auth_hash::create_pass_hash;
use crate::config::ServerConfig;
use crate::net_crypt::{CRYPT_BASE_AUTH, CRYPT_BASE_GAME, CRYPT_BASE_GATE_KEEPER};
use crate::netcli::{NetResult, NetResultCode};
use crate::vault::{AccountInfo, VaultServer, VaultPlayerInfoNode};

struct ApiInterface {
    server_config: Arc<ServerConfig>,
    shutdown_send: broadcast::Sender<()>,
    vault: Arc<VaultServer>,
}

impl ApiInterface {
    // Returns the account info of the account associated with an API token
    async fn check_api_token(&self, api_token: Option<&str>, admin_required: bool)
        -> Option<AccountInfo>
    {
        if let Some(token) = api_token
            && let Ok(Some(account)) = self.vault.get_account_for_token(token).await
            && (!admin_required || account.is_admin())
        {
            return Some(account);
        }
        None
    }

    async fn query_online_players(&self) -> NetResult<Vec<OnlinePlayer>> {
        let template = VaultPlayerInfoNode::new_lookup(Some(1));
        let player_list = self.vault.find_nodes(template).await?;
        let mut players = Vec::with_capacity(player_list.len());
        for player_info in player_list {
            let node = self.vault.fetch_node(player_info).await?.as_player_info_node().unwrap();
            players.push(OnlinePlayer {
                name: node.player_name_ci().to_string(),
                location: node.age_instance_name().to_string(),
            });
        }
        Ok(players)
    }

    async fn create_account(&self, params: AccountParams, api_token: Option<&str>)
        -> NetResult<AccountInfo>
    {
        let (Some(username), Some(password)) = (&params.username, &params.password) else {
            warn!("Missing required parameter(s)");
            return Err(NetResultCode::NetInvalidParameter);
        };
        let account_flags = if params.account_flags.is_none() {
            0
        } else if self.check_api_token(api_token, true).await.is_some() {
            params.parse_account_flags()?.0
        } else {
            return Err(NetResultCode::NetAuthenticationFailed);
        };
        let pass_hash = match create_pass_hash(username, password) {
            Ok(hash) => hash,
            Err(err) => {
                warn!("Failed to create password hash: {err}");
                return Err(NetResultCode::NetInternalError);
            }
        };
        self.vault.create_account(username, &pass_hash, account_flags).await
    }
}

fn gen_unauthorized() -> Response<Full<Bytes>> {
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(CONTENT_TYPE, "application/json")
        .body(Full::from(Bytes::from_static(br#"{"error":"Unauthorized"}"#)))
        .unwrap()
}

fn gen_server_error() -> Response<Full<Bytes>> {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .header(CONTENT_TYPE, "application/json")
        .body(Full::from(Bytes::from_static(br#"{"error":"Internal Server Error"}"#)))
        .unwrap()
}

fn gen_bad_request() -> Response<Full<Bytes>> {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .header(CONTENT_TYPE, "application/json")
        .body(Full::from(Bytes::from_static(br#"{"error":"Bad Request"}"#)))
        .unwrap()
}

fn gen_json_response<T>(value: &T) -> Response<Full<Bytes>>
    where T: ?Sized + serde::Serialize
{
    match serde_json::to_string(value) {
        Ok(json) => Response::builder()
            .header(CONTENT_TYPE, "application/json")
            .body(Full::from(json))
            .unwrap(),
        Err(err) => {
            warn!("Failed to generate JSON: {err}");
            gen_server_error()
        }
    }
}

async fn parse_json_request<T>(request: Request<Incoming>) -> anyhow::Result<T>
    where T: serde::de::DeserializeOwned
{
    let body = request.collect().await?.aggregate();
    Ok(serde_json::from_reader(body.reader())?)
}

const HELP_MSG: Bytes = Bytes::from_static(include_bytes!("api_help.txt"));

async fn api_router(request: Request<Incoming>, api: Arc<ApiInterface>)
        -> Result<Response<Full<Bytes>>, Infallible>
{
    let query_params = if let Some(query) = request.uri().query() {
        form_urlencoded::parse(query.as_bytes()).into_owned()
                .collect::<HashMap<String, String>>()
    } else {
        HashMap::new()
    };
    let api_token = request.headers().get("Authorization")
                           .and_then(|auth| auth.to_str().ok())
                           .and_then(|auth| auth.strip_prefix("Bearer "))
                           .or_else(|| query_params.get("token").map(String::as_str))
                           .map(str::to_owned);

    let response = match (request.method(), request.uri().path()) {
        (&Method::GET, "/") => {
            // Show static API help text
            Response::builder().body(Full::from(HELP_MSG)).unwrap()
        }
        (&Method::GET, "/account/api_tokens") => {
            if let Some(account) = api.check_api_token(api_token.as_deref(), false).await {
                let Ok(account_id) =
                    query_params.get("account")
                                .map_or(Ok(account.account_id), |param| Uuid::from_str(param))
                else {
                    return Ok(gen_bad_request());
                };
                if !account.is_admin() && account_id != account.account_id {
                    gen_unauthorized()
                } else {
                    match api.vault.get_api_tokens(&account_id).await {
                        Ok(tokens) => gen_json_response(&tokens),
                        Err(err) => {
                            warn!("Failed to query API tokens: {err:?}");
                            gen_server_error()
                        }
                    }
                }
            } else {
                gen_unauthorized()
            }
        }
        (&Method::POST, "/account/new") => {
            let params: AccountParams = match parse_json_request(request).await {
                Ok(params) => params,
                Err(err) => {
                    warn!("Failed parsing JSON request: {err}");
                    return Ok(gen_bad_request());
                }
            };
            match api.create_account(params, api_token.as_deref()).await {
                Ok(account) => {
                    Response::builder()
                        .header(CONTENT_TYPE, "application/json")
                        .body(Full::from(Bytes::from(
                            format!(r#"{{"status":"ok","account_id":"{}"}}"#, account.account_id))))
                        .unwrap()
                }
                Err(NetResultCode::NetAccountAlreadyExists) => {
                    Response::builder()
                        .status(StatusCode::CONFLICT)
                        .header(CONTENT_TYPE, "application/json")
                        .body(Full::from(Bytes::from_static(br#"{"error":"Account already exists"}"#)))
                        .unwrap()
                }
                Err(NetResultCode::NetInvalidParameter) => gen_bad_request(),
                Err(NetResultCode::NetAuthenticationFailed) => gen_unauthorized(),
                Err(_) => gen_server_error(),
            }
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
                let _ = writeln!(lines, "Server.{stype}.N \"{}\"", BASE64.encode(&bytes_n));
                let _ = writeln!(lines, "Server.{stype}.X \"{}\"", BASE64.encode(&bytes_x));
            }
            Response::builder().body(Full::from(lines)).unwrap()
        }
        (&Method::GET, "/online") => {
            // Return JSON object containing the names and locations of online players
            match api.query_online_players().await {
                Ok(players) => gen_json_response(&players),
                Err(err) => {
                    warn!("Failed to query online players: {err:?}");
                    gen_server_error()
                }
            }
        }
        (&Method::POST, "/shutdown") => {
            if let Some(admin) = api.check_api_token(api_token.as_deref(), true).await {
                info!("Shutdown requested by {}", admin.account_name);
                let _ = api.shutdown_send.send(());
                Response::builder()
                    .header(CONTENT_TYPE, "application/json")
                    .body(Full::from(Bytes::from_static(br#"{"status":"ok"}"#)))
                    .unwrap()
            } else {
                gen_unauthorized()
            }
        }
        (&Method::GET, "/status") => {
            // Basic status check
            // TODO: Check health of other services and report them here...
            Response::builder().body(Full::from(Bytes::from_static(b"OK"))).unwrap()
        }
        _ => {
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header(CONTENT_TYPE, "application/json")
                .body(Full::from(Bytes::from_static(br#"{"error":"Invalid API Request"}"#)))
                .unwrap()
        }
    };
    Ok(response)
}

pub fn start_api(shutdown_send: broadcast::Sender<()>, vault: Arc<VaultServer>,
                 server_config: Arc<ServerConfig>)
{
    tokio::spawn(async move {
        let mut shutdown_recv = shutdown_send.subscribe();
        let api = Arc::new(ApiInterface {
            server_config,
            shutdown_send,
            vault,
        });

        let listener = match TcpListener::bind(&api.server_config.api_address).await {
            Ok(listener) => listener,
            Err(err) => {
                warn!("Failed to bind API service: {err}");
                return;
            }
        };

        info!("Starting API service on http://{}", api.server_config.api_address);
        let server = http1::Builder::new();
        let graceful = GracefulShutdown::new();

        loop {
            tokio::select! {
                client = listener.accept() => {
                    let (stream, _remote_addr) = match client {
                        Ok(accepted) => accepted,
                        Err(err) => {
                            warn!("Failed to accept API connection: {err}");
                            continue;
                        }
                    };

                    let io = TokioIo::new(stream);
                    let conn = {
                        let api = api.clone();
                        server.serve_connection(io, service_fn(move |request| {
                            api_router(request, api.clone())
                        }))
                    };

                    let graceful_fut = graceful.watch(conn);
                    tokio::spawn(async move {
                        if let Err(err) = graceful_fut.await {
                            warn!("API service error: {err}");
                        }
                    });
                }

                _ = shutdown_recv.recv() => {
                    drop(listener);
                    break;
                }
            }
        }

        info!("Shutting down API service");
        tokio::select! {
            () = graceful.shutdown() => (),
            () = tokio::time::sleep(Duration::from_secs(10)) => {
                warn!("API service did not shut down gracefully after 10 seconds; terminating service.");
            }
        }
    });
}

#[derive(Serialize)]
struct OnlinePlayer {
    name: String,
    location: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountParams {
    username: Option<String>,
    password: Option<String>,
    account_flags: Option<Vec<String>>,
}

impl AccountParams {
    fn parse_account_flags(&self) -> NetResult<(u32, u32)> {
        let mut add_flags = 0;
        let mut remove_flags = 0;
        for flag_name in self.account_flags.as_deref().unwrap_or(&[]) {
            match flag_name.as_str() {
                "admin" => add_flags |= AccountInfo::ADMIN,
                "!admin" => remove_flags |= AccountInfo::ADMIN,
                "banned" => add_flags |= AccountInfo::BANNED,
                "!banned" => remove_flags |= AccountInfo::BANNED,
                "beta" => add_flags |= AccountInfo::BETA_TESTER,
                "!beta" => remove_flags |= AccountInfo::BETA_TESTER,
                _ => return Err(NetResultCode::NetInvalidParameter),
            }
        }
        Ok((add_flags, remove_flags))
    }
}
