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

use std::io::{BufRead, Write};

use anyhow::Result;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::plasma::{StreamRead, StreamWrite};
use crate::plasma::safe_string::{ReadSafeStr, WriteSafeStr, StringFormat};

pub struct Key {
    data: Option<Uoid>
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Uoid {
    location: Location,
    load_mask: u8,
    obj_type: u16,
    obj_name: String,
    obj_id: u32,
    clone_id: u32,
    clone_player_id: u32,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Location {
    sequence: u32,
    flags: u16,
}

impl StreamRead for Key {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        if stream.read_u8()? != 0 {
            Ok(Key { data: Some(Uoid::stream_read(stream)?) })
        } else {
            Ok(Key { data: None })
        }
    }
}

impl StreamWrite for Key {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
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

    pub fn invalid() -> Self {
        Self {
            location: Location::invalid(),
            load_mask: 0xff,
            obj_type: 0,
            obj_name: String::new(),
            obj_id: 0,
            clone_id: 0,
            clone_player_id: 0,
        }
    }

    pub fn obj_type(&self) -> u16 { self.obj_type }
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
        let obj_name = stream.read_safe_str(StringFormat::Latin1)?;
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
            location, load_mask, obj_type, obj_name, obj_id,
            clone_id, clone_player_id
        })
    }
}

impl StreamWrite for Uoid {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
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
        stream.write_safe_str(&self.obj_name, StringFormat::Latin1)?;
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

    pub fn invalid() -> Self {
        Self { sequence: 0xFFFFFFFF, flags: 0}
    }

    pub fn make(prefix: i32, page: i32, flags: u16) -> Self {
        if prefix < 0 {
            #[allow(clippy::cast_sign_loss)]
            Self { sequence: (page & 0xFFFF).wrapping_sub(prefix << 16) as u32 + 0xFF000001, flags }
        } else {
            #[allow(clippy::cast_sign_loss)]
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
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        stream.write_u32::<LittleEndian>(self.sequence)?;
        stream.write_u16::<LittleEndian>(self.flags)?;
        Ok(())
    }
}

#[test]
fn test_location() {
    // Yes, there are multiple ways of encoding the same sequence...
    assert_eq!(Location::make(0, 0, 0).sequence, 0x00000021);
    assert_eq!(Location::make(1, 0, 0).sequence, 0x00010021);
    assert_eq!(Location::make(100, 1, 0).sequence, 0x00640022);
    assert_eq!(Location::make(65535, 65502, 0).sequence, 0xFFFFFFFF);

    assert_eq!(Location::make(1, -1, 0).sequence, 0x00020020);
    assert_eq!(Location::make(100, -33, 0).sequence, 0x00650000);
    assert_eq!(Location::make(65534, -33, 0).sequence, 0xFFFF0000);

    assert_eq!(Location::make(-1, 0, 0).sequence, 0xFF010001);
    assert_eq!(Location::make(-100, 1, 0).sequence, 0xFF640002);
    assert_eq!(Location::make(-255, 65534, 0).sequence, 0xFFFFFFFF);

    assert_eq!(Location::make(-1, -1, 0).sequence, 0xFF020000);
    assert_eq!(Location::make(-254, -1, 0).sequence, 0xFFFF0000);

    // Wrap around -- not actually valid...
    assert_eq!(Location::make(65537, 0, 0).sequence, 0x00010021);
    assert_eq!(Location::make(65536, -1, 0).sequence, 0x00010020);
    assert_eq!(Location::make(65536, -33, 0).sequence, 0x00010000);

    assert_eq!(Location::make(1, 65503, 0).sequence, 0x00020000);
    assert_eq!(Location::make(1, -34, 0).sequence, 0x0001FFFF);
    assert_eq!(Location::make(-255, -2, 0).sequence, 0xFFFFFFFF);
}
