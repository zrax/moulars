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

use std::io::Write;

use anyhow::Result;
use byteorder::{LittleEndian, WriteBytesExt};

use crate::plasma::StreamWrite;

#[allow(clippy::struct_field_names)]
#[derive(Eq, PartialEq, Hash, Copy, Clone, Debug)]
pub struct NodeRef {
    parent_id: u32,
    child_id: u32,
    owner_id: u32,
}

impl NodeRef {
    pub fn new(parent_id: u32, child_id: u32, owner_id: u32) -> Self {
        Self { parent_id, child_id, owner_id }
    }

    pub fn parent(&self) -> u32 { self.parent_id }
    pub fn child(&self) -> u32 { self.child_id }
    pub fn owner(&self) -> u32 { self.owner_id }
}

impl StreamWrite for NodeRef {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        stream.write_u32::<LittleEndian>(self.parent_id)?;
        stream.write_u32::<LittleEndian>(self.child_id)?;
        stream.write_u32::<LittleEndian>(self.owner_id)?;
        stream.write_u8(0)?;    // Seen -- never used
        Ok(())
    }
}
