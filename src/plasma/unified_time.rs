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

use crate::plasma::{StreamRead, StreamWrite};

#[derive(Debug, Default)]
pub struct UnifiedTime {
    secs: u32,
    micros: u32,
}

impl StreamRead for UnifiedTime {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let secs = stream.read_u32::<LittleEndian>()?;
        let micros = stream.read_u32::<LittleEndian>()?;
        Ok(Self { secs, micros })
    }
}

impl StreamWrite for UnifiedTime {
    fn stream_write<S>(&self, stream: &mut S) -> Result<()>
        where S: Write
    {
        stream.write_u32::<LittleEndian>(self.secs)?;
        stream.write_u32::<LittleEndian>(self.micros)?;
        Ok(())
    }
}
