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

use std::io::{BufRead, Write, Cursor, Result};
use std::mem::size_of;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use tokio::io::BufReader;
use tokio::net::TcpStream;

use crate::general_error;
use crate::netcli::NetResultCode;
use crate::plasma::{StreamRead, StreamWrite, net_io};
use super::manifest::Manifest;

pub enum CliToFile {
    PingRequest { ping_time: u32 },
    BuildIdRequest { trans_id: u32 },
    ManifestRequest {
        trans_id: u32,
        manifest_name: String,
        build_id: u32,
    },
    DownloadRequest {
        trans_id: u32,
        filename: String,
        build_id: u32,
    },
    ManifestEntryAck {
        trans_id: u32,
        reader_id: u32,
    },
    DownloadChunkAck {
        trans_id: u32,
        reader_id: u32,
    },
}

pub enum FileToCli {
    PingReply { ping_time: u32 },
    BuildIdReply {
        trans_id: u32,
        result: i32,
        build_id: u32,
    },
    #[allow(unused)]
    BuildIdUpdate { build_id: u32 },
    ManifestReply {
        trans_id: u32,
        result: i32,
        reader_id: u32,
        manifest: Manifest,
    },
    FileDownloadReply {
        trans_id: u32,
        result: i32,
        reader_id: u32,
        total_size: u32,
        file_data: Vec<u8>,
    },
}

#[repr(u32)]
#[derive(FromPrimitive)]
enum ClientMsgId {
    PingRequest = 0,
    BuildIdRequest = 10,
    ManifestRequest = 20,
    DownloadRequest = 21,
    ManifestEntryAck = 22,
    DownloadChunkAck = 23,
}

#[repr(u32)]
enum ServerMsgId {
    PingReply = 0,
    BuildIdReply = 10,
    BuildIdUpdate = 11,
    ManifestReply = 20,
    FileDownloadReply = 21,
}

macro_rules! read_fixed_utf16 {
    ($stream:ident, $len:expr) => ({
        let mut buf = [0u16; $len];
        $stream.read_u16_into::<LittleEndian>(&mut buf)?;
        String::from_utf16_lossy(&buf.split(|ch| ch == &0).next().unwrap())
    })
}

impl CliToFile {
    pub async fn read(stream: &mut BufReader<TcpStream>) -> Result<Self> {
        use tokio::io::AsyncReadExt;

        let msg_size = stream.read_u32_le().await?;
        if (msg_size as usize) < size_of::<u32>() {
            return Err(general_error!("Message size too small"));
        }
        let mut msg_buf = vec![0u8; (msg_size as usize) - size_of::<u32>()];
        stream.read_exact(&mut msg_buf).await?;
        CliToFile::stream_read(&mut Cursor::new(msg_buf))
    }
}

impl StreamRead for CliToFile {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let msg_id = stream.read_u32::<LittleEndian>()?;
        match ClientMsgId::from_u32(msg_id) {
            Some(ClientMsgId::PingRequest) => {
                let ping_time = stream.read_u32::<LittleEndian>()?;
                Ok(CliToFile::PingRequest { ping_time })
            }
            Some(ClientMsgId::BuildIdRequest) => {
                let trans_id = stream.read_u32::<LittleEndian>()?;
                Ok(CliToFile::BuildIdRequest { trans_id })
            }
            Some(ClientMsgId::ManifestRequest) => {
                let trans_id = stream.read_u32::<LittleEndian>()?;
                let manifest_name = read_fixed_utf16!(stream, 260);
                let build_id = stream.read_u32::<LittleEndian>()?;
                Ok(CliToFile::ManifestRequest { trans_id, manifest_name, build_id })
            }
            Some(ClientMsgId::ManifestEntryAck) => {
                let trans_id = stream.read_u32::<LittleEndian>()?;
                let reader_id = stream.read_u32::<LittleEndian>()?;
                Ok(CliToFile::ManifestEntryAck { trans_id, reader_id })
            }
            Some(ClientMsgId::DownloadRequest) => {
                let trans_id = stream.read_u32::<LittleEndian>()?;
                let filename = read_fixed_utf16!(stream, 260);
                let build_id = stream.read_u32::<LittleEndian>()?;
                Ok(CliToFile::DownloadRequest { trans_id, filename, build_id })
            }
            Some(ClientMsgId::DownloadChunkAck) => {
                let trans_id = stream.read_u32::<LittleEndian>()?;
                let reader_id = stream.read_u32::<LittleEndian>()?;
                Ok(CliToFile::DownloadChunkAck { trans_id, reader_id })
            }
            None => Err(general_error!("Bad message ID {}", msg_id))
        }
    }
}

impl FileToCli {
    // Requires special buffering to write the output size correctly
    pub async fn write(&self, stream: &mut TcpStream) -> Result<()> {
        use tokio::io::AsyncWriteExt;

        let buffer = {
            let mut buffer = Cursor::new(Vec::new());
            self.stream_write(&mut buffer)?;
            buffer.into_inner()
        };

        let msg_size = u32::try_from(size_of::<u32>() + buffer.len())
                .map_err(|_| general_error!("Message too large for stream"))?;
        stream.write_u32_le(msg_size).await?;
        stream.write_all(&buffer).await
    }

    pub fn manifest_error(trans_id: u32, result: NetResultCode) -> Self {
        Self::ManifestReply {
            trans_id,
            result: result as i32,
            reader_id: 0,
            manifest: Manifest::new(),
        }
    }

    pub fn download_error(trans_id: u32, result: NetResultCode) -> Self {
        Self::FileDownloadReply {
            trans_id,
            result: result as i32,
            reader_id: 0,
            total_size: 0,
            file_data: Vec::new(),
        }
    }
}

impl StreamWrite for FileToCli {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        match self {
            FileToCli::PingReply { ping_time } => {
                stream.write_u32::<LittleEndian>(ServerMsgId::PingReply as u32)?;
                stream.write_u32::<LittleEndian>(*ping_time)?;
            }
            FileToCli::BuildIdReply { trans_id, result, build_id } => {
                stream.write_u32::<LittleEndian>(ServerMsgId::BuildIdReply as u32)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                stream.write_u32::<LittleEndian>(*build_id)?;
            }
            FileToCli::BuildIdUpdate { build_id } => {
                stream.write_u32::<LittleEndian>(ServerMsgId::BuildIdUpdate as u32)?;
                stream.write_u32::<LittleEndian>(*build_id)?;
            }
            FileToCli::ManifestReply { trans_id, result, reader_id, manifest } => {
                stream.write_u32::<LittleEndian>(ServerMsgId::ManifestReply as u32)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                stream.write_u32::<LittleEndian>(*reader_id)?;
                manifest.stream_write(stream)?;
            }
            FileToCli::FileDownloadReply { trans_id, result, reader_id,
                                           total_size, file_data } => {
                stream.write_u32::<LittleEndian>(ServerMsgId::FileDownloadReply as u32)?;
                stream.write_u32::<LittleEndian>(*trans_id)?;
                stream.write_i32::<LittleEndian>(*result)?;
                stream.write_u32::<LittleEndian>(*reader_id)?;
                stream.write_u32::<LittleEndian>(*total_size)?;
                net_io::write_sized_buffer(stream, file_data)?;
            }
        }

        Ok(())
    }
}
