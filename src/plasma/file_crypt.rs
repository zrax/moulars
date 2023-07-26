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
use std::io::{Read, BufRead, Write, Seek, SeekFrom, Result, ErrorKind};
use std::mem::size_of;
use std::path::Path;
use std::{mem, ptr};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use log::warn;

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum EncryptionType {
    // Plain text
    Unencrypted,
    // "BryceIsSmart" or "whatdoyousee" files -- Yes, these files use the
    // original broken TEA algorithm, not the "fixed" XTEA or XXTEA
    TEA,
    // "notthedroids" files
    XXTEA,
}

const TEA_BLOCK_SIZE: usize = 2;

impl EncryptionType {
    pub fn from_stream<S>(stream: &mut S) -> Result<Self>
        where S: Read + Seek
    {
        let start_pos = stream.stream_position()?;

        let mut buffer = [0; 12];
        if let Err(err) = stream.read_exact(&mut buffer) {
            if err.kind() == ErrorKind::UnexpectedEof {
                // The stream is too small for the encryption marker
                stream.seek(SeekFrom::Start(start_pos))?;
                return Ok(Self::Unencrypted);
            } else {
                return Err(err);
            }
        }
        if &buffer == b"whatdoyousee" || &buffer == b"BryceIsSmart" {
            Ok(Self::TEA)
        } else if &buffer == b"notthedroids" {
            Ok(Self::XXTEA)
        } else {
            stream.seek(SeekFrom::Start(start_pos))?;
            Ok(Self::Unencrypted)
        }
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let mut file = File::open(path)?;
        Self::from_stream(&mut file)
    }

    pub fn write_magic<S>(self, stream: &mut S) -> Result<()>
        where S: Write
    {
        match self {
            EncryptionType::Unencrypted => (),
            EncryptionType::TEA => {
                stream.write_all(b"whatdoyousee")?;
            }
            EncryptionType::XXTEA => {
                stream.write_all(b"notthedroids")?;
            }
        }
        Ok(())
    }
}

// Used for encrypted .age, .csv, .fni files
pub const DEFAULT_KEY: [u32; 4] = [0x6c0a5452, 0x03827d0f, 0x3a170b92, 0x16db7fc2];

pub struct EncryptedReader<S: BufRead> {
    base: S,
    encryption_type: EncryptionType,
    key: [u32; 4],
    buffer: [u8; 8],
    read_pos: usize,
    base_size: usize,
}

impl<S: BufRead + Seek> EncryptedReader<S> {
    pub fn new(mut base_stream: S, key: &[u32; 4]) -> Result<Self> {
        let encryption_type = EncryptionType::from_stream(&mut base_stream)?;
        let base_size = if encryption_type != EncryptionType::Unencrypted {
            base_stream.read_u32::<LittleEndian>()? as usize
        } else {
            0
        };

        Ok(Self {
            base: base_stream,
            encryption_type,
            key: *key,
            buffer: [0; 8],
            read_pos: 0,
            base_size,
        })
    }
}

impl<S: BufRead> EncryptedReader<S> {
    fn next_block(&mut self) -> Result<()> {
        let mut block = [0; TEA_BLOCK_SIZE];
        self.base.read_u32_into::<LittleEndian>(&mut block)?;
        match self.encryption_type {
            EncryptionType::Unencrypted => unreachable!(),
            EncryptionType::TEA => tea_decipher(&mut block, &self.key),
            EncryptionType::XXTEA => xxtea_decipher(&mut block, &self.key),
        }
        for (dest, src) in self.buffer.chunks_exact_mut(size_of::<u32>()).zip(block.iter()) {
            dest.copy_from_slice(&src.to_le_bytes());
        }
        Ok(())
    }

    pub fn into_inner(self) -> S {
        self.base
    }
}

impl<S: BufRead> Read for EncryptedReader<S> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.encryption_type == EncryptionType::Unencrypted {
            return self.base.read(buf);
        }

        let mut dest_pos = 0;
        while dest_pos < buf.len() {
            let buffer_pos = self.read_pos % self.buffer.len();
            let copy_len = (buf.len() - dest_pos).min(self.buffer.len() - buffer_pos)
                           .min(self.base_size - self.read_pos);
            if copy_len == 0 {
                break;
            }

            if buffer_pos == 0 {
                self.next_block()?;
            }
            buf[dest_pos..(dest_pos + copy_len)].copy_from_slice(
                    &self.buffer[buffer_pos..(buffer_pos + copy_len)]);
            dest_pos += copy_len;
            self.read_pos += copy_len;
        }
        Ok(dest_pos)
    }
}

// NOTE: Don't add any fields to this struct without also verifying
// `into_inner()` remains safe
pub struct EncryptedWriter<S: Write + Seek> {
    base: S,
    encryption_type: EncryptionType,
    key: [u32; 4],
    buffer: [u8; 8],
    write_pos: usize,
    base_size: usize,
    // The position of the size field in the file (updated on flush)
    size_pos: u64,
}

impl<S: Write + Seek> EncryptedWriter<S> {
    pub fn new(mut base_stream: S, encryption_type: EncryptionType, key: &[u32; 4])
        -> Result<Self>
    {
        encryption_type.write_magic(&mut base_stream)?;
        // Stream size -- This will be updated later
        let size_pos = base_stream.stream_position()?;
        base_stream.write_u32::<LittleEndian>(0)?;

        Ok(Self {
            base: base_stream,
            encryption_type,
            key: *key,
            buffer: [0; 8],
            write_pos: 0,
            base_size: 0,
            size_pos,
        })
    }

    fn write_block(&mut self) -> Result<()> {
        let mut block = [0; TEA_BLOCK_SIZE];
        for (src, dest) in self.buffer.chunks_exact(size_of::<u32>()).zip(block.iter_mut()) {
            *dest = u32::from_le_bytes(src.try_into().unwrap());
        }
        match self.encryption_type {
            EncryptionType::Unencrypted => unreachable!(),
            EncryptionType::TEA => tea_encipher(&mut block, &self.key),
            EncryptionType::XXTEA => xxtea_encipher(&mut block, &self.key),
        }
        for val in block {
            self.base.write_u32::<LittleEndian>(val)?;
        }
        Ok(())
    }

    pub fn into_inner(mut self) -> S {
        if let Err(err) = self.flush() {
            warn!("Failed to flush stream on into_inner: {}", err);
        }

        // SAFETY: This lets us move the base out of self without dropping it twice
        let inner = unsafe { ptr::read(&self.base) };
        mem::forget(self);

        inner
    }
}

impl<S: Write + Seek> Write for EncryptedWriter<S> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if self.encryption_type == EncryptionType::Unencrypted {
            return self.base.write(buf);
        }

        let mut src_pos = 0;
        while src_pos < buf.len() {
            let buffer_pos = self.write_pos % self.buffer.len();
            let copy_len = (buf.len() - src_pos).min(self.buffer.len() - buffer_pos);
            if copy_len == 0 {
                break;
            }

            self.buffer[buffer_pos..(buffer_pos + copy_len)].copy_from_slice(
                    &buf[src_pos..(src_pos + copy_len)]);
            src_pos += copy_len;
            self.write_pos += copy_len;
            if (self.write_pos % self.buffer.len()) == 0 {
                self.write_block()?;
            }
        }
        self.base_size = self.base_size.max(self.write_pos);
        Ok(src_pos)
    }

    fn flush(&mut self) -> Result<()> {
        if self.encryption_type == EncryptionType::Unencrypted {
            return self.base.flush();
        }
        let sync_pos = self.base.stream_position()?;
        self.write_block()?;
        self.base.seek(SeekFrom::Start(self.size_pos))?;
        self.base.write_u32::<LittleEndian>(self.base_size as u32)?;
        self.base.seek(SeekFrom::Start(sync_pos))?;
        Ok(())
    }
}

impl<S: Write + Seek> Drop for EncryptedWriter<S> {
    fn drop(&mut self) {
        if let Err(err) = self.flush() {
            warn!("Failed to flush stream on drop: {}", err);
        }
    }
}

const TEA_DELTA: u32 = 0x9E3779B9;

fn tea_decipher(block: &mut [u32; 2], key: &[u32; 4]) {
    let mut y = block[0];
    let mut z = block[1];
    let mut sum: u32 = 0xC6EF3720;

    for _ in 0..32 {
        z = z.wrapping_sub(((y << 4) ^ (y >> 5)).wrapping_add(y)
            ^ sum.wrapping_add(key[((sum >> 11) & 3) as usize]));
        sum = sum.wrapping_sub(TEA_DELTA);
        y = y.wrapping_sub(((z << 4) ^ (z >> 5)).wrapping_add(z)
            ^ sum.wrapping_add(key[(sum & 3) as usize]));
    }

    block[0] = y;
    block[1] = z;
}

fn tea_encipher(block: &mut [u32; 2], key: &[u32; 4]) {
    let mut y = block[0];
    let mut z = block[1];
    let mut sum: u32 = 0;

    for _ in 0..32 {
        y = y.wrapping_add(((z << 4) ^ (z >> 5)).wrapping_add(z)
            ^ sum.wrapping_add(key[(sum & 3) as usize]));
        sum = sum.wrapping_add(TEA_DELTA);
        z = z.wrapping_add(((y << 4) ^ (y >> 5)).wrapping_add(y)
            ^ sum.wrapping_add(key[((sum >> 11) & 3) as usize]));
    }

    block[0] = y;
    block[1] = z;
}

macro_rules! mx {
    ($y:ident, $z:ident, $sum:ident, $p:expr, $e:ident, $key:expr) => ({
        (($z >> 5) ^ ($y << 2)).wrapping_add((($y >> 3) ^ ($z << 4)))
            ^ (($sum ^ $y).wrapping_add($key[($p & 3) ^ $e as usize] ^ $z))
    })
}

fn xxtea_decipher(block: &mut [u32], key: &[u32; 4]) {
    let mut y: u32 = block[0];
    let mut z: u32;     // = block[block.len() - 1];
    let q: u32 = 6 + (52 / block.len() as u32);
    let mut sum: u32 = q.wrapping_mul(TEA_DELTA);

    while sum > 0 {
        let e = (sum >> 2) & 3;
        let mut p = block.len() - 1;
        while p > 0 {
            z = block[p - 1];
            block[p] = block[p].wrapping_sub(mx!(y, z, sum, p, e, key));
            y = block[p];
            p -= 1;
        }
        z = block[block.len() - 1];
        block[0] = block[0].wrapping_sub(mx!(y, z, sum, p, e, key));
        y = block[0];
        sum = sum.wrapping_sub(TEA_DELTA);
    }
}

fn xxtea_encipher(block: &mut [u32; 2], key: &[u32; 4]) {
    let mut y: u32;     // = block[0];
    let mut z: u32 = block[block.len() - 1];
    let mut q: u32 = 6 + (52 / block.len() as u32);
    let mut sum: u32 = 0;

    while q > 0 {
        sum = sum.wrapping_add(TEA_DELTA);
        let e = (sum >> 2) & 3;
        let mut p = 0;
        while p < block.len() - 1 {
            y = block[p + 1];
            block[p] = block[p].wrapping_add(mx!(y, z, sum, p, e, key));
            z = block[p];
            p += 1;
        }
        y = block[0];
        block[block.len() - 1] = block[block.len() - 1].wrapping_add(mx!(y, z, sum, p, e, key));
        z = block[block.len() - 1];

        q -= 1;
    }
}
