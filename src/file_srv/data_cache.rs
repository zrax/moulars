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

use std::collections::HashSet;
use std::ffi::OsStr;
use std::io::Result;
use std::path::PathBuf;

use lazy_static::lazy_static;

use crate::config::ServerConfig;
use super::manifest::{Manifest, FileInfo};

pub fn cache_clients(server_config: &ServerConfig) -> Result<()> {
    lazy_static! {
        static ref CLIENT_TYPES: Vec<(&'static str, PathBuf)> = vec![
            ("InternalPatcher", ["client", "windows_ia32", "internal"].iter().collect()),
            ("ExternalPatcher", ["client", "windows_ia32", "external"].iter().collect()),
            ("InternalPatcher_x64", ["client", "windows_x64", "internal"].iter().collect()),
            ("ExternalPatcher_x64", ["client", "windows_x64", "external"].iter().collect()),
        ];
    }

    for (mfs_name, data_dir) in CLIENT_TYPES.iter() {
        let src_dir = server_config.file_data_root.join(data_dir);
        if !src_dir.exists() {
            eprintln!("Warning: {} does not exist.  Skipping manifest for {}",
                      data_dir.display(), mfs_name);
            continue;
        }

        let manifest_path = server_config.file_data_root.join((*mfs_name).to_owned() + ".mfs_cache");
        let mut manifest = if manifest_path.exists() {
            println!("Updating manifest {}", manifest_path.display());
            Manifest::from_cache(&manifest_path)?
        } else {
            println!("Creating manifest {}", manifest_path.display());
            Manifest::new()
        };

        let mut discovered_files = HashSet::new();
        for entry in src_dir.read_dir()? {
            let entry = entry?;
            if entry.path().extension() == Some(OsStr::new("gz")) {
                // We don't send the client .gz files to leave compressed,
                // so this is probably a compressed version of another file
                continue;
            }

            let metadata = entry.metadata()?;
            if metadata.is_file() {
                discovered_files.insert(entry.path());
            } else {
                eprintln!("Skipping '{}' -- not a regular file",
                          entry.path().display());
            }
        }

        for file in manifest.files_mut() {
            if let Err(err) = file.update(server_config) {
                // TODO: If the error is NotFound, should we mark the file as deleted?
                eprintln!("Warning: Failed to update cache for file {}: {}",
                          file.client_path(), err);
            }
            discovered_files.remove(&file.source_path(server_config));
        }
        for path in discovered_files {
            println!("Adding {}", path.display());
            let client_path = path.file_name().unwrap().to_string_lossy().to_string();
            let mut file = FileInfo::new(client_path, path.to_string_lossy().to_string());
            if let Err(err) = file.update(server_config) {
                eprintln!("Warning: Failed to add {} to the cache: {}",
                          path.display(), err);
                continue;
            }
            manifest.add(file);
        }

        manifest.write_cache(&manifest_path)?;
    }

    Ok(())
}
