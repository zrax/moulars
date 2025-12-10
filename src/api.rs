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

use crypto_bigint::U512;
use data_encoding::BASE64;
use http_body_util::{BodyExt, Full};
use hyper::body::{Buf, Bytes, Incoming};
use hyper::header::CONTENT_TYPE;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, Method, StatusCode};
use hyper_util::rt::TokioIo;
use hyper_util::server::graceful::GracefulShutdown;
use serde_derive::{Deserialize, Serialize};
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tracing::{warn, info};
use uuid::Uuid;

use crate::auth_srv::auth_hash::create_pass_hash;
use crate::config::ServerConfig;
use crate::net_crypt::{
    CRYPT_BASE_AUTH, CRYPT_BASE_GAME, CRYPT_BASE_GATE_KEEPER, u512_pow_mod
};
use crate::netcli::{NetResult, NetResultCode};
use crate::vault::{AccountInfo, VaultServer, VaultPlayerInfoNode};

struct ApiInterface {
    server_config: Arc<ServerConfig>,
    shutdown_send: broadcast::Sender<()>,
    vault: Arc<VaultServer>,
}

impl ApiInterface {
    // Returns the account info of the account associated with an API token
    async fn check_api_token(&self, api_token: Option<&str>) -> Option<AccountInfo> {
        if let Some(token) = api_token
            && let Ok(Some(account)) = self.vault.get_account_for_token(token).await
            && !account.is_banned()
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
            let node = self.vault.fetch_node(player_info).await?.as_player_info_node()
                                 .expect("Returned node is not a player info node");
            players.push(OnlinePlayer {
                name: node.player_name_ci().to_string(),
                location: node.age_instance_name().to_string(),
            });
        }
        Ok(players)
    }

    async fn get_account_info(&self, account_id: Option<&str>, requestor: &AccountInfo)
        -> NetResult<AccountInfo>
    {
        let account_id = get_authorized_account(account_id, requestor)?;
        let Some(account) = self.vault.get_account_by_id(&account_id).await? else {
            return Err(NetResultCode::NetAccountNotFound);
        };
        Ok(account)
    }

    async fn create_account(&self, params: AccountParams, api_token: Option<&str>)
        -> NetResult<AccountInfo>
    {
        let Some(requestor) = self.check_api_token(api_token).await else {
            return Err(NetResultCode::NetAuthenticationFailed);
        };
        let (Some(username), Some(password)) = (&params.username, &params.password) else {
            warn!("Missing required parameter(s)");
            return Err(NetResultCode::NetInvalidParameter);
        };
        let account_flags = if params.account_flags.is_none() {
            0
        } else if requestor.is_admin() {
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
        info!("{} requested new account '{username}' with flags: {:?}",
              requestor.account_name, params.account_flags);
        self.vault.create_account(username, &pass_hash, account_flags).await
    }

    async fn update_account(&self, account_id: Option<&str>, params: AccountParams,
                            api_token: Option<&str>) -> NetResult<()>
    {
        let Some(requestor) = self.check_api_token(api_token).await else {
            return Err(NetResultCode::NetAuthenticationFailed);
        };
        let account = self.get_account_info(account_id, &requestor).await?;
        if params.username.is_some() {
            // Disallow updating usernames
            return Err(NetResultCode::NetInvalidParameter);
        }
        let account_flags = if params.account_flags.is_none() {
            None
        } else if requestor.is_admin() {
            let update_flags = params.parse_account_flags()?;
            Some((account.account_flags | update_flags.0) & !update_flags.1)
        } else {
            return Err(NetResultCode::NetAuthenticationFailed);
        };
        let pass_hash = if let Some(password) = params.password {
            match create_pass_hash(&account.account_name, &password) {
                Ok(hash) => Some(hash),
                Err(err) => {
                    warn!("Failed to create password hash: {err}");
                    return Err(NetResultCode::NetInternalError);
                }
            }
        } else {
            None
        };

        info!("{} requested account '{}' update {}flags: {:?}",
              requestor.account_name, account.account_name,
              if pass_hash.is_some() { "password and " } else { "" },
              params.account_flags);
        self.vault.update_account(&account.account_id, pass_hash.as_ref(), account_flags).await
    }
}

fn get_authorized_account(account_id: Option<&str>, requestor: &AccountInfo) -> NetResult<Uuid> {
    let Ok(account_id) = account_id.map_or(Ok(requestor.account_id), Uuid::from_str) else {
        return Err(NetResultCode::NetInvalidParameter);
    };
    if requestor.is_admin() || account_id == requestor.account_id {
        Ok(account_id)
    } else {
        Err(NetResultCode::NetAuthenticationFailed)
    }
}

fn gen_plaintext_response<T>(text: T) -> NetResult<Response<Full<Bytes>>>
    where T: Into<Full<Bytes>>
{
    Response::builder()
        .body(text.into())
        .map_err(|err| {
            warn!("Failed to build plaintext response: {err}");
            NetResultCode::NetInternalError
        })
}

fn gen_json_response<T>(value: T) -> NetResult<Response<Full<Bytes>>>
    where T: serde::Serialize
{
    let json = serde_json::to_string(&value)
        .map_err(|err| {
            warn!("Failed to generate JSON: {err}");
            NetResultCode::NetInternalError
        })?;
    Response::builder()
        .header(CONTENT_TYPE, "application/json")
        .body(json.into())
        .map_err(|err| {
            warn!("Failed to build JSON response: {err}");
            NetResultCode::NetInternalError
        })
}

async fn parse_json_request<T>(request: Request<Incoming>) -> NetResult<T>
    where T: serde::de::DeserializeOwned
{
    let body = request.collect().await
        .map_err(|err| {
            warn!("Failed to read request body: {err}");
            NetResultCode::NetInvalidParameter
        })?
        .aggregate();
    serde_json::from_reader(body.reader())
        .map_err(|err| {
            warn!("Failed to parse JSON request: {err}");
            NetResultCode::NetInvalidParameter
        })
}

const HELP_MSG: Bytes = Bytes::from_static(include_bytes!("api_help.txt"));

async fn api_router(request: Request<Incoming>, api: Arc<ApiInterface>)
        -> NetResult<Response<Full<Bytes>>>
{
    let request_uri = if api.server_config.api_prefix.is_empty() {
        request.uri().path()
    } else {
        let full_uri = request.uri().path();
        if full_uri == api.server_config.api_prefix {
            "/"
        } else if let Some(uri_suffix) = full_uri.strip_prefix(&api.server_config.api_prefix) {
            uri_suffix
        } else {
            return Err(NetResultCode::NetFileNotFound);
        }
    };

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

    match (request.method(), request_uri) {
        (&Method::GET, "/") => {
            // Show static API help text
            gen_plaintext_response(HELP_MSG)
        }
        (&Method::GET, "/account") => {
            let Some(requestor) = api.check_api_token(api_token.as_deref()).await else {
                return Err(NetResultCode::NetAuthenticationFailed);
            };
            let account_id = query_params.get("account").map(String::as_str);
            let account_info = api.get_account_info(account_id, &requestor).await?;
            gen_json_response(AccountInfoJson::from(account_info))
        }
        (&Method::GET, "/account/api_tokens") => {
            let Some(requestor) = api.check_api_token(api_token.as_deref()).await else {
                return Err(NetResultCode::NetAuthenticationFailed);
            };
            let account_id = query_params.get("account").map(String::as_str);
            let auth_id = get_authorized_account(account_id, &requestor)?;
            gen_json_response(api.vault.get_api_tokens(&auth_id).await?)
        }
        (&Method::POST, "/account/new") => {
            let params: AccountParams = parse_json_request(request).await?;
            let account = api.create_account(params, api_token.as_deref()).await?;
            gen_json_response(json!({
                "status": "ok",
                "account_id": account.account_id,
            }))
        }
        (&Method::POST, "/account/update") => {
            let params: AccountParams = parse_json_request(request).await?;
            let account_id = query_params.get("account").map(String::as_str);
            api.update_account(account_id, params, api_token.as_deref()).await?;
            gen_json_response(json!({"status": "ok"}))
        }
        (&Method::GET, "/client_keys") => {
            let mut lines = Vec::with_capacity(6 * 105);
            for (stype, key_g, key_k, key_n) in [
                ("Auth", CRYPT_BASE_AUTH, &api.server_config.auth_k_key, &api.server_config.auth_n_key),
                ("Game", CRYPT_BASE_GAME, &api.server_config.game_k_key, &api.server_config.game_n_key),
                ("Gate", CRYPT_BASE_GATE_KEEPER, &api.server_config.gate_k_key, &api.server_config.gate_n_key)]
            {
                let key_x = u512_pow_mod(&U512::from(key_g), key_k, key_n);
                let bytes_n = key_n.to_be_bytes();
                let bytes_x = key_x.to_be_bytes();
                let _ = writeln!(lines, "Server.{stype}.N \"{}\"", BASE64.encode(&bytes_n));
                let _ = writeln!(lines, "Server.{stype}.X \"{}\"", BASE64.encode(&bytes_x));
            }
            gen_plaintext_response(lines)
        }
        (&Method::GET, "/initialize") => {
            let (account_info, api_token) = api.vault.initialize().await?;
            gen_json_response(json!({
                "username": account_info.account_name,
                "account_id": account_info.account_id,
                "api_token": api_token.token,
            }))
        }
        (&Method::GET, "/online") => {
            // Return JSON object containing the names and locations of online players
            let players = api.query_online_players().await?;
            gen_json_response(&players)
        }
        (&Method::POST, "/shutdown") => {
            if let Some(requestor) = api.check_api_token(api_token.as_deref()).await
                && requestor.is_admin()
            {
                info!("Shutdown requested by {}", requestor.account_name);
                let _ = api.shutdown_send.send(());
                gen_json_response(json!({"status": "ok"}))
            } else {
                Err(NetResultCode::NetAuthenticationFailed)
            }
        }
        (&Method::GET, "/status") => {
            // Basic status check
            // TODO: Check health of other services and report them here...
            gen_plaintext_response("OK")
        }
        _ => Err(NetResultCode::NetFileNotFound),
    }
}

fn gen_server_error() -> Response<Full<Bytes>> {
    // Pre-cache the error response body for 500 errors in case something's broken
    // even with the error response handler.
    const RESPONSE_JSON: Bytes = Bytes::from_static(br#"{"error":"Internal Server Error"}"#);
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .header(CONTENT_TYPE, "application/json")
        .body(Full::from(RESPONSE_JSON))
        .expect("Failed to build 500 response")
}

async fn api_wrapper(request: Request<Incoming>, api: Arc<ApiInterface>)
        -> Result<Response<Full<Bytes>>, Infallible>
{
    match api_router(request, api).await {
        Ok(response) => Ok(response),
        Err(code) => {
            let (status, message) = match code {
                NetResultCode::NetInternalError => return Ok(gen_server_error()),
                NetResultCode::NetInvalidParameter => (StatusCode::BAD_REQUEST, "Bad request"),
                NetResultCode::NetAccountAlreadyExists => (StatusCode::CONFLICT, "Account already exists"),
                NetResultCode::NetAccountNotFound => (StatusCode::NOT_FOUND, "Account not found"),
                NetResultCode::NetAuthenticationFailed => (StatusCode::UNAUTHORIZED, "Unauthorized"),
                NetResultCode::NetServiceForbidden => (StatusCode::FORBIDDEN, "Forbidden"),
                NetResultCode::NetFileNotFound => (StatusCode::NOT_FOUND, "Invalid API request"),
                _ => {
                    warn!("Unhandled NetResultCode: {code:?}");
                    return Ok(gen_server_error());
                }
            };
            let json_body = match serde_json::to_string(&json!({"error": message})) {
                Ok(json) => json,
                Err(err) => {
                    warn!("Failed to generate JSON: {err}");
                    return Ok(gen_server_error());
                }
            };
            Ok(Response::builder()
                .status(status)
                .header(CONTENT_TYPE, "application/json")
                .body(Full::from(json_body))
                .unwrap_or_else(|err| {
                    warn!("Failed to build error response: {err}");
                    gen_server_error()
                }))
        }
    }
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
                            api_wrapper(request, api.clone())
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

#[derive(Serialize)]
struct AccountInfoJson {
    username: String,
    account_id: Uuid,
    account_flags: Vec<String>,
}

impl From<AccountInfo> for AccountInfoJson {
    fn from(account: AccountInfo) -> Self {
        let mut account_flags = Vec::new();
        if account.is_admin() { account_flags.push("admin".to_string()); }
        if account.is_banned() { account_flags.push("banned".to_string()); }
        if account.is_beta_tester() { account_flags.push("beta".to_string()); }

        Self {
            username: account.account_name,
            account_id: account.account_id,
            account_flags,
        }
    }
}
