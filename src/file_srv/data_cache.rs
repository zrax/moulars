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

use std::collections::{HashSet, HashMap};
use std::ffi::OsStr;
use std::io::Result;
use std::path::{Path, PathBuf};

use lazy_static::lazy_static;
use log::{warn, debug};

use crate::config::ServerConfig;
use super::manifest::{Manifest, FileInfo};

pub fn cache_clients(server_config: &ServerConfig) -> Result<()> {
    lazy_static! {
        static ref CLIENT_TYPES: Vec<(&'static str, &'static str, PathBuf)> = vec![
            ("Internal", "", ["client", "windows_ia32", "internal"].iter().collect()),
            ("External", "", ["client", "windows_ia32", "external"].iter().collect()),
            ("Internal", "_x64", ["client", "windows_x64", "internal"].iter().collect()),
            ("External", "_x64", ["client", "windows_x64", "external"].iter().collect()),
        ];
    }

    for (build, suffix, data_dir) in CLIENT_TYPES.iter() {
        let src_dir = server_config.file_data_root.join(data_dir);
        if !src_dir.exists() {
            warn!("{} does not exist.  Skipping manifest for {}{}",
                  data_dir.display(), build, suffix);
            continue;
        }

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
                warn!("Skipping '{}' -- not a regular file", entry.path().display());
            }
        }

        let patcher_mfs_path = server_config.file_data_root.join(
                format!("{}Patcher{}.mfs_cache", build, suffix));
        let thin_mfs_path = server_config.file_data_root.join(
                format!("Thin{}{}.mfs_cache", build, suffix));
        let full_mfs_path = server_config.file_data_root.join(
                format!("{}{}.mfs_cache", build, suffix));

        let mut patcher_mfs = load_or_create_manifest(&patcher_mfs_path)?;
        let mut thin_mfs = load_or_create_manifest(&thin_mfs_path)?;
        let mut full_mfs = load_or_create_manifest(&full_mfs_path)?;

        // Ensure we process files found in multiple manifests only once
        // NOTE: The thin manifest contains no unique files.
        let mut all_client_files = HashMap::with_capacity(
                    patcher_mfs.files().len() + full_mfs.files().len());
        for file in patcher_mfs.files().iter().chain(full_mfs.files().iter()) {
            all_client_files.insert(file.client_path().clone(), file.clone());
        }

        for file in all_client_files.values_mut() {
            if let Err(err) = file.update(server_config) {
                // TODO: If the error is NotFound, should we mark the file as deleted?
                warn!("Failed to update cache for file {}: {}",
                      file.client_path(), err);
            }
            discovered_files.remove(&file.source_path(server_config));
        }

        for file in patcher_mfs.files_mut().iter_mut()
                        .chain(thin_mfs.files_mut().iter_mut())
                        .chain(full_mfs.files_mut().iter_mut())
        {
            *file = all_client_files.get(file.client_path()).unwrap().clone();
        }

        for path in discovered_files {
            debug!("Adding {}", path.display());
            let client_path = path.file_name().unwrap().to_string_lossy().to_string();
            let mut file = FileInfo::new(client_path, path.to_string_lossy().to_string());
            if let Err(err) = file.update(server_config) {
                warn!("Failed to add {} to the cache: {}", path.display(), err);
                continue;
            }

            // Add the newly detected file to the appropriate manifest(s)
            let client_path_lower = file.client_path().to_ascii_lowercase();
            if client_path_lower.contains("vcredist") {
                file.set_redist_update();
                patcher_mfs.add(file);
            } else if client_path_lower.contains("launcher") {
                patcher_mfs.add(file);
            } else {
                // Everything else goes into the client manifests
                thin_mfs.add(file.clone());
                full_mfs.add(file);
            }
        }

        // TODO: Also add client data files to the Thin and Full manifests

        if patcher_mfs.any_updated() {
            patcher_mfs.write_cache(&patcher_mfs_path)?;
        }
        if thin_mfs.any_updated() {
            thin_mfs.write_cache(&thin_mfs_path)?;
        }
        if full_mfs.any_updated() {
            full_mfs.write_cache(&full_mfs_path)?;
        }
    }

    Ok(())
}

fn load_or_create_manifest(path: &Path) -> Result<Manifest> {
    if path.exists() {
        debug!("Updating manifest {}", path.display());
        Manifest::from_cache(path)
    } else {
        debug!("Creating manifest {}", path.display());
        Ok(Manifest::new())
    }
}
