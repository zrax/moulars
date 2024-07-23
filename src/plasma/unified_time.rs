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

use std::io::{BufRead, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::plasma::{StreamRead, StreamWrite};

#[derive(Eq, PartialEq, Copy, Clone, Debug, Default)]
pub struct UnifiedTime {
    secs: u32,
    micros: u32,
}

impl UnifiedTime {
    pub fn new(secs: u32, micros: u32) -> Self {
        Self { secs, micros }
    }

    pub fn from_secs(secs: u32) -> Self {
        Self { secs, micros: 0 }
    }

    pub fn now() -> Result<Self> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)
                    .expect("Current time is before the Unix Epoch");
        Ok(Self {
            // Warning: This will fail in Feb 2106
            secs: u32::try_from(now.as_secs())
                    .with_context(|| format!("Can't encode timestamp {:?}", now))?,
            micros: now.subsec_micros()
        })
    }
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
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        stream.write_u32::<LittleEndian>(self.secs)?;
        stream.write_u32::<LittleEndian>(self.micros)?;
        Ok(())
    }
}
