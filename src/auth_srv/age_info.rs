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
use uuid::Uuid;

use crate::plasma::{StreamRead, StreamWrite};

pub struct NetAgeInfo {
    instance_id: Uuid,
    filename: String,
    instance_name: String,
    user_name: String,
    description: String,
    sequence: u32,
    language: u32,
    population: u32,
    current_population: u32,
}

macro_rules! read_fixed_utf16 {
    ($stream:ident, $len:expr) => ({
        let mut buf = [0u16; $len];
        $stream.read_u16_into::<LittleEndian>(&mut buf)?;
        String::from_utf16_lossy(&buf.split(|ch| ch == &0).next().unwrap())
    })
}

macro_rules! write_fixed_utf16 {
    ($stream:ident, $len:expr, $value:expr) => {
        for ch in $value.encode_utf16().chain(std::iter::repeat(0u16)).take($len) {
            $stream.write_u16::<LittleEndian>(ch)?;
        }
    }
}

impl StreamRead for NetAgeInfo {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let instance_id = Uuid::stream_read(stream)?;
        let filename = read_fixed_utf16!(stream, 64);
        let instance_name = read_fixed_utf16!(stream, 64);
        let user_name = read_fixed_utf16!(stream, 64);
        let description = read_fixed_utf16!(stream, 1024);
        let sequence = stream.read_u32::<LittleEndian>()?;
        let language = stream.read_u32::<LittleEndian>()?;
        let population = stream.read_u32::<LittleEndian>()?;
        let current_population = stream.read_u32::<LittleEndian>()?;

        Ok(Self {
            instance_id, filename, instance_name, user_name, description,
            sequence, language, population, current_population
        })
    }
}

impl StreamWrite for NetAgeInfo {
    fn stream_write<S>(&self, stream: &mut S) -> Result<()>
        where S: Write
    {
        self.instance_id.stream_write(stream)?;
        write_fixed_utf16!(stream, 64, self.filename);
        write_fixed_utf16!(stream, 64, self.instance_name);
        write_fixed_utf16!(stream, 64, self.user_name);
        write_fixed_utf16!(stream, 1024, self.description);
        stream.write_u32::<LittleEndian>(self.sequence)?;
        stream.write_u32::<LittleEndian>(self.language)?;
        stream.write_u32::<LittleEndian>(self.population)?;
        stream.write_u32::<LittleEndian>(self.current_population)?;

        Ok(())
    }
}
