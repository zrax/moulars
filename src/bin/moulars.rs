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

#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![allow(clippy::uninlined_format_args)]    // Added in Rust 1.66

use std::ffi::OsStr;
use std::io::{Write, stdout};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Command, Arg};
use log::error;
use num_bigint::{BigUint, ToBigUint};
use num_prime::RandPrime;

use moulars::config::ServerConfig;
use moulars::lobby::LobbyServer;
use moulars::net_crypt::{CRYPT_BASE_AUTH, CRYPT_BASE_GAME, CRYPT_BASE_GATE_KEEPER};

#[cfg(debug_assertions)]
const DEFAULT_LOG_LEVEL: &str = "debug";

#[cfg(not(debug_assertions))]
const DEFAULT_LOG_LEVEL: &str = "warn";

fn write_progress_pip() {
    let _ = stdout().write(b".");
    let _ = stdout().flush();
}

fn main() {
    // See https://docs.rs/env_logger/latest/env_logger/index.html for
    // details on fine-tuning logging behavior beyond the defaults.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(DEFAULT_LOG_LEVEL)
    ).init();

    let args = Command::new("moulars")
        .about("MOULArs: A Myst Online: Uru Live (Again) server")
        .version("0.1.0")
        .arg(Arg::new("keygen").long("keygen")
            .action(clap::ArgAction::SetTrue).exclusive(true)
            .help("Generate a set of Rc4 keys for client/server communication"))
        .arg(Arg::new("show-keys").long("show-keys")
            .action(clap::ArgAction::SetTrue).exclusive(true)
            .help("Show the client Rc4 keys associated with the configured server keys"))
        .get_matches();

    if args.get_flag("keygen") {
        // Progress pips are written on this line.  Deal with it.
        print!("Generating new keys. This will take a while");
        write_progress_pip();

        let mut rng = rand::thread_rng();
        let mut server_lines = Vec::with_capacity(6);
        let mut client_lines = Vec::with_capacity(6);
        for (stype, key_g) in [
            ("Auth", CRYPT_BASE_AUTH),
            ("Game", CRYPT_BASE_GAME),
            ("Gate", CRYPT_BASE_GATE_KEEPER)]
        {
            loop {
                let key_n: BigUint = rng.gen_safe_prime_exact(512);
                write_progress_pip();
                let key_k: BigUint = rng.gen_safe_prime_exact(512);
                write_progress_pip();
                let key_x = key_g.to_biguint().unwrap().modpow(&key_k, &key_n);
                write_progress_pip();

                // For best compatibility with H-uru/Plasma and DirtSand, the keys
                // are stored in Big Endian byte order
                let bytes_n = key_n.to_bytes_be();
                let bytes_k = key_k.to_bytes_be();
                let bytes_x = key_x.to_bytes_be();

                if bytes_n.len() != 64 || bytes_k.len() != 64 || bytes_x.len() != 64 {
                    // We generated a bad length key.  Somehow, this can happen
                    // despite the "exactly 512 bits" requested above.  So now
                    // we need to start over :(
                    continue;
                }

                let stype_lower = stype.to_ascii_lowercase();
                server_lines.push(format!("{}.n = \"{}\"", stype_lower, base64::encode(bytes_n.clone())));
                server_lines.push(format!("{}.k = \"{}\"", stype_lower, base64::encode(bytes_k)));
                client_lines.push(format!("Server.{}.N \"{}\"", stype, base64::encode(bytes_n)));
                client_lines.push(format!("Server.{}.X \"{}\"", stype, base64::encode(bytes_x)));
                break;
            }
        }
        println!("\n----------------------------");
        println!("Server keys: (moulars.toml)");
        println!("----------------------------");
        println!("[crypt_keys]");
        for line in server_lines {
            println!("{}", line);
        }
        println!("\n----------------------------");
        println!("Client keys: (server.ini)");
        println!("----------------------------");
        for line in client_lines {
            println!("{}", line);
        }

        std::process::exit(0);
    } else if args.get_flag("show-keys") {
        let config = load_config();

        println!("\n----------------------------");
        println!("Client keys: (server.ini)");
        println!("----------------------------");
        for (stype, key_g, key_k, key_n) in [
            ("Auth", CRYPT_BASE_AUTH, &config.auth_k_key, &config.auth_n_key),
            ("Game", CRYPT_BASE_GAME, &config.game_k_key, &config.game_n_key),
            ("Gate", CRYPT_BASE_GATE_KEEPER, &config.gate_k_key, &config.gate_n_key)]
        {
            let key_x = key_g.to_biguint().unwrap().modpow(key_k, key_n);
            let bytes_n = key_n.to_bytes_be();
            let bytes_x = key_x.to_bytes_be();
            println!("Server.{}.N \"{}\"", stype, base64::encode(bytes_n));
            println!("Server.{}.X \"{}\"", stype, base64::encode(bytes_x));
        }

        std::process::exit(0);
    }

    let server_config = load_config();
    let runtime = tokio::runtime::Builder::new_multi_thread()
                            .enable_all().build().unwrap();
    runtime.block_on(async {
        LobbyServer::start(server_config).await;
    });
}

fn load_config() -> Arc<ServerConfig> {
    // Look for a moulars.toml config file with the following precedence:
    //  1) In the same directory as the executable
    //  2) If the executable is in a bin/ directory, in ../etc/
    //  3) In the current working dir (debug builds only)
    //  4) In the root /etc/ dir

    let mut try_paths: Vec<PathBuf> = Vec::new();
    let config_file = Path::new("moulars.toml");

    let exe_path = match std::env::current_exe() {
        Ok(path) => path.parent().unwrap().to_owned(),
        Err(err) => {
            error!("Failed to get executable path: {}", err);
            std::process::exit(1);
        }
    };
    try_paths.push([exe_path.as_path(), config_file].iter().collect());

    if exe_path.file_name() == Some(OsStr::new("bin")) {
        let exe_parent = exe_path.parent().unwrap();
        try_paths.push([exe_parent, Path::new("etc"), config_file].iter().collect());
    }

    #[cfg(debug_assertions)]
    try_paths.push(config_file.to_owned());

    #[cfg(not(windows))]
    try_paths.push(Path::new("/etc/moulars.toml").to_owned());

    for path in &try_paths {
        if !path.exists() {
            continue;
        }
        match ServerConfig::from_file(path) {
            Ok(config) => return config,
            Err(err) => {
                error!("Failed to load config file {}: {}", path.display(), err);
                std::process::exit(1);
            }
        }
    }

    error!("Could not find a moulars.toml config file in any of the following locations:{}",
           try_paths.iter().fold(String::new(), |list, path| {
                list + format!("\n * {}", path.display()).as_str()
           }));
    error!("Please refer to moulars.toml.example for reference on configuring moulars.");
    std::process::exit(1);
}
