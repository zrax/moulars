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

use std::collections::{HashSet, VecDeque};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write, Result, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::Command;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use log::{warn, info};
use rand::Rng;

use crate::file_srv::data_cache::{scan_dir, encrypt_file};
use crate::plasma::{PakFile, StreamWrite};
use crate::plasma::file_crypt::{EncryptionType, EncryptedWriter};

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

pub fn build_secure_files(data_root: &Path, python_exe: Option<&Path>) -> Result<()> {
    let ntd_key = load_or_create_ntd_key(data_root)?;

    let sdl_dir = data_root.join("SDL");
    if sdl_dir.exists() {
        // Ensure SDL files are encrypted
        let mut sdl_files = HashSet::new();
        scan_dir(&sdl_dir, &mut sdl_files)?;
        for sdl_file in sdl_files {
            encrypt_file(&sdl_file, EncryptionType::XXTEA, &ntd_key)?;
        }
    } else {
        warn!("{} not found; skipping SDL files.", sdl_dir.display());
    }

    if let Some(python_exe) = python_exe {
        let python_dir = data_root.join("Python");
        if python_dir.exists() {
            // Build a .pak from the source files
            let py_sources = scan_python_dir(&python_dir)?;
            info!("Compiling {} Python sources...", py_sources.len());
            let mut pak_file = PakFile::new();
            for py_file in py_sources {
                let dfile = py_file.strip_prefix(&python_dir).unwrap();
                let cfile = py_file.with_extension("pyc");
                let py_cmd = format!(
                    "import py_compile; py_compile.compile('{}', cfile='{}', dfile='{}')",
                    py_escape(&py_file), py_escape(&cfile), py_escape(dfile)
                );
                let status = Command::new(python_exe).args(["-c", &py_cmd]).status()?;
                match status.code() {
                    Some(0) => (),
                    Some(code) => warn!("py_compile exited with status {}", code),
                    None => warn!("py_compile process killed by signal"),
                }
                let client_path = dfile.to_string_lossy().replace(['/', '\\'], ".");
                pak_file.add(&cfile, client_path)?;
            }

            // We always just write the .pak file if --python was specified.
            // Checking that it's up to date requires extra bookkeeping on
            // all the contained files, which the .pak file doesn't natively
            // store anywhere.
            let pak_path = python_dir.join("Python.pak");
            info!("Creating {}", pak_path.display());
            let mut pak_stream = EncryptedWriter::new(BufWriter::new(File::create(pak_path)?),
                                                      EncryptionType::XXTEA, &ntd_key)?;
            pak_file.stream_write(&mut pak_stream)?;
            pak_stream.flush()?;
        } else {
            warn!("{} not found; skipping Python files.", python_dir.display());
        }
    } else {
        warn!("No Python compiler specified.  Skipping Python files.");
    }

    Ok(())
}

// NOTE: This file stores the keys in Big Endian format for easier debugging
// with tools like PlasmaShop
pub fn load_or_create_ntd_key(data_root: &Path) -> Result<[u32; 4]> {
    let key_path = data_root.join(".ntd_server.key");
    let mut key_buffer = [0; 4];
    match File::open(&key_path) {
        Ok(file) => {
            let mut stream = BufReader::new(file);
            stream.read_u32_into::<BigEndian>(&mut key_buffer)?;
            Ok(key_buffer)
        }
        Err(err) => {
            if err.kind() == ErrorKind::NotFound {
                let mut rng = rand::thread_rng();
                let mut stream = BufWriter::new(File::create(&key_path)?);
                for v in key_buffer.iter_mut() {
                    *v = rng.gen::<u32>();
                    stream.write_u32::<BigEndian>(*v)?;
                }
                Ok(key_buffer)
            } else {
                Err(err)
            }
        }
    }
}

fn py_escape(path: &Path) -> String {
    path.to_string_lossy().replace('\\', r"\\").replace('\'', r#"\"""#)
}
