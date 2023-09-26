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
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

use crate::general_error;
use crate::plasma::{Creatable, StreamRead, StreamWrite};
use crate::plasma::creatable::ClassID;
use crate::plasma::safe_string::{read_safe_str, write_safe_str, StringFormat};

pub enum GenericType {
    Int(i32),
    Float(f32),
    Bool(bool),
    String(String),
    Char(u8),
    Any(String),
    UInt(u32),
    Double(f64),
    None,
}

pub struct CreatableGenericValue {
    value: GenericType,
}

impl Creatable for CreatableGenericValue {
    fn class_id(&self) -> u16 { ClassID::CreatableGenericValue as u16 }
    fn static_class_id() -> u16 { ClassID::CreatableGenericValue as u16 }
    fn as_creatable(&self) -> &dyn Creatable { self }
}

#[repr(u8)]
#[derive(FromPrimitive)]
enum TypeID
{
    Int = 0, Float, Bool, String, Char, Any, UInt, Double,
    None = 0xFF,
}

impl StreamRead for CreatableGenericValue {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let type_id = stream.read_u8()?;
        match TypeID::from_u8(type_id) {
            Some(TypeID::Int) => {
                let value = stream.read_i32::<LittleEndian>()?;
                Ok(Self { value: GenericType::Int(value) })
            }
            Some(TypeID::Float) => {
                let value = stream.read_f32::<LittleEndian>()?;
                Ok(Self { value: GenericType::Float(value) })
            }
            Some(TypeID::Bool) => {
                let value = stream.read_u8()? != 0;
                Ok(Self { value: GenericType::Bool(value) })
            }
            Some(TypeID::String) => {
                let value = read_safe_str(stream, StringFormat::Latin1)?;
                Ok(Self { value: GenericType::String(value) })
            }
            Some(TypeID::Char) => {
                let value = stream.read_u8()?;
                Ok(Self { value: GenericType::Char(value) })
            }
            Some(TypeID::Any) => {
                let value = read_safe_str(stream, StringFormat::Latin1)?;
                Ok(Self { value: GenericType::Any(value) })
            }
            Some(TypeID::UInt) => {
                let value = stream.read_u32::<LittleEndian>()?;
                Ok(Self { value: GenericType::UInt(value) })
            }
            Some(TypeID::Double) => {
                let value = stream.read_f64::<LittleEndian>()?;
                Ok(Self { value: GenericType::Double(value) })
            }
            Some(TypeID::None) => Ok(Self { value: GenericType::None }),
            None => Err(general_error!("Invalid type ID {}", type_id))
        }
    }
}

impl StreamWrite for CreatableGenericValue {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        match &self.value {
            GenericType::Int(value) => {
                stream.write_u8(TypeID::Int as u8)?;
                stream.write_i32::<LittleEndian>(*value)?;
            }
            GenericType::Float(value) => {
                stream.write_u8(TypeID::Float as u8)?;
                stream.write_f32::<LittleEndian>(*value)?;
            }
            GenericType::Bool(value) => {
                stream.write_u8(TypeID::Bool as u8)?;
                stream.write_u8(u8::from(*value))?;
            }
            GenericType::String(value) => {
                stream.write_u8(TypeID::String as u8)?;
                write_safe_str(stream, value, StringFormat::Latin1)?;
            }
            GenericType::Char(value) => {
                stream.write_u8(TypeID::Char as u8)?;
                stream.write_u8(*value)?;
            }
            GenericType::Any(value) => {
                stream.write_u8(TypeID::Any as u8)?;
                write_safe_str(stream, value, StringFormat::Latin1)?;
            }
            GenericType::UInt(value) => {
                stream.write_u8(TypeID::UInt as u8)?;
                stream.write_u32::<LittleEndian>(*value)?;
            }
            GenericType::Double(value) => {
                stream.write_u8(TypeID::Double as u8)?;
                stream.write_f64::<LittleEndian>(*value)?;
            }
            GenericType::None => {
                stream.write_u8(TypeID::None as u8)?;
            }
        }
        Ok(())
    }
}
