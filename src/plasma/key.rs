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

use crate::plasma::{self, StreamRead, StreamWrite};

use std::io::{BufRead, Write, Result};
use std::sync::Arc;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

pub struct Key {
    data: Option<Arc<Uoid>>
}

pub struct Uoid {
    location: Location,
    load_mask: u8,
    obj_type: u16,
    obj_name: String,
    obj_id: u32,
    clone_id: u32,
    clone_player_id: u32,
}

pub struct Location {
    sequence: u32,
    flags: u16,
}

impl StreamRead for Key {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        if stream.read_u8()? != 0 {
            Ok(Key { data: Some(Arc::new(Uoid::stream_read(stream)?)) })
        } else {
            Ok(Key { data: None })
        }
    }
}

impl StreamWrite for Key {
    fn stream_write<S>(&self, stream: &mut S) -> Result<()>
        where S: Write
    {
        if let Some(uoid) = &self.data {
            stream.write_u8(1)?;
            uoid.stream_write(stream)?;
        } else {
            stream.write_u8(0)?;
        }
        Ok(())
    }
}

impl Uoid {
    const HAS_CLONE_IDS: u8 = 1 << 0;
    const HAS_LOAD_MASK: u8 = 1 << 1;
}

impl StreamRead for Uoid {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let contents = stream.read_u8()?;
        let location = Location::stream_read(stream)?;
        let load_mask = if (contents & Uoid::HAS_LOAD_MASK) != 0 {
            stream.read_u8()?
        } else {
            0xFF
        };
        let obj_type = stream.read_u16::<LittleEndian>()?;
        let obj_id = stream.read_u32::<LittleEndian>()?;
        let obj_name = plasma::read_safe_str(stream, plasma::StringFormat::Latin1)?;
        let clone_id = if (contents & Uoid::HAS_CLONE_IDS) != 0 {
            stream.read_u32::<LittleEndian>()?
        } else {
            0
        };
        let clone_player_id = if (contents & Uoid::HAS_CLONE_IDS) != 0 {
            stream.read_u32::<LittleEndian>()?
        } else {
            0
        };

        Ok(Self {
            location, load_mask, obj_type, obj_id, obj_name,
            clone_id, clone_player_id
        })
    }
}

impl StreamWrite for Uoid {
    fn stream_write<S>(&self, stream: &mut S) -> Result<()>
        where S: Write
    {
        let mut contents = 0;
        if self.load_mask != 0xFF {
            contents |= Uoid::HAS_LOAD_MASK;
        }
        if self.clone_id != 0 || self.clone_player_id != 0 {
            contents |= Uoid::HAS_CLONE_IDS;
        }
        stream.write_u8(contents)?;

        self.location.stream_write(stream)?;
        if (contents & Uoid::HAS_LOAD_MASK) != 0 {
            stream.write_u8(self.load_mask)?;
        }
        stream.write_u16::<LittleEndian>(self.obj_type)?;
        stream.write_u32::<LittleEndian>(self.obj_id)?;
        plasma::write_safe_str(stream, &self.obj_name, plasma::StringFormat::Latin1)?;
        if (contents & Uoid::HAS_CLONE_IDS) != 0 {
            stream.write_u32::<LittleEndian>(self.clone_id)?;
            stream.write_u32::<LittleEndian>(self.clone_player_id)?;
        }

        Ok(())
    }
}

impl Location {
    pub fn new(sequence: u32, flags: u16) -> Self {
        Self { sequence, flags }
    }

    pub fn make(prefix: i32, page: i32, flags: u16) -> Self {
        if prefix < 0 {
            Self { sequence: (page & 0xFFFF).wrapping_sub(prefix << 16) as u32 + 0xFF000001, flags }
        } else {
            Self { sequence: (page & 0xFFFF).wrapping_add(prefix << 16) as u32 + 0x00000021, flags }
        }
    }
}

impl StreamRead for Location {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let sequence = stream.read_u32::<LittleEndian>()?;
        let flags = stream.read_u16::<LittleEndian>()?;
        Ok(Self { sequence, flags })
    }
}

impl StreamWrite for Location {
    fn stream_write<S>(&self, stream: &mut S) -> Result<()>
        where S: Write
    {
        stream.write_u32::<LittleEndian>(self.sequence)?;
        stream.write_u16::<LittleEndian>(self.flags)?;
        Ok(())
    }
}
