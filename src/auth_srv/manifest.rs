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

use std::ffi::OsStr;
use std::io::{BufRead, Write, Cursor, Result};
use std::mem::size_of;
use std::path::Path;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use log::warn;

use crate::general_error;
use crate::plasma::{StreamRead, StreamWrite};
use crate::file_srv::manifest::{read_utf16z_text, write_utf16z_text,
                                read_utf16z_u32, write_utf16z_u32};

#[derive(Clone, Debug)]
pub struct FileInfo {
    path: String,
    file_size: u32,
}

#[derive(Default)]
pub struct Manifest {
    files: Vec<FileInfo>,
}

impl StreamRead for FileInfo {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let path = read_utf16z_text(stream)?;
        let file_size = read_utf16z_u32(stream)?;
        Ok(Self { path, file_size })
    }
}

impl StreamWrite for FileInfo {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        write_utf16z_text(stream, &self.path)?;
        write_utf16z_u32(stream, self.file_size)?;
        Ok(())
    }
}

impl Manifest {
    pub fn new() -> Self {
        Manifest { files: vec![] }
    }

    pub fn from_dir(data_root: &Path, directory: &str, ext: &str) -> Result<Self> {
        let mut files = Vec::new();
        for entry in data_root.join(directory).read_dir()? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_file() && entry.path().extension() == Some(OsStr::new(ext)) {
                if metadata.len() > u64::from(u32::MAX) {
                    warn!("File {} is too large to send to client; Ignoring it...",
                          entry.path().display());
                    continue;
                }
                let client_path = format!("{}\\{}", directory, entry.file_name().to_string_lossy());
                files.push(FileInfo { path: client_path, file_size: metadata.len() as u32 });
            }
        }
        Ok(Manifest { files })
    }

    pub fn files(&self) -> &Vec<FileInfo> { &self.files }
    pub fn files_mut(&mut self) -> &mut Vec<FileInfo> { &mut self.files }
    pub fn add(&mut self, file: FileInfo) { self.files.push(file); }
}

impl StreamRead for Manifest {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let mut files = Vec::new();

        let char_count = stream.read_u32::<LittleEndian>()? as usize;
        let mut file_buffer = vec![0; char_count * size_of::<u16>()];
        stream.read_exact(file_buffer.as_mut_slice())?;

        let last_char_pos = file_buffer.len() - size_of::<u16>();
        let mut file_stream = Cursor::new(file_buffer);
        while (file_stream.position() as usize) < last_char_pos {
            files.push(FileInfo::stream_read(&mut file_stream)?);
        }
        if file_stream.read_u16::<LittleEndian>()? != 0 {
            return Err(general_error!("FileInfo array was not nul-terminated"));
        }

        Ok(Manifest { files })
    }
}

impl StreamWrite for Manifest {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        let mut file_stream = Cursor::new(Vec::new());
        for file in &self.files {
            file.stream_write(&mut file_stream)?;
        }
        file_stream.write_u16::<LittleEndian>(0)?;

        let file_buf = file_stream.into_inner();
        assert_eq!(file_buf.len() % size_of::<u16>(), 0);
        stream.write_u32::<LittleEndian>((file_buf.len() / size_of::<u16>()) as u32)?;
        stream.write_all(file_buf.as_slice())
    }
}
