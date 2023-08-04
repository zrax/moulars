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

use std::io::{Cursor, BufRead, Write, Result};
use std::sync::Arc;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use log::warn;

use crate::general_error;
use crate::plasma::{Uoid, StreamRead, StreamWrite};
use crate::plasma::safe_string::{read_safe_str, write_safe_str, StringFormat};
use super::{DescriptorDb, StateDescriptor, VarType, Variable};
use super::{HAS_UOID, VAR_LENGTH_IO};

#[derive(Debug)]
pub struct State {
    descriptor: Arc<StateDescriptor>,
    simple_vars: Vec<Variable>,
    statedesc_vars: Vec<Variable>,
    object: Option<Uoid>,
    flags: u16,
}

impl State {
    const IO_VERSION: u8 = 6;

    pub fn from_defaults(descriptor: Arc<StateDescriptor>, db: &DescriptorDb) -> Self {
        let mut simple_vars = Vec::with_capacity(descriptor.vars().len());
        let mut statedesc_vars = Vec::with_capacity(descriptor.vars().len());
        for var_desc in descriptor.vars() {
            let var = Variable::from_defaults(var_desc.clone(), db);
            if let VarType::StateDesc(_) = var_desc.var_type() {
                statedesc_vars.push(var);
            } else {
                simple_vars.push(var);
            }
        }
        Self { descriptor, simple_vars, statedesc_vars, object: None, flags: 0 }
    }

    pub fn is_default(&self) -> bool {
        self.simple_vars.iter().all(|var| var.is_default())
            && self.statedesc_vars.iter().all(|var| var.is_default())
    }

    pub fn is_dirty(&self) -> bool {
        self.simple_vars.iter().any(|var| var.is_dirty())
            || self.statedesc_vars.iter().any(|var| var.is_dirty())
    }

    pub fn read<S>(&mut self, stream: &mut S, db: &DescriptorDb) -> Result<()>
        where S: BufRead
    {
        self.flags = stream.read_u16::<LittleEndian>()?;
        let io_version = stream.read_u8()?;
        if io_version != Self::IO_VERSION {
            return Err(general_error!("Unexpected IO version {}", io_version));
        }

        let max_hint = self.descriptor.vars().len();

        let count = read_compressed_size(stream, max_hint)?;
        let read_indices = count != self.simple_vars.len();
        for idx in 0..count {
            let idx = if read_indices {
                read_compressed_size(stream, max_hint)?
            } else {
                idx
            };
            if idx >= self.simple_vars.len() {
                return Err(general_error!("Invalid variable index {}", idx));
            }
            self.simple_vars[idx].read(stream, db)?;
        }

        let count = read_compressed_size(stream, max_hint)?;
        let read_indices = count != self.statedesc_vars.len();
        for idx in 0..count {
            let idx = if read_indices {
                read_compressed_size(stream, max_hint)?
            } else {
                idx
            };
            if idx >= self.statedesc_vars.len() {
                return Err(general_error!("Invalid variable index {}", idx));
            }
            self.statedesc_vars[idx].read(stream, db)?;
        }

        Ok(())
    }

    pub fn write(&self, stream: &mut dyn Write) -> Result<()> {
        stream.write_u16::<LittleEndian>(self.flags)?;
        stream.write_u8(Self::IO_VERSION)?;

        let max_hint = self.descriptor.vars().len();

        let count = self.simple_vars.iter().filter(|var| var.is_dirty()).count();
        write_compressed_size(stream, max_hint, count)?;
        let write_indices = count != self.simple_vars.len();
        for (idx, var) in self.simple_vars.iter().enumerate() {
            if var.is_dirty() {
                if write_indices {
                    write_compressed_size(stream, max_hint, idx)?;
                }
                var.write(stream)?;
            }
        }

        let count = self.statedesc_vars.iter().filter(|var| var.is_dirty()).count();
        write_compressed_size(stream, max_hint, count)?;
        let write_indices = count != self.statedesc_vars.len();
        for (idx, var) in self.statedesc_vars.iter().enumerate() {
            if var.is_dirty() {
                if write_indices {
                    write_compressed_size(stream, max_hint, idx)?;
                }
                var.write(stream)?;
            }
        }

        Ok(())
    }

    pub fn from_blob(blob: &Vec<u8>, db: &DescriptorDb) -> Result<Self> {
        let mut stream = Cursor::new(blob);
        let read_flags = stream.read_u16::<LittleEndian>()?;
        if (read_flags & VAR_LENGTH_IO) == 0 {
            return Err(general_error!("Unsupported blob format"));
        }

        let descriptor_name = read_safe_str(&mut stream, StringFormat::Latin1)?;
        let version = stream.read_u16::<LittleEndian>()?;
        if let Some(descriptor) = db.get_version(&descriptor_name, version as u32) {
            let mut state = State::from_defaults(descriptor, db);
            if (read_flags & HAS_UOID) != 0 {
                state.object = Some(Uoid::stream_read(&mut stream)?);
            }
            state.read(&mut stream, db)?;
            if stream.position() as usize != stream.get_ref().len() {
                warn!("Did not fully parse SDL blob! ({} of {} bytes read)",
                      stream.position(), stream.get_ref().len());
            }
            Ok(state)
        } else {
            Err(general_error!("Could not find descriptor {} version {}",
                descriptor_name, version))
        }
    }

    pub fn to_blob(&self) -> Result<Vec<u8>> {
        let mut stream = Cursor::new(Vec::new());
        let mut write_flags = VAR_LENGTH_IO;
        if self.object.is_some() {
            write_flags |= HAS_UOID;
        }
        stream.write_u16::<LittleEndian>(write_flags)?;

        write_safe_str(&mut stream, self.descriptor.name(), StringFormat::Latin1)?;
        stream.write_u16::<LittleEndian>(self.descriptor.version() as u16)?;
        if let Some(uoid) = &self.object {
            uoid.stream_write(&mut stream)?;
        }
        self.write(&mut stream)?;

        Ok(stream.into_inner())
    }
}

pub(super) fn read_compressed_size<S>(stream: &mut S, max_hint: usize) -> Result<usize>
    where S: BufRead
{
    if max_hint < 0x100 {
        Ok(stream.read_u8()? as usize)
    } else if max_hint < 0x10000 {
        Ok(stream.read_u16::<LittleEndian>()? as usize)
    } else {
        Ok(stream.read_u32::<LittleEndian>()? as usize)
    }
}

pub(super) fn write_compressed_size(stream: &mut dyn Write, max_hint: usize, value: usize)
    -> Result<()>
{
    if value > u32::MAX as usize {
        return Err(general_error!("Size {} is too large for stream", value));
    }
    if max_hint < 0x100 {
        stream.write_u8(value as u8)
    } else if max_hint < 0x10000 {
        stream.write_u16::<LittleEndian>(value as u16)
    } else {
        stream.write_u32::<LittleEndian>(value as u32)
    }
}
