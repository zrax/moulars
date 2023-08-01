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

use std::collections::{HashSet, HashMap, VecDeque};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{Cursor, BufReader, BufWriter, Write, Result, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::Command;

use log::{warn, info};
use once_cell::sync::Lazy;
use tempfile::NamedTempFile;

use crate::{general_error, path_utils};
use crate::config::load_or_create_ntd_key;
use crate::plasma::{AgeInfo, PageFile, PakFile, StreamWrite};
use crate::plasma::audio::SoundBuffer;
use crate::plasma::creatable::ClassID;
use crate::plasma::file_crypt::{self, EncryptionType, EncryptedWriter};
use super::manifest::{Manifest, FileInfo};
use super::server::ignore_file;

pub fn scan_dir(path: &Path, file_set: &mut HashSet<PathBuf>) -> Result<()> {
    for entry in path.read_dir()? {
        let entry = entry?;
        if !entry.metadata()?.is_file() {
            warn!("Skipping '{}' -- not a regular file", entry.path().display());
        }

        if !ignore_file(&entry.path(), false) {
            file_set.insert(entry.path());
        }
    }
    Ok(())
}

fn scan_python_dir(python_root: &Path) -> Result<HashSet<PathBuf>> {
    let mut file_set = HashSet::new();
    let mut scan_dirs = VecDeque::new();
    scan_dirs.push_back(python_root.to_owned());
    while let Some(dir) = scan_dirs.pop_front() {
        for entry in dir.read_dir()? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_file() && entry.path().extension() == Some(OsStr::new("py")) {
                file_set.insert(entry.path());
            } else if metadata.is_dir() {
                scan_dirs.push_back(entry.path());
            }
        }
    }
    Ok(file_set)
}

pub fn cache_clients(data_root: &Path, python_exe: Option<&Path>) -> Result<()> {
    static CLIENT_TYPES: Lazy<Vec<(&str, &str, PathBuf)>> = Lazy::new(|| vec![
        ("Internal", "", ["client", "windows_ia32", "internal"].iter().collect()),
        ("External", "", ["client", "windows_ia32", "external"].iter().collect()),
        ("Internal", "_x64", ["client", "windows_x64", "internal"].iter().collect()),
        ("External", "_x64", ["client", "windows_x64", "external"].iter().collect()),
    ]);

    let ntd_key = load_or_create_ntd_key(data_root)?;

    let mut game_data_files = HashSet::new();
    for data_dir in ["avi", "dat", "sfx", "SDL"] {
        let src_dir = data_root.join(data_dir);
        if src_dir.exists() && src_dir.is_dir() {
            scan_dir(&src_dir, &mut game_data_files)?;
        } else {
            warn!("{} does not exist.  Skipping {} files for manifests.",
                  src_dir.display(), data_dir);
        }
    }

    let python_dir = data_root.join("Python");
    if python_dir.exists() {
        let python_pak = python_dir.join("Python.pak");
        if let Some(python_exe) = python_exe {
            if let Err(err) = process_python(&python_dir, python_exe, &python_pak, &ntd_key) {
                warn!("Failed to build Python.pak: {}", err);
            }
        } else {
            warn!("No Python compiler specified.  Skipping Python files.");
        }
        if python_pak.exists() {
            game_data_files.insert(python_pak);
        }
    } else {
        warn!("{} does not exist.  Skipping Python files.", python_dir.display());
    }

    for file in &game_data_files {
        if let Some(ext) = file.extension() {
            if ext == OsStr::new("age") || ext == OsStr::new("fni") || ext == OsStr::new("csv") {
                // Ensure the file is encrypted for external clients
                if let Err(err) = encrypt_file(file, EncryptionType::TEA,
                                               &file_crypt::DEFAULT_KEY)
                {
                    warn!("Failed to encrypt {}: {}", file.display(), err);
                }
            } else if ext == OsStr::new("sdl") {
                if let Err(err) = encrypt_file(file, EncryptionType::XXTEA, &ntd_key) {
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

        let age_info = AgeInfo::from_file(age_file)?;
        let mut has_relevance = false;
        for page in age_info.pages() {
            let page_path = data_root.join("dat")
                    .join(format!("{}_District_{}.prp", age_name, page.name()));
            if page_path.exists() {
                expected_files.insert(page_path.clone());

                // Scan for and add any SFX files referenced by this PRP
                let mut prp_stream = BufReader::new(File::open(page_path)?);
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

                if page.has_keys(ClassID::RelevanceRegion as u16) {
                    has_relevance = true;
                }
            } else {
                warn!("Missing referenced Page file: {}", page_path.display());
            }
        }

        if has_relevance {
            let csv_path = age_file.with_extension("csv");
            expected_files.insert(csv_path);
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

    // Build a SecurePreloader manifest
    let secure_preloader_mfs_path = data_root.join("SecurePreloader.mfs_cache");
    let mut secure_preloader_mfs = load_or_create_manifest(&secure_preloader_mfs_path)?;
    let mut secure_preloader_files: HashSet<PathBuf>
            = game_data_files.iter().filter(|f| is_secure_preloader_file(f)).cloned().collect();
    for file in secure_preloader_mfs.files_mut() {
        *file = update_cache_file(&mut data_cache, file, data_root).clone();
        secure_preloader_files.remove(&file.source_path(data_root));
    }
    for sec_file in secure_preloader_files {
        let file = create_cache_file(&mut data_cache, &sec_file, data_root);
        secure_preloader_mfs.add(file.clone());
    }
    if secure_preloader_mfs.any_updated() {
        secure_preloader_mfs.write_cache(&secure_preloader_mfs_path)?;
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

fn encrypt_file(path: &Path, encryption_type: EncryptionType, key: &[u32; 4])
    -> Result<()>
{
    if EncryptionType::from_file(path)? == EncryptionType::Unencrypted {
        info!("Encrypting {}", path.display());
        // These files are generally small enough to just load entirely
        // into memory...
        let file_content = std::fs::read(path)?;
        let mut out_file = EncryptedWriter::new(BufWriter::new(File::create(path)?),
                                                encryption_type, key)?;
        std::io::copy(&mut Cursor::new(file_content), &mut out_file)?;
        out_file.flush()?;
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
        path_utils::to_windows(&src_path.to_string_lossy())
    };

    // The file might not have been in this manifest, but it could be
    // in others.  Use the cached version if it's available.
    data_cache.entry(path.to_path_buf()).or_insert_with(|| {
        let download_path = src_path.to_string_lossy();
        let mut file = FileInfo::new(client_path, &download_path);
        if let Err(err) = file.update(data_root) {
            warn!("Failed to add {} to the cache: {}", path.display(), err);
        }
        file
    })
}

fn compyle_dir(python_dir: &Path, python_exe: &Path, pak_file: &mut PakFile)
    -> Result<()>
{
    // Look for the glue code.  For now, we append this to everything in the
    // specified subdir if the glue file exists (this will be the case for
    // the plasma sources, but not generally for the system sources).  This
    // can probably be refined later...
    let glue_path = python_dir.join(["plasma", "glue.py"].iter().collect::<PathBuf>());
    let glue_source = if glue_path.exists() {
        std::io::read_to_string(BufReader::new(File::open(glue_path)?))?
    } else {
        info!("Skipping glue for {}.", python_dir.display());
        String::new()
    };

    let py_sources = scan_python_dir(python_dir)?;
    info!("Compiling {} Python sources from {}...", py_sources.len(),
          python_dir.display());

    // Spawning hundreds of Python processes is very slow, so we build a big
    // list and compile all sources in a single Python script instead
    let mut compyle_src = BufWriter::new(NamedTempFile::new()?);
    compyle_src.write_all(b"import py_compile\n")?;

    // We need to keep the NamedTempFile handles around so the files don't
    // get deleted before we can compile them...
    let mut temp_sources = Vec::new();

    for py_file in &py_sources {
        let dfile = py_file.strip_prefix(python_dir).unwrap();
        let cfile = py_file.with_extension("pyc");

        let (src_path, temp_src) = if glue_source.is_empty() {
            (py_file.clone(), None)
        } else {
            let mut temp_src = BufWriter::new(NamedTempFile::new()?);
            std::io::copy(&mut File::open(py_file)?, &mut temp_src)?;
            temp_src.write_all(b"\n\n")?;
            temp_src.write_all(glue_source.as_bytes())?;

            // NOTE: into_inner() will also flush the buffer
            let temp_src = temp_src.into_inner().map_err(|err| {
                general_error!("Failed to extract buffered stream: {}", err)
            })?;
            (temp_src.path().to_path_buf(), Some(temp_src))
        };

        writeln!(compyle_src, "py_compile.compile('{}', cfile='{}', dfile='{}')",
                 py_escape(&src_path), py_escape(&cfile), py_escape(dfile))?;
        temp_sources.push((cfile, dfile, temp_src));
    }

    // NOTE: into_inner() will also flush the buffer
    let compyle_src = compyle_src.into_inner().map_err(|err| {
        general_error!("Failed to extract buffered stream: {}", err)
    })?;
    match Command::new(python_exe).args([OsStr::new("-OO"),
                compyle_src.path().as_os_str()]).status()?.code()
    {
        Some(0) => (),
        Some(code) => warn!("py_compile exited with status {}", code),
        None => warn!("py_compile process killed by signal"),
    }
    for (cfile, dfile, _) in temp_sources {
        pak_file.add(&cfile, get_module_name(dfile, python_dir))?;
    }

    Ok(())
}

fn process_python(python_dir: &Path, python_exe: &Path, python_pak: &Path,
                  key: &[u32; 4]) -> Result<()>
{
    // Build a .pak from the source files
    let mut pak_file = PakFile::new();

    for subdir in ["plasma", "system"] {
        let python_subdir = python_dir.join(subdir);
        if python_subdir.exists() {
            compyle_dir(&python_subdir, python_exe, &mut pak_file)?;
        } else {
            warn!("Could not find {} python sources in {}", subdir,
                  python_subdir.display());
        }
    }

    // We always just write the .pak file if --python was specified.
    // Checking that it's up to date requires extra bookkeeping on
    // all the contained files, which the .pak file doesn't natively
    // store anywhere.
    info!("Creating {}", python_pak.display());
    let mut pak_stream = EncryptedWriter::new(BufWriter::new(File::create(python_pak)?),
                                              EncryptionType::XXTEA, key)?;
    pak_file.stream_write(&mut pak_stream)?;
    pak_stream.flush()?;

    Ok(())
}

fn py_escape(path: &Path) -> String {
    path.to_string_lossy().replace('\\', r"\\").replace('\'', r#"\"""#)
}

fn get_module_name(path: &Path, base_dir: &Path) -> String {
    let mut module_components = Vec::new();
    module_components.push(path.file_name().unwrap().to_string_lossy());
    for ancestor in path.ancestors().skip(1) {
        if let Some(component) = ancestor.file_name() {
            if base_dir.join(ancestor).join("__init__.py").exists() {
                // This is a module subdir
                module_components.push(component.to_string_lossy());
            }
        }
    }

    module_components.reverse();
    module_components.join(".")
}

fn is_secure_preloader_file(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        ext == OsStr::new("sdl") || ext == OsStr::new("pak")
    } else {
        false
    }
}
