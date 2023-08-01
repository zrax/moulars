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

use std::io::{BufRead, Result, Write};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::plasma::{Key, Creatable, StreamRead, StreamWrite};
use crate::plasma::{read_safe_str, write_safe_str, StringFormat};
use crate::plasma::creatable::ClassID;

pub struct SoundBuffer {
    key: Key,
    flags: u32,
    data_length: u32,
    file_name: String,
    wav_header: WavHeader,
}

struct WavHeader {
    format_tag: u16,
    num_channels: u16,
    samples_per_sec: u32,
    avg_bytes_per_sec: u32,
    block_align: u16,
    bits_per_sample: u16,
}

impl SoundBuffer {
    // Flags
    const IS_EXTERNAL: u32 = 1 << 0;
    #[allow(unused)]
    const ALWAYS_EXTERNAL: u32 = 1 << 1;
    const ONLY_LEFT_CHANNEL: u32 = 1 << 2;
    const ONLY_RIGHT_CHANNEL: u32 = 1 << 3;
    const STREAM_COMPRESSED: u32 = 1 << 4;

    pub fn is_external(&self) -> bool {
        (self.flags & Self::IS_EXTERNAL) != 0
    }

    pub fn split_channel(&self) -> bool {
        (self.flags & (Self::ONLY_LEFT_CHANNEL | Self::ONLY_RIGHT_CHANNEL)) != 0
    }

    pub fn stream_compressed(&self) -> bool {
        (self.flags & Self::STREAM_COMPRESSED) != 0
    }

    pub fn file_name(&self) -> &String { &self.file_name }
}

impl Creatable for SoundBuffer {
    fn class_id(&self) -> u16 { ClassID::SoundBuffer as u16 }
    fn static_class_id() -> u16 { ClassID::SoundBuffer as u16 }
    fn as_creatable(&self) -> &dyn Creatable { self }
}

impl StreamRead for SoundBuffer {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let key = Key::stream_read(stream)?;
        let flags = stream.read_u32::<LittleEndian>()?;
        let data_length = stream.read_u32::<LittleEndian>()?;
        let file_name = read_safe_str(stream, StringFormat::Utf8)?;
        let wav_header = WavHeader::stream_read(stream)?;
        Ok(Self { key, flags, data_length, file_name, wav_header })
    }
}

impl StreamWrite for SoundBuffer {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        self.key.stream_write(stream)?;
        stream.write_u32::<LittleEndian>(self.flags)?;
        stream.write_u32::<LittleEndian>(self.data_length)?;
        write_safe_str(stream, &self.file_name, StringFormat::Utf8)?;
        self.wav_header.stream_write(stream)?;
        Ok(())
    }
}

impl StreamRead for WavHeader {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let format_tag = stream.read_u16::<LittleEndian>()?;
        let num_channels = stream.read_u16::<LittleEndian>()?;
        let samples_per_sec = stream.read_u32::<LittleEndian>()?;
        let avg_bytes_per_sec = stream.read_u32::<LittleEndian>()?;
        let block_align = stream.read_u16::<LittleEndian>()?;
        let bits_per_sample = stream.read_u16::<LittleEndian>()?;

        Ok(Self { format_tag, num_channels, samples_per_sec, avg_bytes_per_sec,
                  block_align, bits_per_sample})
    }
}

impl StreamWrite for WavHeader {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        stream.write_u16::<LittleEndian>(self.format_tag)?;
        stream.write_u16::<LittleEndian>(self.num_channels)?;
        stream.write_u32::<LittleEndian>(self.samples_per_sec)?;
        stream.write_u32::<LittleEndian>(self.avg_bytes_per_sec)?;
        stream.write_u16::<LittleEndian>(self.block_align)?;
        stream.write_u16::<LittleEndian>(self.bits_per_sample)?;
        Ok(())
    }
}
