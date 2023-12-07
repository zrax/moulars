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

use crate::general_error;
use crate::plasma::{UnifiedTime, StreamRead, StreamWrite};

pub struct NetMessage {
    content_flags: u32,
    timestamp: UnifiedTime,
    context: u32,
    trans_id: u32,
    player_id: u32,
    acct_id: Uuid,
}

impl NetMessage {
    // Content Flags
    pub const HAS_TIME_SENT: u32                = 1 << 0;
    pub const HAS_GAME_MSG_RECEIVERS: u32       = 1 << 1;
    pub const ECHO_BACK_TO_SENDER: u32          = 1 << 2;
    pub const REQUEST_P2P: u32                  = 1 << 3;
    pub const ALLOW_TIME_OUT: u32               = 1 << 4;
    pub const INDIRECT_MEMBER: u32              = 1 << 5;
    pub const PUBLIC_IP_CLIENT: u32             = 1 << 6;
    pub const HAS_CONTEXT: u32                  = 1 << 7;
    pub const ASK_VAULT_FOR_GAME_STATE: u32     = 1 << 8;
    pub const HAS_TRANSACTION_ID: u32           = 1 << 9;
    pub const NEW_SDL_STATE: u32                = 1 << 10;
    pub const INITIAL_AGE_STATE_REQUEST: u32    = 1 << 11;
    pub const HAS_PLAYER_ID: u32                = 1 << 12;
    pub const USE_RELEVANCE_REGIONS: u32        = 1 << 13;
    pub const HAS_ACCT_UUID: u32                = 1 << 14;
    pub const INTER_AGE_ROUTING: u32            = 1 << 15;
    pub const HAS_VERSION: u32                  = 1 << 16;
    pub const IS_SYSTEM_MESSAGE: u32            = 1 << 17;
    pub const NEEDS_RELIABLE_SEND: u32          = 1 << 18;
    pub const ROUTE_TO_ALL_PLAYERS: u32         = 1 << 19;
}

const NETMSG_PROTOCOL_MAJ: u8 = 12;
const NETMSG_PROTOCOL_MIN: u8 = 6;

impl StreamRead for NetMessage {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let content_flags = stream.read_u32::<LittleEndian>()?;

        if (content_flags & Self::HAS_VERSION) != 0 {
            // Don't bother storing these values...  We only accept one
            // version anyway.
            let protocol_maj = stream.read_u8()?;
            let protocol_min = stream.read_u8()?;

            if (protocol_maj != NETMSG_PROTOCOL_MAJ)
                    || (protocol_min != NETMSG_PROTOCOL_MIN)
            {
                return Err(general_error!("Invalid protocol version: {}.{}",
                                          protocol_maj, protocol_min));
            }
        }

        let timestamp = if (content_flags & Self::HAS_TIME_SENT) != 0 {
            UnifiedTime::stream_read(stream)?
        } else {
            UnifiedTime::default()
        };
        let context = if (content_flags & Self::HAS_CONTEXT) != 0 {
            stream.read_u32::<LittleEndian>()?
        } else {
            0
        };
        let trans_id = if (content_flags & Self::HAS_TRANSACTION_ID) != 0 {
            stream.read_u32::<LittleEndian>()?
        } else {
            0
        };
        let player_id = if (content_flags & Self::HAS_PLAYER_ID) != 0 {
            stream.read_u32::<LittleEndian>()?
        } else {
            0
        };
        let acct_id = if (content_flags & Self::HAS_ACCT_UUID) != 0 {
            Uuid::stream_read(stream)?
        } else {
            Uuid::nil()
        };

        Ok(Self { content_flags, timestamp, context, trans_id, player_id, acct_id })
    }
}

impl StreamWrite for NetMessage {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        stream.write_u32::<LittleEndian>(self.content_flags)?;

        if (self.content_flags & Self::HAS_VERSION) != 0 {
            stream.write_u8(NETMSG_PROTOCOL_MAJ)?;
            stream.write_u8(NETMSG_PROTOCOL_MIN)?;
        }
        if (self.content_flags & Self::HAS_TIME_SENT) != 0 {
            self.timestamp.stream_write(stream)?;
        }
        if (self.content_flags & Self::HAS_CONTEXT) != 0 {
            stream.write_u32::<LittleEndian>(self.context)?;
        }
        if (self.content_flags & Self::HAS_TRANSACTION_ID) != 0 {
            stream.write_u32::<LittleEndian>(self.trans_id)?;
        }
        if (self.content_flags & Self::HAS_PLAYER_ID) != 0 {
            stream.write_u32::<LittleEndian>(self.player_id)?;
        }
        if (self.content_flags & Self::HAS_ACCT_UUID) != 0 {
            self.acct_id.stream_write(stream)?;
        }

        Ok(())
    }
}
