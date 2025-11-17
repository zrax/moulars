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

use std::io::{Cursor, BufRead, Write};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use log::warn;

use crate::plasma::{Uoid, StreamRead, StreamWrite};
use crate::plasma::safe_string::{ReadSafeStr, WriteSafeStr, StringFormat};
use super::{DescriptorDb, StateDescriptor, VarType, Variable};
use super::{HAS_UOID, VAR_LENGTH_IO};

#[derive(Clone, Debug)]
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

    pub fn descriptor(&self) -> &StateDescriptor { &self.descriptor }

    pub fn is_default(&self) -> bool {
        self.simple_vars.iter().all(Variable::is_default)
            && self.statedesc_vars.iter().all(Variable::is_default)
    }

    pub fn is_dirty(&self) -> bool {
        self.simple_vars.iter().any(Variable::is_dirty)
            || self.statedesc_vars.iter().any(Variable::is_dirty)
    }

    pub fn read<S>(&mut self, stream: &mut S, db: &DescriptorDb) -> Result<()>
        where S: BufRead
    {
        self.flags = stream.read_u16::<LittleEndian>()?;
        let io_version = stream.read_u8()?;
        if io_version != Self::IO_VERSION {
            return Err(anyhow!("Unexpected IO version {io_version}"));
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
                return Err(anyhow!("Invalid variable index {idx}"));
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
                return Err(anyhow!("Invalid variable index {idx}"));
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

    pub fn from_blob(blob: &[u8], db: &DescriptorDb) -> Result<Self> {
        let mut stream = Cursor::new(blob);
        let read_flags = stream.read_u16::<LittleEndian>()?;
        if (read_flags & VAR_LENGTH_IO) == 0 {
            return Err(anyhow!("Unsupported blob format"));
        }

        let descriptor_name = stream.read_safe_str(StringFormat::Latin1)?;
        let version = stream.read_u16::<LittleEndian>()?;
        if let Some(descriptor) = db.get_version(&descriptor_name, version) {
            let mut state = State::from_defaults(descriptor, db);
            if (read_flags & HAS_UOID) != 0 {
                state.object = Some(Uoid::stream_read(&mut stream)?);
            }
            state.read(&mut stream, db)?;
            #[allow(clippy::cast_possible_truncation)]
            if stream.position() as usize != stream.get_ref().len() {
                warn!("Did not fully parse SDL blob! ({} of {} bytes read)",
                      stream.position(), stream.get_ref().len());
            }
            Ok(state)
        } else {
            Err(anyhow!("Could not find descriptor {descriptor_name} version {version}"))
        }
    }

    pub fn to_blob(&self) -> Result<Vec<u8>> {
        let mut stream = Cursor::new(Vec::new());
        let mut write_flags = VAR_LENGTH_IO;
        if self.object.is_some() {
            write_flags |= HAS_UOID;
        }
        stream.write_u16::<LittleEndian>(write_flags)?;

        stream.write_safe_str(self.descriptor.name(), StringFormat::Latin1)?;
        stream.write_u16::<LittleEndian>(self.descriptor.version())?;
        if let Some(uoid) = &self.object {
            uoid.stream_write(&mut stream)?;
        }
        self.write(&mut stream)?;

        Ok(stream.into_inner())
    }

    pub fn get_var(&self, var_name: &str) -> Option<&Variable> {
        self.simple_vars.iter().chain(self.statedesc_vars.iter())
                .find(|var| var.descriptor().name() == var_name)
    }

    pub fn get_var_mut(&mut self, var_name: &str) -> Option<&mut Variable> {
        self.simple_vars.iter_mut().chain(self.statedesc_vars.iter_mut())
                .find(|var| var.descriptor().name() == var_name)
    }

    pub fn upgrade(&self, db: &DescriptorDb) -> Option<Self> {
        let Some(new_desc) = db.get_latest(self.descriptor.name()) else {
            // This can't happen unless the descriptor db provided here is
            // different from the one we originally constructed this state with.
            panic!("Descriptor database is missing descriptors for {}",
                   self.descriptor.name());
        };
        if Arc::ptr_eq(&new_desc, &self.descriptor) {
            // Already at the latest version
            return None;
        }

        let mut new_state = Self::from_defaults(new_desc, db);
        for old_var in self.simple_vars.iter().chain(self.statedesc_vars.iter()) {
            if let Some(new_var) = new_state.get_var_mut(old_var.descriptor().name()) {
                new_var.upgrade_from(old_var, db);
            }
        }

        Some(new_state)
    }
}

pub(super) fn read_compressed_size<S>(stream: &mut S, max_hint: usize) -> Result<usize>
    where S: BufRead
{
    if max_hint < 0x100 {
        Ok(usize::from(stream.read_u8()?))
    } else if max_hint < 0x10000 {
        Ok(usize::from(stream.read_u16::<LittleEndian>()?))
    } else {
        let size = stream.read_u32::<LittleEndian>()?;
        Ok(usize::try_from(size)?)
    }
}

pub(super) fn write_compressed_size(stream: &mut dyn Write, max_hint: usize, value: usize)
    -> Result<()>
{
    if max_hint < 0x100 {
        if let Ok(value8) = u8::try_from(value) {
            return Ok(stream.write_u8(value8)?);
        }
    } else if max_hint < 0x10000 {
        if let Ok(value16) = u16::try_from(value) {
            return Ok(stream.write_u16::<LittleEndian>(value16)?);
        }
    } else if let Ok(value32) = u32::try_from(value) {
        return Ok(stream.write_u32::<LittleEndian>(value32)?);
    }
    Err(anyhow!("Size {value} is too large for stream"))
}

#[cfg(test)]
fn setup_test_state(db: &DescriptorDb) -> Result<State> {
    let desc = db.get_version("Test", 1).expect("Could not get StateDesc Test v1");
    let mut state = State::from_defaults(desc, db);
    assert!(state.is_default());
    assert!(!state.is_dirty());

    state.get_var_mut("bTestVar1")
            .expect("Could not find variable bTestVar1 in SDL state")
            .set_bool(0, true)?;
    state.get_var_mut("iTestVar3")
            .expect("Could not find variable iTestVar3 in SDL state")
            .set_int(0, 6)?;

    assert!(!state.is_default());
    assert!(state.is_dirty());

    Ok(state)
}

#[test]
fn test_blob_round_trip() -> Result<()> {
    use super::descriptor_db::TEST_DESCRIPTORS;

    let _ = env_logger::builder().is_test(true).filter_level(log::LevelFilter::Debug)
                .format_timestamp(None).format_target(false).try_init();

    let db = DescriptorDb::from_string(TEST_DESCRIPTORS)?;
    let orig_state = setup_test_state(&db)?;
    let orig_blob = orig_state.to_blob()?;
    let new_state = State::from_blob(&orig_blob, &db)?;
    let new_blob = new_state.to_blob()?;
    assert_eq!(orig_blob, new_blob);

    Ok(())
}

#[test]
fn test_sdl_upgrade() -> Result<()> {
    use super::descriptor_db::TEST_DESCRIPTORS;

    let _ = env_logger::builder().is_test(true).filter_level(log::LevelFilter::Debug)
                .format_timestamp(None).format_target(false).try_init();

    let db = DescriptorDb::from_string(TEST_DESCRIPTORS)?;
    let orig_state = setup_test_state(&db)?;
    let orig_var1 = orig_state.get_var("bTestVar1").expect("Failed to get bTestVar1 variable");
    let orig_var2 = orig_state.get_var("bTestVar2").expect("Failed to get bTestVar2 variable");
    let orig_var3 = orig_state.get_var("iTestVar3").expect("Failed to get iTestVar3 variable");
    let orig_var4 = orig_state.get_var("iTestVar4").expect("Failed to get iTestVar4 variable");
    assert!(!orig_var1.is_default());
    assert!(orig_var2.is_default());
    assert!(!orig_var3.is_default());
    assert!(orig_var4.is_default());

    let new_state = orig_state.upgrade(&db).expect("Upgrade didn't find a new version");
    let new_var1 = new_state.get_var("bTestVar1").expect("Failed to get bTestVar1 variable");
    let new_var2 = new_state.get_var("bTestVar2").expect("Failed to get bTestVar2 variable");
    let new_var3 = new_state.get_var("iTestVar3").expect("Failed to get iTestVar3 variable");
    let new_var4 = new_state.get_var("iTestVar4").expect("Failed to get iTestVar4 variable");
    let new_var5 = new_state.get_var("bTestVar5").expect("Failed to get bTestVar5 variable");
    assert_eq!(orig_var1.get_bool(0)?, new_var1.get_bool(0)?);
    assert_eq!(orig_var2.get_bool(0)?, new_var2.get_bool(0)?);
    assert_eq!(orig_var3.get_int(0)?, new_var3.get_int(0)?);
    assert_eq!(orig_var4.get_int(0)?, new_var4.get_int(0)?);
    assert!(new_var5.is_default());

    Ok(())
}
