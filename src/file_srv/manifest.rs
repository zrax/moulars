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

use std::io::{Cursor, Result};

use byteorder::{LittleEndian, WriteBytesExt};

pub struct FileInfo {
    filename: String,
    download_name: String,
    file_hash: [u16; 32],
    download_hash: [u16; 32],
    file_size: u32,
    download_size: u32,
    flags: u32,
}

pub struct Manifest {
    files: Vec<FileInfo>,
}

macro_rules! write_utf16z_text {
    ($stream:ident, $source:expr) => {
        for ch in $source {
            $stream.write_u16::<LittleEndian>(ch)?;
        }
        $stream.write_u16::<LittleEndian>(0)?;
    }
}

// Yes, it's as dumb as it sounds...
macro_rules! write_utf16z_u32 {
    ($stream:ident, $value:expr) => {
        $stream.write_u16::<LittleEndian>(($value >> 16) as u16)?;
        $stream.write_u16::<LittleEndian>(($value & 0xFFFF) as u16)?;
        $stream.write_u16::<LittleEndian>(0)?;
    }
}

impl FileInfo {
    pub fn encode_to_stream(&self, stream: &mut Cursor<Vec<u8>>) -> Result<()> {
        write_utf16z_text!(stream, self.filename.encode_utf16());
        write_utf16z_text!(stream, self.download_name.encode_utf16());
        write_utf16z_text!(stream, self.file_hash);
        write_utf16z_text!(stream, self.download_hash);
        write_utf16z_u32!(stream, self.file_size);
        write_utf16z_u32!(stream, self.download_size);
        write_utf16z_u32!(stream, self.flags);

        Ok(())
    }
}

impl Manifest {
    pub fn new() -> Self {
        Manifest { files: vec![] }
    }

    pub fn num_files(&self) -> u32 { self.files.len() as u32 }

    pub fn encode_for_stream(&self) -> Result<Vec<u8>> {
        let mut stream = Cursor::new(Vec::new());

        for file in &self.files {
            file.encode_to_stream(&mut stream)?;
        }

        Ok(stream.into_inner())
    }
}
