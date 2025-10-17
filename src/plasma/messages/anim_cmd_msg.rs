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
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};

use crate::plasma::{BitVector, StreamRead, StreamWrite};
use crate::plasma::creatable::derive_creatable;
use crate::plasma::safe_string::{ReadSafeStr, WriteSafeStr, StringFormat};
use super::{NetSafety, MessageWithCallbacks};

pub struct AnimCmdMsg {
    base: MessageWithCallbacks,
    cmd: BitVector,
    begin: f32,
    end: f32,
    loop_end: f32,
    loop_begin: f32,
    speed: f32,
    speed_change_rate: f32,
    time: f32,
    anim_name: String,
    loop_name: String,
}

derive_creatable!(AnimCmdMsg, NetSafety, (Message, MessageWithCallbacks));

impl StreamRead for AnimCmdMsg {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let base = MessageWithCallbacks::stream_read(stream)?;
        let cmd = BitVector::stream_read(stream)?;
        let begin = stream.read_f32::<LittleEndian>()?;
        let end = stream.read_f32::<LittleEndian>()?;
        let loop_end = stream.read_f32::<LittleEndian>()?;
        let loop_begin = stream.read_f32::<LittleEndian>()?;
        let speed = stream.read_f32::<LittleEndian>()?;
        let speed_change_rate = stream.read_f32::<LittleEndian>()?;
        let time = stream.read_f32::<LittleEndian>()?;
        let anim_name = stream.read_safe_str(StringFormat::Latin1)?;
        let loop_name = stream.read_safe_str(StringFormat::Latin1)?;

        Ok(Self { base, cmd, begin, end, loop_end, loop_begin, speed,
                  speed_change_rate, time, anim_name, loop_name })
    }
}

impl StreamWrite for AnimCmdMsg {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        self.base.stream_write(stream)?;
        self.cmd.stream_write(stream)?;
        stream.write_f32::<LittleEndian>(self.begin)?;
        stream.write_f32::<LittleEndian>(self.end)?;
        stream.write_f32::<LittleEndian>(self.loop_end)?;
        stream.write_f32::<LittleEndian>(self.loop_begin)?;
        stream.write_f32::<LittleEndian>(self.speed)?;
        stream.write_f32::<LittleEndian>(self.speed_change_rate)?;
        stream.write_f32::<LittleEndian>(self.time)?;
        stream.write_safe_str(&self.anim_name, StringFormat::Latin1)?;
        stream.write_safe_str(&self.loop_name, StringFormat::Latin1)?;
        Ok(())
    }
}

impl NetSafety for AnimCmdMsg {
    fn make_net_safe(&mut self) -> bool {
        self.base.make_net_safe()
    }
}
