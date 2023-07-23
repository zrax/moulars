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
use std::fs::File;
use std::io::{Cursor, BufReader, Result, ErrorKind};
use std::path::Path;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use log::{warn, info};
use rand::Rng;

use crate::file_srv::data_cache::scan_dir;
use crate::plasma::file_crypt::{EncryptionType, EncryptedWriter};

pub fn build_secure_files(data_root: &Path) -> Result<()> {
    let ntd_key = load_or_create_ntd_key(data_root)?;

    let sdl_dir = data_root.join("SDL");
    if sdl_dir.exists() {
        // Ensure SDL files are encrypted
        let mut sdl_files = HashSet::new();
        scan_dir(&sdl_dir, &mut sdl_files)?;
        for sdl_file in sdl_files {
            encrypt_file(&sdl_file, &ntd_key)?;
        }
    } else {
        warn!("{} not found; skipping SDL files.", sdl_dir.display());
    }

    let python_dir = data_root.join("Python");
    if python_dir.exists() {
        // Build a .pak from the source files
        todo!()
    } else {
        warn!("{} not found; skipping Python files.", python_dir.display());
    }

    Ok(())
}

// NOTE: This file stores the keys in Big Endian format for easier debugging
// with tools like PlasmaShop
fn load_or_create_ntd_key(data_root: &Path) -> Result<[u32; 4]> {
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
                let mut stream = File::create(&key_path)?;
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

fn encrypt_file(path: &Path, key: &[u32; 4]) -> Result<()> {
    if EncryptionType::from_file(path)? == EncryptionType::Unencrypted {
        info!("Encrypting {}", path.display());
        // These files are generally small enough to just load entirely
        // into memory...
        let file_content = std::fs::read(path)?;
        let mut out_file = EncryptedWriter::new(File::create(path)?,
                                                EncryptionType::XXTEA, key)?;
        std::io::copy(&mut Cursor::new(file_content), &mut out_file)?;
    }
    Ok(())
}
