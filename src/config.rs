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

use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{anyhow, Context, Result};
use data_encoding::BASE64;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use crypto_bigint::U512;
use rand::TryRngCore;
use serde_derive::Deserialize;

pub struct ServerConfig {
    /* Listen address for the lobby server */
    pub listen_address: String,

    /* Listen address for the API service */
    pub api_address: String,

    /* URI prefix for API endpoints */
    pub api_prefix: String,

    /* Product configuration */
    pub build_id: u32,

    /* Rc4 Encryption keys */
    pub auth_n_key: U512,
    pub auth_k_key: U512,
    pub game_n_key: U512,
    pub game_k_key: U512,
    pub gate_n_key: U512,
    pub gate_k_key: U512,

    /* GateKeeper server addresses */
    pub file_serv_ip: String,
    pub auth_serv_ip: String,
    pub game_serv_ip: String,

    /* File server data path */
    pub data_root: PathBuf,

    /* Vault backend */
    pub db_url: String,

    /* Restrict logins to just Admins + Beta Testers */
    pub restrict_logins: bool,
}

fn decode_crypt_key(value: &str) -> Result<U512> {
    let bytes = BASE64.decode(value.as_bytes())
            .with_context(|| format!("Could not parse Base64 key '{value}'"))?;
    if bytes.len() == U512::BYTES {
        Ok(U512::from_be_slice(&bytes))
    } else {
        Err(anyhow!("Invalid key length for key '{value}'"))
    }
}

impl ServerConfig {
    pub fn from_file(path: &Path) -> Result<ServerConfig> {
        #![allow(clippy::similar_names)]

        let config_file = std::fs::read_to_string(path)?;
        let config: StructuredConfig = toml::from_str(&config_file)
                .context("Failed to parse config file")?;

        let server_section = config.server.unwrap_or_default();

        // The default is to listen on 127.0.0.1, which means that ONLY
        // connections from localhost are allowed.  To listen on any IPv4
        // address, you should set listen_address = "0.0.0.0"
        let listen_address = format!("{}:{}",
                server_section.listen_address.as_deref().unwrap_or("127.0.0.1"),
                server_section.listen_port.unwrap_or(14617));
        let build_id = config.build_id.unwrap_or(918);
        let data_root =
            if let Some(data_root) = config.data_root {
                PathBuf::from(data_root)
            } else {
                std::env::current_dir()
                    .context("Failed to determine current working directory")?
                    .join("data")
            };

        let auth_n_key = decode_crypt_key(&config.crypt_keys.auth.n)?;
        let auth_k_key = decode_crypt_key(&config.crypt_keys.auth.k)?;
        let game_n_key = decode_crypt_key(&config.crypt_keys.game.n)?;
        let game_k_key = decode_crypt_key(&config.crypt_keys.game.k)?;
        let gate_n_key = decode_crypt_key(&config.crypt_keys.gate.n)?;
        let gate_k_key = decode_crypt_key(&config.crypt_keys.gate.k)?;

        // Again, the defaults are only useful when connecting from localhost.
        // These should be configured to an EXTERNAL IP address, since they
        // are the addresses sent to the client for establishing additional
        // connections to this (or another) server.
        let file_serv_ip = server_section.file_server_ip.as_deref()
                                .unwrap_or("127.0.0.1").to_string();
        let auth_serv_ip = server_section.auth_server_ip.as_deref()
                                .unwrap_or("127.0.0.1").to_string();
        let game_serv_ip = server_section.game_server_ip.as_deref()
                                .unwrap_or("127.0.0.1").to_string();

        let api_section = config.api.unwrap_or_default();

        let api_address = format!("{}:{}",
                api_section.address.as_deref().unwrap_or("127.0.0.1"),
                api_section.port.unwrap_or(14615));

        let mut api_prefix = api_section.uri_prefix.unwrap_or_default();

        // Normalize the prefix to the form "/prefix", or empty for no prefix.
        if !api_prefix.is_empty() && !api_prefix.starts_with('/') {
            api_prefix.insert(0, '/');
        }
        if api_prefix.ends_with('/') {
            api_prefix.pop();
        }

        let db_url = config.vault_db.db_url;

        let restrict_logins = config.restrict_logins.unwrap_or(false);

        Ok(ServerConfig {
            listen_address,
            api_address,
            api_prefix,
            build_id,
            auth_n_key,
            auth_k_key,
            game_n_key,
            game_k_key,
            gate_n_key,
            gate_k_key,
            file_serv_ip,
            auth_serv_ip,
            game_serv_ip,
            data_root,
            db_url,
            restrict_logins,
        })
    }

    pub fn get_ntd_key(&self) -> Result<[u32; 4]> {
        load_or_create_ntd_key(&self.data_root)
    }
}

#[derive(Deserialize)]
struct StructuredConfig {
    data_root: Option<String>,
    build_id: Option<u32>,
    restrict_logins: Option<bool>,
    server: Option<ServerAddrConfig>,
    api: Option<ApiConfig>,
    crypt_keys: ConfigKeys,
    vault_db: VaultDbConfig,
}

#[derive(Deserialize, Default)]
struct ServerAddrConfig {
    listen_address: Option<String>,
    listen_port: Option<u16>,
    file_server_ip: Option<String>,
    auth_server_ip: Option<String>,
    game_server_ip: Option<String>,
}

#[derive(Deserialize, Default)]
struct ApiConfig {
    address: Option<String>,
    port: Option<u16>,
    uri_prefix: Option<String>,
}

#[derive(Deserialize)]
struct ConfigKeys {
    auth: ConfigKeyPair,
    game: ConfigKeyPair,
    gate: ConfigKeyPair,
}

#[derive(Deserialize)]
struct ConfigKeyPair {
    n: String,
    k: String,
}

#[derive(Deserialize, Default)]
struct VaultDbConfig {
    db_url: String,
}

// NOTE: This file stores the keys in Big Endian format for easier debugging
// with tools like PlasmaShop
pub fn load_or_create_ntd_key(data_root: &Path) -> Result<[u32; 4]> {
    static NTD_KEY: OnceLock<[u32; 4]> = OnceLock::new();
    if let Some(key) = NTD_KEY.get() {
        return Ok(*key);
    }

    let key_path = data_root.join(".ntd_server.key");
    let mut key_buffer = [0; 4];
    let key = match File::open(&key_path) {
        Ok(file) => {
            let mut stream = BufReader::new(file);
            stream.read_u32_into::<BigEndian>(&mut key_buffer)?;
            key_buffer
        }
        Err(err) => {
            if err.kind() == io::ErrorKind::NotFound {
                let mut stream = BufWriter::new(File::create(&key_path)?);
                for v in &mut key_buffer {
                    *v = rand::rngs::OsRng.try_next_u32()?;
                    stream.write_u32::<BigEndian>(*v)?;
                }
                key_buffer
            } else {
                return Err(err.into())
            }
        }
    };

    NTD_KEY.set(key).expect("Tried to set NTD key twice");
    Ok(key)
}
