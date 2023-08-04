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

use std::io::{BufRead, Write, Result};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use num_traits::FromPrimitive;

use crate::general_error;
use crate::plasma::StreamRead;
use super::creatable::{Creatable, ClassID};
use super::messages::MessageInterface;

// Just used for namespace familiarity...
pub struct Factory { }

impl Factory {
    pub fn read_creatable<S>(stream: &mut S) -> Result<Option<Box<dyn Creatable>>>
        where S: BufRead
    {
        let class_id = stream.read_u16::<LittleEndian>()?;
        Self::read_creatable_as(stream, class_id)
    }

    pub fn read_creatable_as<S>(stream: &mut S, class_id: u16)
            -> Result<Option<Box<dyn Creatable>>>
        where S: BufRead
    {
        use super::net_common::CreatableGenericValue;
        use super::messages::MessageWithCallbacks;

        match ClassID::from_u16(class_id) {
            Some(ClassID::SoundBuffer) =>
                Err(general_error!("SoundBuffer only supported for Manifest generation")),
            Some(ClassID::RelevanceRegion) =>
                Err(general_error!("RelevanceRegion only supported for Manifest generation")),
            Some(ClassID::MessageWithCallbacks) =>
                Ok(Some(Box::new(MessageWithCallbacks::stream_read(stream)?))),
            Some(ClassID::CreatableGenericValue) =>
                Ok(Some(Box::new(CreatableGenericValue::stream_read(stream)?))),
            Some(ClassID::Nil) => Ok(None),
            None => Err(general_error!("Unknown creatable type 0x{:04x}", class_id)),
        }
    }

    pub fn read_message<S>(stream: &mut S) -> Result<Option<Box<dyn MessageInterface>>>
        where S: BufRead
    {
        if let Some(creatable) = Self::read_creatable(stream)? {
            let msg_type = creatable.class_id();
            if let Some(msg) = creatable.as_message() {
                Ok(Some(msg))
            } else {
                Err(general_error!("Unexpected creatable type 0x{:04x} (expected Message)",
                    msg_type))
            }
        } else {
            Ok(None)
        }
    }

    pub fn write_creatable(stream: &mut dyn Write,
                           creatable: Option<&dyn Creatable>) -> Result<()>
    {
        if let Some(creatable) = creatable {
            stream.write_u16::<LittleEndian>(creatable.class_id())?;
            creatable.stream_write(stream)?;
        } else {
            stream.write_u16::<LittleEndian>(ClassID::Nil as u16)?;
        }
        Ok(())
    }
}
