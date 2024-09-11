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

use std::io::{BufRead, BufReader, BufWriter, Read, Write};

use anyhow::{anyhow, Context, Result};
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;

use crate::plasma::{BitVector, Creatable, StreamRead, StreamWrite};
use crate::plasma::Factory;
use crate::plasma::creatable::derive_creatable;
use super::{Message, NetSafety};

pub struct LinkingMgrMsg {
    base: Message,
    content_flags: BitVector,
    cmd: u8,
    args: CreatableList,
}

impl LinkingMgrMsg {
    // Content Flags
    const HAVE_COMMAND: usize = 0;
    const HAVE_ARGS: usize = 1;
}

derive_creatable!(LinkingMgrMsg, NetSafety, (Message));

impl StreamRead for LinkingMgrMsg {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let base = Message::stream_read(stream)?;
        let content_flags = BitVector::stream_read(stream)?;
        let cmd = if content_flags.get(Self::HAVE_COMMAND) {
            stream.read_u8()?
        } else {
            0
        };
        let args = if content_flags.get(Self::HAVE_ARGS) {
            CreatableList::stream_read(stream)?
        } else {
            CreatableList::default()
        };

        Ok(Self { base, content_flags, cmd, args })
    }
}

impl StreamWrite for LinkingMgrMsg {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        self.base.stream_write(stream)?;
        self.content_flags.stream_write(stream)?;
        if self.content_flags.get(Self::HAVE_COMMAND) {
            stream.write_u8(self.cmd)?;
        }
        if self.content_flags.get(Self::HAVE_ARGS) {
            self.args.stream_write(stream)?;
        }

        Ok(())
    }
}

impl NetSafety for LinkingMgrMsg {
    fn make_net_safe(&mut self) -> bool { false }
}

struct CreatableList {
    flags: u8,
    // Technically this should be a BTreeMap, but we don't care about adding
    // or removing items, so this keeps the order as provided by the sender.
    items: Vec<(u16, Box<dyn Creatable>)>,
}

impl CreatableList {
    // Flags
    const WANT_COMPRESSION: u8 = 1 << 0;
    const COMPRESSED: u8 = 1 << 1;
    //const WRITTEN: u8 = 1 << 2;       // Only used in the client
}

impl Default for CreatableList {
    fn default() -> Self {
        Self { flags: Self::WANT_COMPRESSION, items: Vec::new() }
    }
}

impl StreamRead for CreatableList {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let mut flags = stream.read_u8()?;

        let buffer_size = stream.read_u32::<LittleEndian>()? as usize;
        let mut buffer = Vec::with_capacity(buffer_size);
        if flags & Self::COMPRESSED != 0 {
            let compressed_size = stream.read_u32::<LittleEndian>()? as usize;
            let mut compressed = vec![0; compressed_size];
            stream.read_exact(&mut compressed)?;
            let decompressed_size = ZlibDecoder::new(&compressed[..]).read_to_end(&mut buffer)?;
            if decompressed_size != buffer_size {
                return Err(anyhow!("Buffer decompressed size mismatch"));
            }
            flags &= !Self::COMPRESSED;
        } else {
            let read_size = stream.take(buffer_size as u64).read_to_end(&mut buffer)?;
            if read_size != buffer_size {
                return Err(anyhow!("Buffer read size mismatch"));
            }
        }

        let mut creatable_stream = BufReader::new(&buffer[..]);
        let item_count = creatable_stream.read_u16::<LittleEndian>()?;
        let mut items = Vec::with_capacity(item_count as usize);
        for _ in 0..item_count {
            let item_id = creatable_stream.read_u16::<LittleEndian>()?;
            if let Some(item) = Factory::read_creatable(&mut creatable_stream)? {
                items.push((item_id, item));
            } else {
                return Err(anyhow!("Unexpected null creatable in list"));
            }
        }

        Ok(Self { flags, items })
    }
}

const COMPRESS_THRESHOLD: usize = 256;

impl StreamWrite for CreatableList {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        let mut buffer = Vec::new();
        {
            let mut creatable_stream = BufWriter::new(&mut buffer);
            let item_count = u16::try_from(self.items.len()).context("Too many items for stream")?;
            creatable_stream.write_u16::<LittleEndian>(item_count)?;
            for (item_id, item) in &self.items {
                creatable_stream.write_u16::<LittleEndian>(*item_id)?;
                Factory::write_creatable(&mut creatable_stream, Some(item.as_ref()))?;
            }
        }
        let buffer_size = u32::try_from(buffer.len()).context("Buffer too large for stream")?;

        let mut flags = self.flags & !Self::COMPRESSED;
        if flags & Self::WANT_COMPRESSION != 0 && buffer.len() >= COMPRESS_THRESHOLD {
            let mut compressed = BufWriter::new(Vec::new());
            if ZlibEncoder::new(&mut compressed, Compression::default()).write_all(&buffer).is_ok() {
                let compressed = compressed.into_inner()?;
                if compressed.len() < buffer.len() {
                    buffer = compressed;
                    flags |= Self::COMPRESSED;
                }
            }
            // If compression failed, just write the uncompressed list...
        }

        stream.write_u8(flags)?;
        stream.write_u32::<LittleEndian>(buffer_size)?;
        if flags & Self::COMPRESSED != 0 {
            // This was already checked to be smaller than the uncompressed buffer
            let compressed_size = u32::try_from(buffer.len()).expect("Compressed buffer too large");
            stream.write_u32::<LittleEndian>(compressed_size)?;
        }
        stream.write_all(&buffer)?;

        Ok(())
    }
}
