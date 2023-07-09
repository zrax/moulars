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
use uuid::Uuid;

pub trait StreamRead {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead, Self: Sized;
}

pub trait StreamWrite {
    fn stream_write<S>(&self, stream: &mut S) -> Result<()>
        where S: Write;
}

impl StreamRead for Uuid {
    fn stream_read<S>(stream: &mut S) -> Result<Uuid>
        where S: BufRead
    {
        let data1 = stream.read_u32::<LittleEndian>()?;
        let data2 = stream.read_u16::<LittleEndian>()?;
        let data3 = stream.read_u16::<LittleEndian>()?;
        let data4 = {
            let mut buffer = [0u8; 8];
            stream.read_exact(&mut buffer)?;
            buffer
        };
        Ok(Uuid::from_fields(data1, data2, data3, &data4))
    }
}

impl StreamWrite for Uuid {
    fn stream_write<S>(&self, stream: &mut S) -> Result<()>
        where S: Write
    {
        let (data1, data2, data3, data4) = self.as_fields();
        stream.write_u32::<LittleEndian>(data1)?;
        stream.write_u16::<LittleEndian>(data2)?;
        stream.write_u16::<LittleEndian>(data3)?;
        stream.write_all(data4)?;
        Ok(())
    }
}
