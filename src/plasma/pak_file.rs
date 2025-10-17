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
use std::io::{Cursor, BufRead, BufReader, Write};
use std::mem::size_of;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::plasma::{StreamRead, StreamWrite};
use crate::plasma::safe_string::{ReadSafeStr, WriteSafeStr, StringFormat};

pub struct FileInfo {
    name: String,
    data: Vec<u8>,
}

#[derive(Default)]
pub struct PakFile {
    files: Vec<FileInfo>
}

impl FileInfo {
    pub fn name(&self) -> &String { &self.name }
}

impl PakFile {
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    pub fn add(&mut self, path: &Path, stored_name: String) -> Result<()> {
        let mut file = BufReader::new(File::open(path)?);
        skip_pyc_headers(&mut file)?;
        let mut buffer = Cursor::new(Vec::new());
        std::io::copy(&mut file, &mut buffer)?;
        self.files.push(FileInfo { name: stored_name, data: buffer.into_inner() });
        Ok(())
    }

    pub fn files(&self) -> &Vec<FileInfo> { &self.files }
}

impl StreamRead for PakFile {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let num_files = stream.read_u32::<LittleEndian>()?;
        let mut files = Vec::with_capacity(num_files as usize);

        for _ in 0..num_files {
            let name = stream.read_safe_str(StringFormat::Utf8)?;
            // Offset.  We hope they're in order...
            let _ = stream.read_u32::<LittleEndian>()?;
            files.push(FileInfo { name, data: Vec::new() });
        }
        for file in &mut files {
            let size = stream.read_u32::<LittleEndian>()?;
            file.data.resize(size as usize, 0);
            stream.read_exact(file.data.as_mut_slice())?;
        }

        Ok(Self { files })
    }
}

impl StreamWrite for PakFile {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        let num_files = u32::try_from(self.files.len()).context("Too many files for stream")?;
        stream.write_u32::<LittleEndian>(num_files)?;
        let mut offset_accum = size_of::<u32>();

        // Compute the offsets first, so we don't have to do a bunch of
        // seeking later.
        offset_accum = self.files.iter().fold(offset_accum, |acc, file| {
            // Safe String (u16 + string) + u32 offset
            acc + size_of::<u16>() + file.name.len() + size_of::<u32>()
        });

        // Write the table of contents with computed offsets
        for file in &self.files {
            let cur_offset = u32::try_from(offset_accum)
                    .context("Pak file contents too large")?;
            stream.write_safe_str(&file.name, StringFormat::Utf8)?;
            stream.write_u32::<LittleEndian>(cur_offset)?;

            // The data includes a u32 size header
            offset_accum += size_of::<u32>() + file.data.len();
        }

        // Write the file content
        for file in &self.files {
            let file_size = u32::try_from(file.data.len()).context("Pak file contents too large")?;
            stream.write_u32::<LittleEndian>(file_size)?;
            stream.write_all(file.data.as_slice())?;
        }

        Ok(())
    }
}

const MAGIC_PY_MIN: u32 = 0x0A0D0000;
const MAGIC_PY_MAX: u32 = 0x0A0DFFFF;

// WARNING:  Python magic numbers are NOT in order; specifically, there was
// a break from 2.x to 3.0, so the ranges must be checked as well.
//const MAGIC_PY3_MIN: u32 = 0x0A0D0C3A;
const MAGIC_PY_3_3: u32 = 0x0A0D0C9E;
const MAGIC_PY_3_7: u32 = 0x0A0D0D42;
const MAGIC_PY2_MIN: u32 = 0x0A0DC687;

fn skip_pyc_headers<S>(stream: &mut S) -> Result<()>
    where S: BufRead
{
    // Skip the file headers.  How much to skip depends on the Python
    // version, which can be simplified by looking at the magic number.
    let magic = stream.read_u32::<LittleEndian>()?;
    if !(MAGIC_PY_MIN..=MAGIC_PY_MAX).contains(&magic) {
        return Err(anyhow!("Unsupported Python version or not a pyc file"));
    }

    let flags = if (MAGIC_PY_3_7..MAGIC_PY2_MIN).contains(&magic) {
        stream.read_u32::<LittleEndian>()?
    } else {
        0
    };
    if (flags & 0x1) != 0 {
        // Optional checksums added in Python 3.7
        let _ = stream.read_u32::<LittleEndian>()?;
        let _ = stream.read_u32::<LittleEndian>()?;
    } else {
        // Timestamp
        let _ = stream.read_u32::<LittleEndian>()?;
        if (MAGIC_PY_3_3..MAGIC_PY2_MIN).contains(&magic) {
            // Size parameter added in Python 3.3
            let _ = stream.read_u32::<LittleEndian>()?;
        }
    }

    Ok(())
}
