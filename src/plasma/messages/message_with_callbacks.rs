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

use crate::general_error;
use crate::plasma::{StreamRead, StreamWrite};
use crate::plasma::Factory;
use crate::plasma::creatable::derive_creatable;
use super::{Message, MessageInterface};

pub struct MessageWithCallbacks {
    base: Message,
    callbacks: Vec<Box<dyn MessageInterface>>,
}

derive_creatable!(MessageWithCallbacks, Message);

impl StreamRead for MessageWithCallbacks {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let base = Message::stream_read(stream)?;
        let num_callbacks = stream.read_u32::<LittleEndian>()?;
        let mut callbacks = Vec::with_capacity(num_callbacks as usize);
        for _ in 0..num_callbacks {
            if let Some(msg) = Factory::read_message(stream)? {
                callbacks.push(msg);
            } else {
                return Err(general_error!("Unexpected null message in callbacks"));
            }
        }

        Ok(Self { base, callbacks })
    }
}

impl StreamWrite for MessageWithCallbacks {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        let num_callbacks = u32::try_from(self.callbacks.len())
                .map_err(|_| general_error!("Too many messages for stream"))?;

        self.base.stream_write(stream)?;
        stream.write_u32::<LittleEndian>(num_callbacks)?;
        for msg in &self.callbacks {
            Factory::write_creatable(stream, Some(msg.as_creatable()))?;
        }
        Ok(())
    }
}

impl MessageInterface for MessageWithCallbacks {
    fn make_net_safe(&mut self) -> bool {
        for msg in &mut self.callbacks {
            if !msg.make_net_safe() {
                return false;
            }
        }
        true
    }
}
