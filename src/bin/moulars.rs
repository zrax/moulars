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

use std::io::{Write, stdout};
use std::sync::Arc;

use clap::{Command, Arg};
use num_bigint::{BigUint, ToBigUint};
use num_prime::RandPrime;

use moulars::config::ServerConfig;
use moulars::lobby::LobbyServer;

#[cfg(debug_assertions)]
const DEFAULT_LOG_LEVEL: &str = "debug";

#[cfg(not(debug_assertions))]
const DEFAULT_LOG_LEVEL: &str = "warn";

const CRYPT_BASE_AUTH: u32 = 41;
const CRYPT_BASE_GAME: u32 = 73;
const CRYPT_BASE_GATE_KEEPER: u32 = 4;

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
            ("GateKeeper", CRYPT_BASE_GATE_KEEPER)]
        {
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

            let stype_lower = stype.to_ascii_lowercase();
            server_lines.push(format!("{}.n = \"{}\"", stype_lower, base64::encode(bytes_n.clone())));
            server_lines.push(format!("{}.k = \"{}\"", stype_lower, base64::encode(bytes_k)));
            client_lines.push(format!("Server.{}.N \"{}\"", stype, base64::encode(bytes_n)));
            client_lines.push(format!("Server.{}.X \"{}\"", stype, base64::encode(bytes_x)));
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
    }

    let config = ServerConfig::dummy_config();
    server_main(config);
}

#[tokio::main]
async fn server_main(server_config: Arc<ServerConfig>) {
    LobbyServer::start(server_config).await;
}
