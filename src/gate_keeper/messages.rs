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

use std::io::Write;

use anyhow::{anyhow, Result};
use byteorder::{LittleEndian, WriteBytesExt};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use tokio::io::{AsyncReadExt, BufReader};

use crate::net_crypt::CryptTcpStream;
use crate::plasma::{StreamWrite, net_io};

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
    pub async fn read(stream: &mut BufReader<CryptTcpStream>) -> Result<Self> {
        let msg_id = stream.read_u16_le().await?;
        match ClientMsgId::from_u16(msg_id) {
            Some(ClientMsgId::PingRequest) => {
                let trans_id = stream.read_u32_le().await?;
                let ping_time = stream.read_u32_le().await?;
                let payload = net_io::read_sized_buffer(stream, MAX_PING_PAYLOAD).await?;
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
            None => Err(anyhow!("Bad message ID {msg_id}"))
        }
    }
}

impl StreamWrite for GateKeeperToCli {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        match self {
            GateKeeperToCli::PingReply { trans_id, ping_time, payload } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::PingReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_u32::<LittleEndian>(*ping_time)?;
                net_io::write_sized_buffer(stream, payload)?;
            }
            GateKeeperToCli::FileServIpAddressReply { trans_id, ip_addr } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::FileServIpAddressReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                net_io::write_utf16_str(stream, ip_addr)?;
            }
            GateKeeperToCli::AuthServIpAddressReply { trans_id, ip_addr } => {
                stream.write_u16::<LittleEndian>(ServerMsgId::AuthServIpAddressReply as u16)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                net_io::write_utf16_str(stream, ip_addr)?;
            }
        }

        Ok(())
    }
}
