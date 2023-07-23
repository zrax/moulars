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
use std::fs::File;
use std::io::{Cursor, Result, ErrorKind};
use std::path::{Path, PathBuf};

use log::{warn, info};
use once_cell::sync::Lazy;

use crate::plasma::{AgeInfo, PageFile};
use crate::plasma::audio::SoundBuffer;
use crate::plasma::creatable::ClassID;
use crate::plasma::file_crypt::{self, EncryptionType, EncryptedWriter};
use super::manifest::{Manifest, FileInfo};

fn ignore_file(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        if ext == OsStr::new("gz") {
            // We don't send the client .gz files to leave compressed,
            // so this is probably a compressed version of another file
            return true;
        }
    }

    if let Some(file_name) = path.file_name() {
        if file_name == OsStr::new("desktop.ini")
                || file_name.to_string_lossy().starts_with('.') {
            return true;
        }
    }

    false
}

fn scan_dir(path: &Path, file_set: &mut HashSet<PathBuf>) -> Result<()> {
    for entry in path.read_dir()? {
        let entry = entry?;
        if !entry.metadata()?.is_file() {
            warn!("Skipping '{}' -- not a regular file", entry.path().display());
        }

        if !ignore_file(&entry.path()) {
            file_set.insert(entry.path());
        }
    }
    Ok(())
}

pub fn cache_clients(data_root: &Path) -> Result<()> {
    static CLIENT_TYPES: Lazy<Vec<(&str, &str, PathBuf)>> = Lazy::new(|| vec![
        ("Internal", "", ["client", "windows_ia32", "internal"].iter().collect()),
        ("External", "", ["client", "windows_ia32", "external"].iter().collect()),
        ("Internal", "_x64", ["client", "windows_x64", "internal"].iter().collect()),
        ("External", "_x64", ["client", "windows_x64", "external"].iter().collect()),
    ]);

    let mut game_data_files = HashSet::new();
    for data_dir in ["avi", "dat", "sfx"] {
        let src_dir = data_root.join(data_dir);
        if src_dir.exists() && src_dir.is_dir() {
            scan_dir(&src_dir, &mut game_data_files)?;
        } else {
            warn!("{} does not exist.  Skipping {} files for manifests.",
                  src_dir.display(), data_dir);
        }
    }

    for file in &game_data_files {
        if let Some(ext) = file.extension() {
            if ext == OsStr::new("age") || ext == OsStr::new("fni") || ext == OsStr::new("csv") {
                // Ensure the file is encrypted for external clients
                if let Err(err) = encrypt_file(file) {
                    warn!("Failed to encrypt {}: {}", file.display(), err);
                }
            }
        }
    }

    let mut data_cache = HashMap::new();
    let mut sfx_flags = HashMap::new();

    for age_file in game_data_files.iter().filter(|f| f.extension() == Some(OsStr::new("age"))) {
        let age_name = age_file.file_stem().unwrap().to_string_lossy();

        let mut expected_files = HashSet::new();
        expected_files.insert(age_file.clone());
        let fni_path = age_file.with_extension("fni");
        if fni_path.exists() {
            expected_files.insert(fni_path);
        }
        let csv_path = age_file.with_extension("csv");
        if csv_path.exists() {
            expected_files.insert(csv_path);
        }

        let age_info = AgeInfo::from_file(age_file)?;
        for page in age_info.pages() {
            let page_path = data_root.join("dat")
                    .join(format!("{}_District_{}.prp", age_name, page.name()));
            if page_path.exists() {
                expected_files.insert(page_path.clone());

                // Scan for and add any SFX files referenced by this PRP
                let mut prp_stream = std::io::BufReader::new(File::open(page_path)?);
                let page = PageFile::read(&mut prp_stream)?;
                for key in page.get_keys(ClassID::SoundBuffer as u16) {
                    let obj = page.read_object::<_, SoundBuffer>(&mut prp_stream, key.as_ref())?;
                    let sfx_path = data_root.join("sfx").join(obj.file_name());
                    sfx_flags.entry(sfx_path.clone()).or_insert(FileInfo::ogg_flags(&obj));
                    expected_files.insert(sfx_path.clone());

                    // Also look for a .sub file with the same name
                    let sub_file = sfx_path.with_extension("sub");
                    if sub_file.exists() {
                        expected_files.insert(sub_file);
                    }
                }
            } else {
                warn!("Missing referenced Page file: {}", page_path.display());
            }
        }

        let age_mfs_path = data_root.join(format!("{}.mfs_cache", age_name));
        let mut age_mfs = load_or_create_manifest(&age_mfs_path)?;
        for file in age_mfs.files_mut() {
            *file = update_cache_file(&mut data_cache, file, data_root).clone();
            expected_files.remove(&file.source_path(data_root));
        }
        for path in expected_files {
            let file = create_cache_file(&mut data_cache, &path, data_root);
            if path.extension() == Some(OsStr::new("ogg")) {
                let ogg_flags = sfx_flags.get(&path).expect("Got SFX file with no .ogg flags");
                file.add_flags(*ogg_flags);
            }
            age_mfs.add(file.clone());
        }
        if age_mfs.any_updated() {
            age_mfs.write_cache(&age_mfs_path)?;
        }
    }

    for (build, suffix, client_data_dir) in CLIENT_TYPES.iter() {
        let src_dir = data_root.join(client_data_dir);
        if !src_dir.exists() || !src_dir.is_dir() {
            warn!("{} does not exist.  Skipping manifest for {}{}",
                  client_data_dir.display(), build, suffix);
            continue;
        }

        // Fetch runtime files specific to this client configuration.
        let mut client_files = game_data_files.clone();
        scan_dir(&src_dir, &mut client_files)?;

        let patcher_mfs_path = data_root.join(
                format!("{}Patcher{}.mfs_cache", build, suffix));
        let thin_mfs_path = data_root.join(
                format!("Thin{}{}.mfs_cache", build, suffix));
        let full_mfs_path = data_root.join(
                format!("{}{}.mfs_cache", build, suffix));

        let mut patcher_mfs = load_or_create_manifest(&patcher_mfs_path)?;
        let mut thin_mfs = load_or_create_manifest(&thin_mfs_path)?;
        let mut full_mfs = load_or_create_manifest(&full_mfs_path)?;

        // Update any files already in the manifests
        for file in patcher_mfs.files_mut().iter_mut()
                        .chain(thin_mfs.files_mut().iter_mut())
                        .chain(full_mfs.files_mut().iter_mut())
        {
            *file = update_cache_file(&mut data_cache, file, data_root).clone();
            client_files.remove(&file.source_path(data_root));
        }

        for path in client_files {
            let file = create_cache_file(&mut data_cache, &path, data_root);

            // Add the newly detected file to the appropriate manifest(s)
            let client_path_lower = file.client_path().to_ascii_lowercase();
            let ext = path.extension();
            if client_path_lower.contains("vcredist") {
                file.set_redist_update();
                patcher_mfs.add(file.clone());
            } else if client_path_lower.contains("launcher") {
                patcher_mfs.add(file.clone());
            } else if ext == Some(OsStr::new("prp")) || ext == Some(OsStr::new("fni"))
                    || ext == Some(OsStr::new("csv")) || ext == Some(OsStr::new("ogg"))
                    || ext == Some(OsStr::new("sub")) {
                full_mfs.add(file.clone());
            } else {
                // Everything else goes into both client manifests
                thin_mfs.add(file.clone());
                full_mfs.add(file.clone());
            }
        }

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
        info!("Updating manifest {}", path.display());
        Manifest::from_cache(path)
    } else {
        info!("Creating manifest {}", path.display());
        Ok(Manifest::new())
    }
}

fn encrypt_file(path: &Path) -> Result<()> {
    if EncryptionType::from_file(path)? == EncryptionType::Unencrypted {
        info!("Encrypting {}", path.display());
        // These files are generally small enough to just load entirely
        // into memory...
        let file_content = std::fs::read(path)?;
        let mut out_file = EncryptedWriter::new(File::create(path)?,
                                EncryptionType::TEA, &file_crypt::DEFAULT_KEY)?;
        std::io::copy(&mut Cursor::new(file_content), &mut out_file)?;
    }
    Ok(())
}

fn update_cache_file<'dc>(data_cache: &'dc mut HashMap<PathBuf, FileInfo>,
                          file: &FileInfo, data_root: &Path) -> &'dc mut FileInfo
{
    data_cache.entry(file.source_path(data_root)).or_insert_with(|| {
        let mut file = file.clone();
        if let Err(err) = file.update(data_root) {
            if err.kind() == ErrorKind::NotFound {
                warn!("Removing {}", file.client_path());
                file.mark_deleted();
            } else {
                warn!("Failed to update cache for file {}: {}",
                      file.client_path(), err);
            }
        }
        file
    })
}

fn create_cache_file<'dc>(data_cache: &'dc mut HashMap<PathBuf, FileInfo>,
                          path: &Path, data_root: &Path) -> &'dc mut FileInfo
{
    let src_path = path.strip_prefix(data_root).unwrap();
    info!("Adding {}", src_path.display());

    let client_path = if src_path.starts_with("client") {
        src_path.file_name().unwrap().to_string_lossy().to_string()
    } else {
        src_path.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "\\")
    };

    // The file might not have been in this manifest, but it could be
    // in others.  Use the cached version if it's available.
    data_cache.entry(path.to_path_buf()).or_insert_with(|| {
        let download_path = src_path.to_string_lossy().to_string();
        let mut file = FileInfo::new(client_path, download_path);
        if let Err(err) = file.update(data_root) {
            warn!("Failed to add {} to the cache: {}", path.display(), err);
        }
        file
    })
}
