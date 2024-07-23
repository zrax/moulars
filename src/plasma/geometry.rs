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

use anyhow::Result;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use super::{StreamRead, StreamWrite};

#[derive(Copy, Clone, PartialEq, Default, Debug)]
pub struct Vector3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Quaternion {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Matrix44 {
    pub identity: bool,
    pub data: [[f32; 4]; 4],
}

impl StreamRead for Vector3 {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let x = stream.read_f32::<LittleEndian>()?;
        let y = stream.read_f32::<LittleEndian>()?;
        let z = stream.read_f32::<LittleEndian>()?;
        Ok(Self { x, y, z })
    }
}

impl StreamWrite for Vector3 {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        stream.write_f32::<LittleEndian>(self.x)?;
        stream.write_f32::<LittleEndian>(self.y)?;
        stream.write_f32::<LittleEndian>(self.z)?;
        Ok(())
    }
}

impl Default for Quaternion {
    fn default() -> Self {
        Self { x: 0_f32, y: 0_f32, z: 0_f32, w: 1_f32 }
    }
}

impl StreamRead for Quaternion {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let x = stream.read_f32::<LittleEndian>()?;
        let y = stream.read_f32::<LittleEndian>()?;
        let z = stream.read_f32::<LittleEndian>()?;
        let w = stream.read_f32::<LittleEndian>()?;
        Ok(Self { x, y, z, w })
    }
}

impl StreamWrite for Quaternion {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        stream.write_f32::<LittleEndian>(self.x)?;
        stream.write_f32::<LittleEndian>(self.y)?;
        stream.write_f32::<LittleEndian>(self.z)?;
        stream.write_f32::<LittleEndian>(self.w)?;
        Ok(())
    }
}

const IDENTITY_MATRIX: [[f32; 4]; 4] = [
    [1_f32, 0_f32, 0_f32, 0_f32],
    [0_f32, 1_f32, 0_f32, 0_f32],
    [0_f32, 0_f32, 1_f32, 0_f32],
    [0_f32, 0_f32, 0_f32, 1_f32],
];

impl Default for Matrix44 {
    fn default() -> Self {
        Self { identity: true, data: IDENTITY_MATRIX }
    }
}

impl StreamRead for Matrix44 {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        if stream.read_u8()? != 0 {
            let mut data = [[0_f32; 4]; 4];
            for row in &mut data {
                for val in row {
                    *val = stream.read_f32::<LittleEndian>()?;
                }
            }
            Ok(Self { identity: false, data })
        } else {
            Ok(Matrix44::default())
        }
    }
}

impl StreamWrite for Matrix44 {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        if self.identity {
            stream.write_u8(0)?;
        } else {
            stream.write_u8(1)?;
            for row in self.data {
                for val in row {
                    stream.write_f32::<LittleEndian>(val)?;
                }
            }
        }
        Ok(())
    }
}
