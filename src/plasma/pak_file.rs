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

use std::fs::File;
use std::io::{Cursor, Write, Result};
use std::mem::size_of;
use std::path::Path;

use byteorder::{LittleEndian, WriteBytesExt};

use crate::general_error;
use crate::plasma::{StreamWrite, StringFormat, write_safe_str};

struct FileInfo {
    name: String,
    data: Vec<u8>,
}

#[derive(Default)]
pub struct PakFile {
    files: Vec<FileInfo>
}

impl PakFile {
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    pub fn add(&mut self, path: &Path, stored_name: String) -> Result<()> {
        let mut file = File::open(path)?;
        let mut buffer = Cursor::new(Vec::new());
        std::io::copy(&mut file, &mut buffer)?;
        self.files.push(FileInfo { name: stored_name, data: buffer.into_inner() });
        Ok(())
    }
}

impl StreamWrite for PakFile {
    fn stream_write<S>(&self, stream: &mut S) -> Result<()>
        where S: Write
    {
        stream.write_u32::<LittleEndian>(self.files.len() as u32)?;
        let mut offset_accum = size_of::<u32>();

        // Compute the offsets first, so we don't have to do a bunch of
        // seeking later.
        offset_accum = self.files.iter().fold(offset_accum, |acc, file| {
            // Safe String (u16 + string) + u32 offset
            acc + size_of::<u16>() + file.name.as_bytes().len() + size_of::<u32>()
        });

        // Write the table of contents with computed offsets
        for file in &self.files {
            if offset_accum > (u32::MAX as usize) {
                return Err(general_error!("Pak file contents too large"));
            }
            write_safe_str(stream, &file.name, StringFormat::Utf8)?;
            stream.write_u32::<LittleEndian>(offset_accum as u32)?;

            // The data includes a u32 size header
            offset_accum += size_of::<u32>() + file.data.len();
        }

        // Write the file content
        for file in &self.files {
            stream.write_u32::<LittleEndian>(file.data.len() as u32)?;
            stream.write_all(file.data.as_slice())?;
        }

        Ok(())
    }
}
