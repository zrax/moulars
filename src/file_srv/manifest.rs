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

use std::io::{self, BufRead, BufReader, Write, BufWriter, Cursor};
use std::ffi::OsStr;
use std::fs::File;
use std::mem::size_of;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use data_encoding::{HEXLOWER, HEXLOWER_PERMISSIVE};
use flate2::write::GzEncoder;
use log::debug;
use md5::{Md5, Digest};

use crate::path_utils;
use crate::plasma::{StreamRead, StreamWrite};
use crate::plasma::audio::SoundBuffer;

#[derive(Debug, Clone)]
pub struct FileInfo {
    client_path: String,
    download_path: String,
    file_hash: [u8; 16],
    download_hash: [u8; 16],
    file_size: u32,
    download_size: u32,
    flags: u32,
    updated: bool,
    deleted: bool,
}

#[derive(Default)]
pub struct Manifest {
    files: Vec<FileInfo>,
}

fn md5_hash_file(path: &Path) -> Result<[u8; 16]> {
    let mut file = File::open(path)?;
    let mut hash = Md5::new();
    std::io::copy(&mut file, &mut hash)?;
    Ok(hash.finalize().into())
}

impl FileInfo {
    // Flags for FileInfo
    const OGG_SPLIT_CHANNELS: u32 = 1 << 0;
    const OGG_STREAM_COMPRESSED: u32 = 1 << 1;
    const OGG_STEREO: u32 = 1 << 2;
    const COMPRESSED_GZ: u32 = 1 << 3;
    const REDIST_UPDATE: u32 = 1 << 4;

    // Creates a new entry with invalid hash/size data.
    // It will need to be populated with real data via update() before
    // it can be send to a client.
    pub fn new(client_path: String, download_path: &str) -> Self {
        let download_path = path_utils::to_windows(download_path);

        Self {
            client_path,
            download_path,
            file_hash: [0; 16],
            download_hash: [0; 16],
            file_size: 0,
            download_size: 0,
            flags: 0,
            updated: true,
            deleted: false,
        }
    }

    pub fn client_path(&self) -> &String { &self.client_path }
    pub fn download_path(&self) -> &String { &self.download_path }

    // Returns the path to the source file on the server
    pub fn source_path(&self, data_root: &Path) -> PathBuf {
        let native_path = path_utils::to_native(&self.download_path);
        let src_path = data_root.join(native_path);
        if self.is_compressed() && src_path.extension() == Some(OsStr::new("gz")) {
            // The original source file is uncompressed at the same path.
            src_path.with_extension("")
        } else {
            src_path
        }
    }

    pub fn is_compressed(&self) -> bool { (self.flags & Self::COMPRESSED_GZ) != 0 }

    pub fn add_flags(&mut self, flags: u32) { self.flags |= flags; }
    pub fn set_redist_update(&mut self) { self.add_flags(Self::REDIST_UPDATE); }

    pub fn ogg_flags(sound_buffer: &SoundBuffer) -> u32 {
        let mut flags = 0;
        if sound_buffer.split_channel() {
            flags |= Self::OGG_SPLIT_CHANNELS;
        } else {
            flags |= Self::OGG_STEREO;
        }
        if sound_buffer.stream_compressed() {
            flags |= Self::OGG_STREAM_COMPRESSED;
        }
        flags
    }

    pub fn update(&mut self, data_root: &Path) -> Result<()> {
        let src_path = self.source_path(data_root);

        let updated_file_hash = md5_hash_file(&src_path)?;
        let src_metadata = src_path.metadata()?;
        if src_metadata.len() != u64::from(self.file_size)
            || updated_file_hash != self.file_hash
        {
            // The source file has changed (or this is the first time we're
            // updating it), so we need to update the other properties as well.
            debug!("Updating {}", src_path.display());
            self.file_hash = updated_file_hash;
            self.file_size = u32::try_from(src_metadata.len())
                    .context("Source file is too large")?;

            // Try compressing the file.  If we don't get at least 10% savings,
            // it's not worth compressing and we should send it uncompressed.
            // This will generally be the case for encrypted files and ogg
            // files (which are already compressed in their own way)
            let gz_path = path_utils::append_extension(&src_path, "gz");
            {
                let mut gz_stream = GzEncoder::new(File::create(&gz_path)?,
                                                   flate2::Compression::default());
                let mut src_file = File::open(&src_path)?;
                std::io::copy(&mut src_file, &mut gz_stream)?;
                gz_stream.flush()?;
            }
            let gz_metadata = gz_path.metadata()?;
            if gz_metadata.len() < (u64::from(self.file_size) * 9) / 10 {
                // Compressed stream is small enough -- keep it and update
                // the manifest cache to reference it.
                self.download_path = path_utils::to_windows(
                        &gz_path.strip_prefix(data_root)
                        .with_context(|| format!("Path '{}' is not in the data root", gz_path.display()))?
                        .to_string_lossy());
                self.download_hash = md5_hash_file(&gz_path)?;
                // Already verified to be less than the (checked) size of the
                // source file.
                #[allow(clippy::cast_possible_truncation)] {
                    self.download_size = gz_metadata.len() as u32;
                }
                self.flags |= Self::COMPRESSED_GZ;
            } else {
                // Keep the file uncompressed.  The download hash and size will
                // match the hash and size of the destination file.
                self.download_path = path_utils::to_windows(
                        &src_path.strip_prefix(data_root)
                        .with_context(|| format!("Path '{}' is not in the data root", src_path.display()))?
                        .to_string_lossy());
                self.download_hash = self.file_hash;
                self.download_size = self.file_size;
                self.flags &= !Self::COMPRESSED_GZ;
                std::fs::remove_file(gz_path)?;
            }
            self.updated = true;
        }

        Ok(())
    }

    // Use this to indicate that the source file was deleted
    pub fn mark_deleted(&mut self) {
        self.deleted = true;
        self.updated = true;
    }

    pub fn as_ds_mfs(&self) -> String {
        format!("{},{},{},{},{},{},{}", self.client_path, self.download_path,
                HEXLOWER.encode(&self.file_hash), HEXLOWER.encode(&self.download_hash),
                self.file_size, self.download_size, self.flags)
    }
}

pub fn read_utf16z_text<S>(stream: &mut S) -> Result<String>
    where S: BufRead
{
    let mut buffer = Vec::new();
    loop {
        let ch = stream.read_u16::<LittleEndian>()?;
        if ch == 0 {
            break;
        }
        buffer.push(ch);
    }
    Ok(String::from_utf16_lossy(buffer.as_slice()))
}

pub fn read_utf16z_md5_hash<S>(stream: &mut S) -> Result<[u8; 16]>
    where S: BufRead
{
    // Convert UTF-16 hex to a binary hash
    let mut buffer = [0; 32];
    stream.read_u16_into::<LittleEndian>(&mut buffer)?;
    if stream.read_u16::<LittleEndian>()? != 0 {
        return Err(anyhow!("MD5 hash was not nul-terminated"));
    }
    let result = HEXLOWER_PERMISSIVE.decode(String::from_utf16_lossy(&buffer).as_bytes())
            .map_err(|err| anyhow!("Invalid hex literal: {}", err))?;
    result.try_into().map_err(|_| anyhow!("Invalid MD5 hash length"))
}

// Yes, it's as dumb as it sounds...
pub fn read_utf16z_u32<S>(stream: &mut S) -> Result<u32>
    where S: BufRead
{
    let value = u32::from(stream.read_u16::<LittleEndian>()?) << 16
              | u32::from(stream.read_u16::<LittleEndian>()?);
    if stream.read_u16::<LittleEndian>()? != 0 {
        return Err(anyhow!("uint32 value was not nul-terminated"));
    }
    Ok(value)
}

impl StreamRead for FileInfo {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let client_path = read_utf16z_text(stream)?;
        let download_path = read_utf16z_text(stream)?;
        let file_hash = read_utf16z_md5_hash(stream)?;
        let download_hash = read_utf16z_md5_hash(stream)?;
        let file_size = read_utf16z_u32(stream)?;
        let download_size = read_utf16z_u32(stream)?;
        let flags = read_utf16z_u32(stream)?;

        Ok(Self {
            client_path, download_path, file_hash, download_hash,
            file_size, download_size, flags, updated: false, deleted: false
        })
    }
}

pub fn write_utf16z_text(stream: &mut dyn Write, value: &str) -> io::Result<()> {
    for ch in value.encode_utf16() {
        stream.write_u16::<LittleEndian>(ch)?;
    }
    stream.write_u16::<LittleEndian>(0)
}

pub fn write_utf16z_md5_hash(stream: &mut dyn Write, value: &[u8; 16])
    -> io::Result<()>
{
    // Convert binary hash to a UTF-16 hex representation
    for ch in HEXLOWER.encode(value).encode_utf16() {
        stream.write_u16::<LittleEndian>(ch)?;
    }
    stream.write_u16::<LittleEndian>(0)
}

// Yes, it's as dumb as it sounds...
pub fn write_utf16z_u32(stream: &mut dyn Write, value: u32) -> io::Result<()> {
    stream.write_u16::<LittleEndian>((value >> 16) as u16)?;
    stream.write_u16::<LittleEndian>((value & 0xFFFF) as u16)?;
    stream.write_u16::<LittleEndian>(0)
}

impl StreamWrite for FileInfo {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        assert!(!self.deleted);

        write_utf16z_text(stream, &self.client_path)?;
        write_utf16z_text(stream, &self.download_path)?;
        write_utf16z_md5_hash(stream, &self.file_hash)?;
        write_utf16z_md5_hash(stream, &self.download_hash)?;
        write_utf16z_u32(stream, self.file_size)?;
        write_utf16z_u32(stream, self.download_size)?;
        write_utf16z_u32(stream, self.flags)?;

        Ok(())
    }
}

impl Manifest {
    const CACHE_MAGIC: u32 = 0x0153464d;    // 'MFS\x01'

    pub fn new() -> Self {
        Manifest { files: vec![] }
    }

    pub fn from_cache(path: &Path) -> Result<Self> {
        let mut stream = BufReader::new(File::open(path)?);
        let cache_magic = stream.read_u32::<LittleEndian>()?;
        if cache_magic != Self::CACHE_MAGIC {
            return Err(anyhow!("Unknown/invalid cache file magic '{:08x}'", cache_magic));
        }

        Manifest::stream_read(&mut stream)
    }

    pub fn write_cache(&self, path: &Path) -> Result<()> {
        let mut stream = BufWriter::new(File::create(path)?);
        stream.write_u32::<LittleEndian>(Self::CACHE_MAGIC)?;
        self.stream_write(&mut stream)?;
        Ok(stream.flush()?)
    }

    pub fn files(&self) -> &Vec<FileInfo> { &self.files }
    pub fn files_mut(&mut self) -> &mut Vec<FileInfo> { &mut self.files }
    pub fn add(&mut self, file: FileInfo) { self.files.push(file); }

    pub fn any_updated(&self) -> bool {
        self.files.iter().any(|f| f.updated)
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
            return Err(anyhow!("FileInfo array was not nul-terminated"));
        }

        Ok(Manifest { files })
    }
}

impl StreamWrite for Manifest {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        // Don't write deleted files.  We need to keep them around in the
        // cache though so the check for updated records still works properly.
        let write_files: Vec<&FileInfo> = self.files.iter().filter(|f| !f.deleted).collect();
        let num_files = u32::try_from(write_files.len()).context("Too many files for stream")?;
        stream.write_u32::<LittleEndian>(num_files)?;

        let mut file_stream = Cursor::new(Vec::new());
        for file in &write_files {
            file.stream_write(&mut file_stream)?;
        }
        file_stream.write_u16::<LittleEndian>(0)?;

        let file_buf = file_stream.into_inner();
        assert_eq!(file_buf.len() % size_of::<u16>(), 0);
        let entry_size = u32::try_from(file_buf.len() / size_of::<u16>())
                .context("Manifest entry too large for stream")?;
        stream.write_u32::<LittleEndian>(entry_size)?;
        Ok(stream.write_all(file_buf.as_slice())?)
    }
}
