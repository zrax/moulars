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

use anyhow::{Context, Result};
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};

use crate::plasma::{Key, Creatable, StreamRead, StreamWrite};

pub struct Message {
    sender: Key,
    receivers: Vec<Key>,
    timestamp: f64,
    bcast_flags: u32,
}

impl Message {
    /* BCast Flags */
    /*
    pub const BCAST_BY_TYPE: u32                = 1 << 0;
    pub const UNUSED: u32                       = 1 << 1;
    pub const PROPAGATE_TO_CHILDREN: u32        = 1 << 2;
    pub const BY_EXACT_TYPE: u32                = 1 << 3;
    pub const PROPAGATE_TO_MODIFIERS: u32       = 1 << 4;
    pub const CLEAR_AFTER_BCAST: u32            = 1 << 5;
    pub const NET_PROPAGATE: u32                = 1 << 6;
    pub const NET_SENT: u32                     = 1 << 7;
    pub const NET_USE_RELEVANCE_REGIONS: u32    = 1 << 8;
    pub const NET_FORCE: u32                    = 1 << 9;
    pub const NET_NON_LOCAL: u32                = 1 << 10;
    pub const LOCAL_PROPAGATE: u32              = 1 << 11;
    pub const MSG_WATCH: u32                    = 1 << 12;
    pub const NET_START_CASCADE: u32            = 1 << 13;
    pub const NET_ALLOW_INTER_AGE: u32          = 1 << 14;
    pub const NET_SEND_UNRELIABLE: u32          = 1 << 15;
    pub const CCR_SEND_TO_ALL_PLAYERS: u32      = 1 << 16;
    pub const NET_CREATED_REMOTELY: u32         = 1 << 17;
    */
}

impl StreamRead for Message {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let sender = Key::stream_read(stream)?;

        let num_receivers = stream.read_u32::<LittleEndian>()?;
        let mut receivers = Vec::with_capacity(num_receivers as usize);
        for _ in 0..num_receivers {
            receivers.push(Key::stream_read(stream)?);
        }

        let timestamp = stream.read_f64::<LittleEndian>()?;
        let bcast_flags = stream.read_u32::<LittleEndian>()?;

        Ok(Self { sender, receivers, timestamp, bcast_flags })
    }
}

impl StreamWrite for Message {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        self.sender.stream_write(stream)?;
        let num_receivers = u32::try_from(self.receivers.len())
                .context("Too many receivers for stream")?;
        stream.write_u32::<LittleEndian>(num_receivers)?;
        for rc_key in &self.receivers {
            rc_key.stream_write(stream)?;
        }
        stream.write_f64::<LittleEndian>(self.timestamp)?;
        stream.write_u32::<LittleEndian>(self.bcast_flags)?;
        Ok(())
    }
}

pub trait MessageInterface: Creatable {
    // Call this to make a Message safe for transmission to other clients
    // over the network.  If it cannot be made safe, or should not be
    // transmitted, this should return `false` so the server will reject it.
    fn make_net_safe(&mut self) -> bool;
}
