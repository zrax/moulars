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

use uuid::Uuid;

pub trait StreamRead {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead, Self: Sized;
}

pub trait StreamWrite {
    fn stream_write<S>(&self, stream: &mut S) -> Result<()>
        where S: Write, Self: Sized;
}

impl StreamRead for Uuid {
    fn stream_read<S>(stream: &mut S) -> Result<Uuid>
        where S: BufRead
    {
        let mut buffer = [0u8; 16];
        stream.read_exact(&mut buffer)?;
        Ok(Uuid::from_bytes_le(buffer))
    }
}

impl StreamWrite for Uuid {
    fn stream_write<S>(&self, stream: &mut S) -> Result<()>
        where S: Write
    {
        stream.write_all(self.to_bytes_le().as_slice())
    }
}
