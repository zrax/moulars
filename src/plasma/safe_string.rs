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

use std::io::{BufRead, Write, Result, Error, ErrorKind};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

#[derive(Eq, PartialEq)]
pub enum StringFormat {
    Latin1, Utf8, Utf16,
}

pub fn read_safe_str<S>(stream: &mut S, format: StringFormat) -> Result<String>
    where S: BufRead
{
    let length = stream.read_u16::<LittleEndian>()?;
    if (length & 0xF000) != 0 {
        // Discarded -- old format
        let _ = stream.read_u16::<LittleEndian>()?;
    }

    if format == StringFormat::Utf16 {
        let mut buffer = vec![0u16; (length & 0x0FFF) as usize];
        stream.read_u16_into::<LittleEndian>(buffer.as_mut_slice())?;
        let _ = stream.read_u16::<LittleEndian>()?;     // Trailing '\0'
        if let Some(&first_char) = buffer.first() {
            if (first_char & 0x8000) != 0 {
                for ch in &mut buffer {
                    *ch = !*ch;
                }
            }
        }
        Ok(String::from_utf16_lossy(buffer.as_slice()))
    } else {
        let mut buffer = vec![0u8; (length & 0x0FFF) as usize];
        stream.read_exact(buffer.as_mut_slice())?;
        if let Some(&first_char) = buffer.first() {
            if (first_char & 0x80) != 0 {
                for ch in &mut buffer {
                    *ch = !*ch;
                }
            }
        }
        if format == StringFormat::Utf8 {
            Ok(String::from_utf8_lossy(buffer.as_slice()).into())
        } else {
            // This performs a conversion from Latin-1
            Ok(buffer.iter().map(|&ch| ch as char).collect())
        }
    }
}

pub fn write_safe_str<S>(stream: &mut S, value: &str, format: StringFormat) -> Result<()>
    where S: Write
{
    if format == StringFormat::Utf16 {
        let buffer: Vec<u16> = value.encode_utf16().collect();
        let length_key = buffer.len();
        if length_key > 0x0FFF {
            return Err(Error::new(ErrorKind::Other, "String too large for SafeString encoding"));
        }
        stream.write_u16::<LittleEndian>(length_key as u16 | 0xF000)?;
        for ch in buffer {
            stream.write_u16::<LittleEndian>(!ch)?;
        }
        stream.write_u16::<LittleEndian>(0)?;   // Trailing '\0'
    } else {
        let buffer: Vec<u8> = if format == StringFormat::Utf8 {
            value.as_bytes().iter().map(|&ch| !ch).collect()
        } else {
            value.chars().map(|ch| if (ch as u32) < 0x100 { !(ch as u8) } else { !b'?' }).collect()
        };
        let length_key = buffer.len();
        if length_key > 0x0FFF {
            return Err(Error::new(ErrorKind::Other, "String too large for SafeString encoding"));
        }
        stream.write_u16::<LittleEndian>(length_key as u16 | 0xF000)?;
        for ch in buffer {
            stream.write_u8(!ch)?;
        }
    }

    Ok(())
}
