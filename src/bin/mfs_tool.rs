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

// This program converts a moulars .mfs_cache file into a DirtSand .mfs on
// stdout.  It can be useful for debugging, as well as porting data back to
// DirtSand for compatibility/comparison.

use std::fs::File;
use std::io::{Cursor, BufReader, BufWriter, Result};
use std::mem::size_of;
use std::path::{Path, PathBuf};

use clap::{Command, Arg, ArgAction};
use clap::builder::PathBufValueParser;
use log::{error, warn};

use moulars::auth_srv::sec_files::build_secure_files;
use moulars::file_srv::Manifest;
use moulars::file_srv::data_cache::cache_clients;
use moulars::general_error;
use moulars::plasma::{StreamRead, PakFile};
use moulars::plasma::file_crypt::EncryptedReader;

fn main() {
    // Just print log messages generated by the moulars library
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .format_target(false)
        .init();

    let decrypt_cmd = Command::new("decrypt")
        .about("Decrypt an encrypted file")
        .arg(Arg::new("key").value_name("key_value").short('k').long("key")
                .help("Big Endian key to use for decryption"))
        .arg(Arg::new("out_filename").value_name("out_file").short('o').long("out")
                .value_parser(PathBufValueParser::new())
                .conflicts_with("in_place")
                .help("Write output to file instead of stdout"))
        .arg(Arg::new("in_place").short('i').long("in-place")
                .action(ArgAction::SetTrue)
                .help("Decrypt file in-place"))
        .arg(Arg::new("filename").required(true)
                .value_parser(PathBufValueParser::new()));

    let dump_cmd = Command::new("dump")
        .about("Dump a cached manifest in DirtSand format")
        .arg(Arg::new("mfs_cache").required(true)
                .value_parser(PathBufValueParser::new()));

    let list_pak_cmd = Command::new("ls-pak")
        .about("List the files in an optionally encrypted .pak file")
        .arg(Arg::new("key").value_name("key_value").short('k').long("key")
                .help("Big Endian key to use for decryption"))
        .arg(Arg::new("pak_file").required(true)
                .value_parser(PathBufValueParser::new()));

    let update_cmd = Command::new("update")
        .about("Update manifests and secure files for the data and auth servers")
        .arg(Arg::new("python_exe").value_name("python_exe").long("python")
                .value_parser(PathBufValueParser::new())
                .help("Path to Python executable for compiling .pak files"))
        .arg(Arg::new("data_root").required(true)
                .value_parser(PathBufValueParser::new()));

    let args = Command::new("mfs_tool")
        .version("1.0")
        .about("Tool for updating and debugging manifests for MOULArs")
        .subcommand(decrypt_cmd)
        .subcommand(dump_cmd)
        .subcommand(list_pak_cmd)
        .subcommand(update_cmd)
        .arg_required_else_help(true)
        .get_matches();

    match args.subcommand() {
        Some(("decrypt", decrypt_args)) => {
            let file_path = decrypt_args.get_one::<PathBuf>("filename").unwrap();
            let out_file = if decrypt_args.get_flag("in_place") {
                Some(file_path.as_path())
            } else {
                decrypt_args.get_one::<PathBuf>("out_filename").map(|v| v.as_path())
            };
            let key_opt = decrypt_args.get_one::<String>("key").map(|v| v.as_str());
            if let Err(err) = decrypt_file(file_path, out_file, key_opt) {
                error!("Failed to decrypt {}: {}", file_path.display(), err);
                std::process::exit(1);
            }
        }
        Some(("dump", dump_args)) => {
            let mfs_cache = dump_args.get_one::<PathBuf>("mfs_cache").unwrap();
            let manifest = match Manifest::from_cache(mfs_cache) {
                Ok(manifest) => manifest,
                Err(err) => {
                    error!("Failed to load manifest cache: {}", err);
                    std::process::exit(1);
                }
            };
            for file in manifest.files() {
                println!("{}", file.as_ds_mfs());
            }
        }
        Some(("ls-pak", ls_pak_args)) => {
            let pak_file = ls_pak_args.get_one::<PathBuf>("pak_file").unwrap();
            let key_opt = ls_pak_args.get_one::<String>("key").map(|v| v.as_str());
            if let Err(err) = list_pak(pak_file, key_opt) {
                error!("Failed to load pak file: {}", err);
                std::process::exit(1);
            }
        }
        Some(("update", update_args)) => {
            let data_root = update_args.get_one::<PathBuf>("data_root").unwrap();
            if let Err(err) = cache_clients(data_root) {
                warn!("Failed to update file server cache: {}", err);
            }
            let python_exe = update_args.get_one::<PathBuf>("python_exe").map(|p| p.as_path());
            if let Err(err) = build_secure_files(data_root, python_exe) {
                warn!("Failed to build secure files: {}", err);
            }
        }
        _ => ()
    }
}

fn get_key(key_opt: Option<&str>) -> Result<[u32; 4]> {
    if let Some(key_str) = key_opt {
        let mut buffer = [0; 16];
        match hex::decode_to_slice(key_str, &mut buffer) {
            Ok(()) => {
                let mut key = [0; 4];
                for (src, dest) in buffer.chunks_exact(size_of::<u32>()).zip(key.iter_mut()) {
                    *dest = u32::from_be_bytes(src.try_into().unwrap());
                }
                Ok(key)
            }
            Err(err) => {
                Err(general_error!("Invalid key value: {}", err))
            }
        }
    } else {
        Ok(moulars::plasma::file_crypt::DEFAULT_KEY)
    }
}

fn decrypt_file(path: &Path, out_file: Option<&Path>, key_opt: Option<&str>)
    -> Result<()>
{
    let key = get_key(key_opt)?;
    let mut stream = EncryptedReader::new(BufReader::new(File::open(path)?), &key)?;
    if let Some(out_filename) = out_file {
        if out_filename.exists() &&
                std::fs::canonicalize(out_filename)? == std::fs::canonicalize(path)?
        {
            // The files are the same, so we need to decrypt it in memory first...
            let mut in_stream = Cursor::new(Vec::new());
            std::io::copy(&mut stream, &mut in_stream)?;
            drop(stream);

            in_stream.set_position(0);
            let mut out_stream = BufWriter::new(File::create(out_filename)?);
            std::io::copy(&mut in_stream, &mut out_stream)?;
        } else {
            let mut out_stream = BufWriter::new(File::create(out_filename)?);
            std::io::copy(&mut stream, &mut out_stream)?;
        };
    } else {
        std::io::copy(&mut stream, &mut std::io::stdout())?;
    }
    Ok(())
}

fn list_pak(path: &Path, key_opt: Option<&str>) -> Result<()>
{
    let key = get_key(key_opt)?;
    let file_reader = BufReader::new(File::open(path)?);
    let mut stream = BufReader::new(EncryptedReader::new(file_reader, &key)?);
    let pak_file = PakFile::stream_read(&mut stream)?;
    for file in pak_file.files() {
        println!("{}", file.name());
    }
    Ok(())
}
