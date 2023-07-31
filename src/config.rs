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

use std::io::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use log::error;
use num_bigint::BigUint;
use serde_derive::Deserialize;

use crate::general_error;

pub enum VaultDbBackend {
    None,
    Sqlite,
    Postgres,
}

pub struct ServerConfig {
    /* Listen address for the lobby server */
    pub listen_address: String,

    /* Product configuration */
    pub build_id: u32,

    /* Rc4 Encryption keys */
    pub auth_n_key: BigUint,
    pub auth_k_key: BigUint,
    pub game_n_key: BigUint,
    pub game_k_key: BigUint,
    pub gate_n_key: BigUint,
    pub gate_k_key: BigUint,

    /* GateKeeper server addresses */
    pub file_serv_ip: String,
    pub auth_serv_ip: String,

    /* File server data path */
    pub data_root: PathBuf,

    /* Vault backend */
    pub db_type: VaultDbBackend,
}

fn decode_crypt_key(value: &str) -> Result<BigUint> {
    let bytes = base64::decode(value)
            .map_err(|err| general_error!("Could not parse Base64 key '{}': {}",
                                          value, err))?;
    if bytes.len() == 64 {
        Ok(BigUint::from_bytes_be(&bytes))
    } else {
        Err(general_error!("Invalid key length for key '{}'", value))
    }
}

impl ServerConfig {
    pub fn from_file(path: &Path) -> Result<Arc<ServerConfig>> {
        let config_file = std::fs::read_to_string(path)?;
        let config: StructuredConfig = toml::from_str(&config_file)
                .map_err(|err| general_error!("Failed to parse config file: {}", err))?;

        let server_section = config.server.unwrap_or_default();

        // The default is to listen on 127.0.0.1, which means that ONLY
        // connections from localhost are allowed.  To listen on any IPv4
        // address, you should set listen_address = "0.0.0.0"
        let listen_address = format!("{}:{}",
                server_section.listen_address.unwrap_or("127.0.0.1".to_string()),
                server_section.listen_port.unwrap_or(14617));
        let build_id = config.build_id.unwrap_or(918);
        let data_root =
            if let Some(data_root) = config.data_root {
                PathBuf::from(data_root)
            } else {
                std::env::current_dir()
                    .map_err(|err| general_error!("Failed to determine current working directory: {}", err))?
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
        let file_serv_ip = server_section.file_server_ip.unwrap_or("127.0.0.1".to_string());
        let auth_serv_ip = server_section.auth_server_ip.unwrap_or("127.0.0.1".to_string());

        let vault_db_section = config.vault_db.unwrap_or_default();
        let db_type = if let Some(type_str) = vault_db_section.db_type {
            match type_str.as_str() {
                "none" => VaultDbBackend::None,
                "sqlite" => VaultDbBackend::Sqlite,
                "postgres" => VaultDbBackend::Postgres,
                _ => return Err(general_error!("Unknown database type: {}", type_str))
            }
        } else {
            VaultDbBackend::None
        };

        Ok(Arc::new(ServerConfig {
            listen_address,
            build_id,
            auth_n_key,
            auth_k_key,
            game_n_key,
            game_k_key,
            gate_n_key,
            gate_k_key,
            file_serv_ip,
            auth_serv_ip,
            data_root,
            db_type,
        }))
    }

    pub fn dummy_config() -> Arc<ServerConfig> {
        // A configuration used for testing.  Please don't use these values
        // (especially the keys) in production, as they are not secure.

        /* Generated keys from dirtsand (Big Endian):
        [Server Keys - unpacked into bytes below]
        auth.n = 1EQWzco8zsVBOqS8nXraPmq3g7CcJXnj/dQJI/n9wh5LEBjsO0yRAfzZGUgwhcChs9JY0Y7Cq7EGhurqpGFlBw==
        auth.k = 12VYlr12lSeWQiKRlBoKmg//K+7Yinfi2LnKDtpDgXoMxtdG1VIAbJqC+daPcBrumR5f1pekUXY1/R5TyBVyFw==
        game.n = yv0uMQNU49083++/v8Z2H9/7XM+LnoO1cvYgyfGObKvuXXURrdcA9vqsa3P+XyGw9UoyUMjiNvJS+LYI4z5Msw==
        game.k = zbFlBUeE9/gTkIM46RWLKkOP8uCEZiJhgmTb0eruFetcBGnLKeeJMWrkbfuKcSPBpAgqLgsdHumoAakxdBp22w==
        gate.n = 0r0DRrkYkbpGAhByXgtWHAnJ041fxxzPQGewiUZYfjTtQ4B2byPs1UA6ofD+8/POn6s83dTvk7tW/gvqJASuiw==
        gate.k = 6f+PNqyQ3V9nqzU2WybULjrliez0kyikITMAG4O24LvAtvLpk1PHcddgtrePuowhE+3wwt7p4BgZvXFv4ooiXw==

        [Client Config - for use in plClient's server.ini]
        Server.Auth.N "1EQWzco8zsVBOqS8nXraPmq3g7CcJXnj/dQJI/n9wh5LEBjsO0yRAfzZGUgwhcChs9JY0Y7Cq7EGhurqpGFlBw=="
        Server.Auth.X "nzmFeJW3w3YgYsqMy1cHM46zyRQnW4UQZ83q8u24gLYGXUzoiJf54oQNJt+N2EgJumoj8aEkW45b9zLs9V8ajQ=="
        Server.Game.N "yv0uMQNU49083++/v8Z2H9/7XM+LnoO1cvYgyfGObKvuXXURrdcA9vqsa3P+XyGw9UoyUMjiNvJS+LYI4z5Msw=="
        Server.Game.X "lL2P9YonL67VtgUpDgzkVVnEZXii6mZL/hBLCim8p+21kJ+vaiNUfxXdmXNKpcF4C4CqFzmnBSHTSKsFi1xxbQ=="
        Server.Gate.N "0r0DRrkYkbpGAhByXgtWHAnJ041fxxzPQGewiUZYfjTtQ4B2byPs1UA6ofD+8/POn6s83dTvk7tW/gvqJASuiw=="
        Server.Gate.X "JXzVmCNwL4D6WTT01JR4gb8drehMCkSvVtRFRg4+63TN91Mf5pa7ktoo/rNohriUjqgm4/5GvsyARe24uPvs6w=="
        Server.Gate.Host "127.0.0.1"
        Server.Auth.Host "127.0.0.1"
        Server.DispName "Localhost"
        */

        let auth_n: [u8; 64] = [212, 68, 22, 205, 202, 60, 206, 197, 65, 58, 164, 188, 157, 122, 218, 62, 106, 183, 131, 176, 156, 37, 121, 227, 253, 212, 9, 35, 249, 253, 194, 30, 75, 16, 24, 236, 59, 76, 145, 1, 252, 217, 25, 72, 48, 133, 192, 161, 179, 210, 88, 209, 142, 194, 171, 177, 6, 134, 234, 234, 164, 97, 101, 7];
        let auth_k: [u8; 64] = [215, 101, 88, 150, 189, 118, 149, 39, 150, 66, 34, 145, 148, 26, 10, 154, 15, 255, 43, 238, 216, 138, 119, 226, 216, 185, 202, 14, 218, 67, 129, 122, 12, 198, 215, 70, 213, 82, 0, 108, 154, 130, 249, 214, 143, 112, 26, 238, 153, 30, 95, 214, 151, 164, 81, 118, 53, 253, 30, 83, 200, 21, 114, 23];
        let game_n: [u8; 64] = [202, 253, 46, 49, 3, 84, 227, 221, 60, 223, 239, 191, 191, 198, 118, 31, 223, 251, 92, 207, 139, 158, 131, 181, 114, 246, 32, 201, 241, 142, 108, 171, 238, 93, 117, 17, 173, 215, 0, 246, 250, 172, 107, 115, 254, 95, 33, 176, 245, 74, 50, 80, 200, 226, 54, 242, 82, 248, 182, 8, 227, 62, 76, 179];
        let game_k: [u8; 64] = [205, 177, 101, 5, 71, 132, 247, 248, 19, 144, 131, 56, 233, 21, 139, 42, 67, 143, 242, 224, 132, 102, 34, 97, 130, 100, 219, 209, 234, 238, 21, 235, 92, 4, 105, 203, 41, 231, 137, 49, 106, 228, 109, 251, 138, 113, 35, 193, 164, 8, 42, 46, 11, 29, 30, 233, 168, 1, 169, 49, 116, 26, 118, 219];
        let gate_n: [u8; 64] = [210, 189, 3, 70, 185, 24, 145, 186, 70, 2, 16, 114, 94, 11, 86, 28, 9, 201, 211, 141, 95, 199, 28, 207, 64, 103, 176, 137, 70, 88, 126, 52, 237, 67, 128, 118, 111, 35, 236, 213, 64, 58, 161, 240, 254, 243, 243, 206, 159, 171, 60, 221, 212, 239, 147, 187, 86, 254, 11, 234, 36, 4, 174, 139];
        let gate_k: [u8; 64] = [233, 255, 143, 54, 172, 144, 221, 95, 103, 171, 53, 54, 91, 38, 212, 46, 58, 229, 137, 236, 244, 147, 40, 164, 33, 51, 0, 27, 131, 182, 224, 187, 192, 182, 242, 233, 147, 83, 199, 113, 215, 96, 182, 183, 143, 186, 140, 33, 19, 237, 240, 194, 222, 233, 224, 24, 25, 189, 113, 111, 226, 138, 34, 95];

        let cwd = match std::env::current_dir() {
            Ok(path) => path,
            Err(err) => {
                error!("Failed to determine current working directory: {}", err);
                std::process::exit(1);
            }
        };

        Arc::new(ServerConfig {
            // Warning: Never listen on an external IP address with dummy keys
            listen_address: "127.0.0.1:14617".to_string(),
            build_id: 918,
            auth_n_key: BigUint::from_bytes_be(&auth_n),
            auth_k_key: BigUint::from_bytes_be(&auth_k),
            game_n_key: BigUint::from_bytes_be(&game_n),
            game_k_key: BigUint::from_bytes_be(&game_k),
            gate_n_key: BigUint::from_bytes_be(&gate_n),
            gate_k_key: BigUint::from_bytes_be(&gate_k),
            file_serv_ip: "127.0.0.1".to_string(),
            auth_serv_ip: "127.0.0.1".to_string(),
            // Only works if we're running from a directory that contains a data dir...
            data_root: cwd.join("data"),
            db_type: VaultDbBackend::Sqlite,
        })
    }
}

#[derive(Deserialize)]
struct StructuredConfig {
    server: Option<ServerAddrConfig>,
    build_id: Option<u32>,
    data_root: Option<String>,
    crypt_keys: ConfigKeys,
    vault_db: Option<VaultDbConfig>,
}

#[derive(Deserialize, Default)]
struct ServerAddrConfig {
    listen_address: Option<String>,
    listen_port: Option<u16>,
    file_server_ip: Option<String>,
    auth_server_ip: Option<String>,
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
    db_type: Option<String>,
}
