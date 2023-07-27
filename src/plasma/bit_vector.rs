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

use crate::general_error;
use crate::plasma::{StreamRead, StreamWrite};

#[derive(Default)]
pub struct BitVector {
    bits: Vec<u32>
}

impl BitVector {
    pub fn new() -> Self {
        BitVector { bits: Vec::new() }
    }

    pub fn get(&self, bit: usize) -> bool {
        if (bit / 32) >= self.bits.len() {
            false
        } else {
            (self.bits[bit / 32] & (1 << (bit % 32))) != 0
        }
    }

    pub fn set(&mut self, bit: usize, value: bool) {
        if (bit / 32) >= self.bits.len() {
            self.bits.resize((bit / 32) + 1, 0u32);
        }
        if value {
            self.bits[bit / 32] |= 1 << (bit % 32);
        } else {
            self.bits[bit / 32] &= !(1 << (bit % 32));
        }
    }
}

impl StreamRead for BitVector {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let count = stream.read_u32::<LittleEndian>()?;
        let mut bits = vec![0; count as usize];
        stream.read_u32_into::<LittleEndian>(&mut bits)?;
        Ok(BitVector { bits })
    }
}

impl StreamWrite for BitVector {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        if self.bits.len() > u32::MAX as usize {
            return Err(general_error!("Waaaaaay too many bits..."));
        }
        stream.write_u32::<LittleEndian>(self.bits.len() as u32)?;
        for bitfield in &self.bits {
            stream.write_u32::<LittleEndian>(*bitfield)?;
        }
        Ok(())
    }
}

#[test]
fn test_bit_vector() {
    let mut bv = BitVector::new();
    assert_eq!(bv.bits.len(), 0);
    assert_eq!(bv.get(0), false);
    assert_eq!(bv.get(31), false);
    assert_eq!(bv.get(32), false);

    bv.set(0, true);
    assert_eq!(bv.bits.len(), 1);
    assert_eq!(bv.get(0), true);
    assert_eq!(bv.get(31), false);
    assert_eq!(bv.get(32), false);

    bv.set(0, false);
    assert_eq!(bv.bits.len(), 1);
    assert_eq!(bv.get(0), false);
    assert_eq!(bv.get(31), false);
    assert_eq!(bv.get(32), false);

    bv.set(31, true);
    assert_eq!(bv.bits.len(), 1);
    assert_eq!(bv.get(0), false);
    assert_eq!(bv.get(31), true);
    assert_eq!(bv.get(32), false);

    bv.set(32, true);
    assert_eq!(bv.bits.len(), 2);
    assert_eq!(bv.get(0), false);
    assert_eq!(bv.get(31), true);
    assert_eq!(bv.get(32), true);
}
