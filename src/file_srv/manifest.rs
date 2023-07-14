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

use std::io::{BufRead, BufReader, Write, BufWriter, Cursor, Result};
use std::fs::File;
use std::mem::size_of;
use std::path::Path;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::general_error;
use crate::plasma::{StreamRead, StreamWrite};

// Flags for FileInfo
pub const OGG_SPLIT_CHANNELS: u32 = 1 << 0;
pub const OGG_STREAM: u32 = 1 << 1;
pub const OGG_STEREO: u32 = 1 << 2;
pub const COMPRESSED_GZ: u32 = 1 << 3;
pub const REDIST_UPDATE: u32 = 1 << 4;
pub const DELETED: u32 = 1 << 5;

pub struct FileInfo {
    filename: String,
    download_name: String,
    file_hash: [u8; 16],
    download_hash: [u8; 16],
    file_size: u32,
    download_size: u32,
    flags: u32,
}

pub struct Manifest {
    files: Vec<FileInfo>,
}

macro_rules! read_utf16z_text {
    ($stream:ident) => ({
        let mut buffer = Vec::new();
        loop {
            let ch = $stream.read_u16::<LittleEndian>()?;
            if ch == 0 {
                break;
            }
            buffer.push(ch);
        }
        String::from_utf16_lossy(buffer.as_slice())
    })
}

fn to_nybble(ch: u16) -> Result<u8> {
    if ch >= b'0' as u16 && ch <= b'9' as u16 {
        Ok((ch as u8) - b'0')
    } else if ch >= b'A' as u16 && ch <= b'F' as u16 {
        Ok((ch as u8) - b'A' + 10)
    } else if ch >= b'a' as u16 && ch <= b'f' as u16 {
        Ok((ch as u8) - b'a' + 10)
    } else {
        Err(general_error!("Invalid hex digit in hash string: '{}'", ch))
    }
}

fn to_byte(hi: u16, lo: u16) -> Result<u8> {
    Ok(to_nybble(hi)? << 4 | to_nybble(lo)?)
}

macro_rules! read_utf16z_md5_hash {
    ($stream:ident) => ({
        // Convert UTF-16 hex to a binary hash
        let mut buffer = [0u8; 16];
        for i in 0..16 {
            let hi = $stream.read_u16::<LittleEndian>()?;
            let lo = $stream.read_u16::<LittleEndian>()?;
            buffer[i] = to_byte(hi, lo)?;
        }
        if $stream.read_u16::<LittleEndian>()? != 0 {
            return Err(general_error!("MD5 hash was not nul-terminated"));
        }
        buffer
    })
}

// Yes, it's as dumb as it sounds...
macro_rules! read_utf16z_u32 {
    ($stream:ident) => ({
        let value = ($stream.read_u16::<LittleEndian>()? as u32) << 16
                  | ($stream.read_u16::<LittleEndian>()? as u32);
        if $stream.read_u16::<LittleEndian>()? != 0 {
            return Err(general_error!("uint32 value was not nul-terminated"));
        }
        value
    })
}

impl StreamRead for FileInfo {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let filename = read_utf16z_text!(stream);
        let download_name = read_utf16z_text!(stream);
        let file_hash = read_utf16z_md5_hash!(stream);
        let download_hash = read_utf16z_md5_hash!(stream);
        let file_size = read_utf16z_u32!(stream);
        let download_size = read_utf16z_u32!(stream);
        let flags = read_utf16z_u32!(stream);

        Ok(Self { filename, download_name, file_hash, download_hash,
                  file_size, download_size, flags })
    }
}

macro_rules! write_utf16z_text {
    ($stream:ident, $value:expr) => {
        for ch in $value {
            $stream.write_u16::<LittleEndian>(ch)?;
        }
        $stream.write_u16::<LittleEndian>(0)?;
    }
}

fn hex_digits(byte: u8) -> (u8, u8) {
    const DIGITS: &[u8] = b"0123456789abcdef";
    (DIGITS[(byte >> 4) as usize], DIGITS[(byte & 0x0F) as usize])
}

macro_rules! write_utf16z_md5_hash {
    ($stream:ident, $value:expr) => {
        // Convert binary hash to a UTF-16 hex representation
        for ch in $value {
            let (hi, lo) = hex_digits(ch);
            $stream.write_u16::<LittleEndian>(hi as u16)?;
            $stream.write_u16::<LittleEndian>(lo as u16)?;
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

impl StreamWrite for FileInfo {
    fn stream_write<S>(&self, stream: &mut S) -> Result<()>
        where S: Write
    {
        write_utf16z_text!(stream, self.filename.encode_utf16());
        write_utf16z_text!(stream, self.download_name.encode_utf16());
        write_utf16z_md5_hash!(stream, self.file_hash);
        write_utf16z_md5_hash!(stream, self.download_hash);
        write_utf16z_u32!(stream, self.file_size);
        write_utf16z_u32!(stream, self.download_size);
        write_utf16z_u32!(stream, self.flags);

        Ok(())
    }
}

impl Manifest {
    const CACHE_MAGIC: u32 = 0x0153464d;    // 'MFS\x01'

    pub fn new() -> Self {
        Manifest { files: vec![] }
    }

    pub fn from_cache(path: &Path) -> Result<Self> {
        let mfs_file = File::open(path)?;
        let mut stream = BufReader::new(mfs_file);
        let cache_magic = stream.read_u32::<LittleEndian>()?;
        if cache_magic != Self::CACHE_MAGIC {
            return Err(general_error!("Unknown/invalid cache file magic '{:08x}'", cache_magic));
        }

        Manifest::stream_read(&mut stream)
    }

    pub fn write_cache(&self, path: &Path) -> Result<()> {
        let mfs_file = File::create(path)?;
        let mut stream = BufWriter::new(mfs_file);
        stream.write_u32::<LittleEndian>(Self::CACHE_MAGIC)?;
        self.stream_write(&mut stream)
    }
}

impl StreamRead for Manifest {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let num_files = stream.read_u32::<LittleEndian>()?;
        let mut files = Vec::with_capacity(num_files as usize);

        let char_count = stream.read_u32::<LittleEndian>()? as usize;
        let mut file_buffer = vec![0; char_count * size_of::<u16>()];
        stream.read_exact(file_buffer.as_mut_slice())?;

        let mut file_stream = Cursor::new(file_buffer);
        for _ in 0..num_files {
            files.push(FileInfo::stream_read(&mut file_stream)?);
        }
        if file_stream.read_u16::<LittleEndian>()? != 0 {
            return Err(general_error!("FileInfo array was not nul-terminated"));
        }

        Ok(Manifest { files })
    }
}

impl StreamWrite for Manifest {
    fn stream_write<S>(&self, stream: &mut S) -> Result<()>
        where S: Write
    {
        stream.write_u32::<LittleEndian>(self.files.len() as u32)?;

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
