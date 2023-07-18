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

use std::io::{Write, Result};

use byteorder::{LittleEndian, WriteBytesExt};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use tokio::io::{AsyncReadExt, BufReader};

use crate::general_error;
use crate::crypt::CryptStream;
use crate::plasma::StreamWrite;

#[allow(clippy::enum_variant_names)]
pub enum CliToGateKeeper {
    PingRequest {
        trans_id: u32,
        ping_time: u32,
        payload: Vec<u8>,
    },
    FileServIpAddressRequest {
        trans_id: u32,
        from_patcher: bool,
    },
    AuthServIpAddressRequest {
        trans_id: u32,
    },
}

#[allow(clippy::enum_variant_names)]
pub enum GateKeeperToCli {
    PingReply {
        trans_id: u32,
        ping_time: u32,
        payload: Vec<u8>,
    },
    FileServIpAddressReply {
        trans_id: u32,
        ip_addr: String,
    },
    AuthServIpAddressReply {
        trans_id: u32,
        ip_addr: String,
    },
}

#[repr(u16)]
#[derive(FromPrimitive)]
#[allow(clippy::enum_variant_names)]
enum ClientMsgId {
    PingRequest = 0,
    FileServIPAddressRequest,
    AuthServIPAddressRequest,
}

#[repr(u16)]
#[allow(clippy::enum_variant_names)]
enum ServerMsgId {
    PingReply = 0,
    FileServIpAddressReply,
    AuthServIpAddressReply,
}

const MAX_PING_PAYLOAD: u32 = 64 * 1024;

impl CliToGateKeeper {
    pub async fn read(stream: &mut BufReader<CryptStream>) -> Result<Self> {
        let msg_id = stream.read_u16_le().await?;
        match ClientMsgId::from_u16(msg_id) {
            Some(ClientMsgId::PingRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let ping_time = stream.read_u32_le().await?;
                let payload_size = stream.read_u32_le().await?;
                if payload_size > MAX_PING_PAYLOAD {
                    return Err(general_error!("Ping payload too large ({} bytes)",
                               payload_size));
                }
                let mut payload = vec![0; payload_size as usize];
                stream.read_exact(payload.as_mut_slice()).await?;
                Ok(CliToGateKeeper::PingRequest { trans_id, ping_time, payload })
            }
            Some(ClientMsgId::FileServIPAddressRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let from_patcher = stream.read_u8().await? != 0;
                Ok(CliToGateKeeper::FileServIpAddressRequest { trans_id, from_patcher })
            }
            Some(ClientMsgId::AuthServIPAddressRequest) => {
                let trans_id = stream.read_u32_le().await?;
                Ok(CliToGateKeeper::AuthServIpAddressRequest { trans_id })
            }
            None => Err(general_error!("Bad message ID {}", msg_id))
        }
    }
}

impl StreamWrite for GateKeeperToCli {
    fn stream_write<S>(&self, stream: &mut S) -> Result<()>
        where S: Write
    {
        match self {
            GateKeeperToCli::PingReply { trans_id, ping_time, payload } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::PingReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_u32::<LittleEndian>(*ping_time)?;
                stream.write_u32::<LittleEndian>(payload.len() as u32)?;
                stream.write_all(payload.as_slice())?;
            }
            GateKeeperToCli::FileServIpAddressReply { trans_id, ip_addr } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::FileServIpAddressReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                let ip_addr_utf16: Vec<u16> = ip_addr.encode_utf16().collect();
                stream.write_u16::<LittleEndian>(ip_addr_utf16.len() as u16)?;
                for ch in ip_addr_utf16 {
                    stream.write_u16::<LittleEndian>(ch)?;
                }
            }
            GateKeeperToCli::AuthServIpAddressReply { trans_id, ip_addr } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::AuthServIpAddressReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                let ip_addr_utf16: Vec<u16> = ip_addr.encode_utf16().collect();
                stream.write_u16::<LittleEndian>(ip_addr_utf16.len() as u16)?;
                for ch in ip_addr_utf16 {
                    stream.write_u16::<LittleEndian>(ch)?;
                }
            }
        }

        Ok(())
    }
}
