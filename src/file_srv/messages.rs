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

use crate::plasma::{StreamRead, StreamWrite};
use crate::file_srv::manifest::Manifest;

use std::io::{BufRead, Write, Result, Error, ErrorKind, Cursor};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

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
        result: u32,
        build_id: u32,
    },
    BuildIdUpdate { build_id: u32 },
    ManifestReply {
        trans_id: u32,
        result: u32,
        reader_id: u32,
        manifest: Manifest,
    },
    FileDownloadReply {
        trans_id: u32,
        result: u32,
        reader_id: u32,
        file_size: u32,
        file_data: Vec<u8>,
    },
}

const CLI2FILE_PING_REQUEST: u32 = 0;
const CLI2FILE_BUILD_ID_REQUEST: u32 = 10;
const CLI2FILE_MANIFEST_REQUEST: u32 = 20;
const CLI2FILE_DOWNLOAD_REQUEST: u32 = 21;
const CLI2FILE_MANIFEST_ENTRY_ACK: u32 = 22;
const CLI2FILE_DOWNLOAD_CHUNK_ACK: u32 = 23;

const FILE2CLI_PING_REPLY: u32 = 0;
const FILE2CLI_BUILD_ID_REPLY: u32 = 10;
const FILE2CLI_BUILD_ID_UPDATE: u32 = 11;
const FILE2CLI_MANIFEST_REPLY: u32 = 20;
const FILE2CLI_FILE_DOWNLOAD_REPLY: u32 = 21;

macro_rules! read_fixed_utf16 {
    ($stream:ident, $len:expr) => ({
        let mut buf = [0u16; $len];
        $stream.read_u16_into::<LittleEndian>(&mut buf)?;
        String::from_utf16_lossy(&buf.split(|ch| ch == &0).next().unwrap())
    })
}

impl StreamRead for CliToFile {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let msg_size = stream.read_u32::<LittleEndian>()?;
        let mut msg_buf = vec![0u8; msg_size as usize];
        stream.read_exact(&mut msg_buf)?;
        let mut msg = Cursor::new(msg_buf);

        match msg.read_u32::<LittleEndian>()? {
            CLI2FILE_PING_REQUEST => {
                let ping_time = msg.read_u32::<LittleEndian>()?;
                Ok(CliToFile::PingRequest { ping_time })
            }
            CLI2FILE_BUILD_ID_REQUEST => {
                let trans_id = msg.read_u32::<LittleEndian>()?;
                Ok(CliToFile::BuildIdRequest { trans_id })
            }
            CLI2FILE_MANIFEST_REQUEST => {
                let trans_id = msg.read_u32::<LittleEndian>()?;
                let manifest_name = read_fixed_utf16!(msg, 260);
                let build_id = msg.read_u32::<LittleEndian>()?;
                Ok(CliToFile::ManifestRequest { trans_id, manifest_name, build_id })
            }
            CLI2FILE_MANIFEST_ENTRY_ACK => {
                let trans_id = msg.read_u32::<LittleEndian>()?;
                let reader_id = msg.read_u32::<LittleEndian>()?;
                Ok(CliToFile::ManifestEntryAck { trans_id, reader_id })
            }
            CLI2FILE_DOWNLOAD_REQUEST => {
                let trans_id = msg.read_u32::<LittleEndian>()?;
                let filename = read_fixed_utf16!(msg, 260);
                let build_id = msg.read_u32::<LittleEndian>()?;
                Ok(CliToFile::DownloadRequest { trans_id, filename, build_id })
            }
            CLI2FILE_DOWNLOAD_CHUNK_ACK => {
                let trans_id = msg.read_u32::<LittleEndian>()?;
                let reader_id = msg.read_u32::<LittleEndian>()?;
                Ok(CliToFile::DownloadChunkAck { trans_id, reader_id })
            }
            msg_id => {
                Err(Error::new(ErrorKind::Other, format!("Bad message ID {}", msg_id)))
            },
        }
    }
}

impl StreamWrite for FileToCli {
    fn stream_write<S>(&self, stream: &mut S) -> Result<()>
        where S: Write
    {
        let mut buffer = Cursor::new(Vec::new());
        buffer.write_u32::<LittleEndian>(0)?;

        match self {
            FileToCli::PingReply { ping_time } => {
                buffer.write_u32::<LittleEndian>(FILE2CLI_PING_REPLY)?;
                buffer.write_u32::<LittleEndian>(*ping_time)?;
            }
            FileToCli::BuildIdReply { trans_id, result, build_id } => {
                buffer.write_u32::<LittleEndian>(FILE2CLI_BUILD_ID_REPLY)?;
                buffer.write_u32::<LittleEndian>(*trans_id)?;
                buffer.write_u32::<LittleEndian>(*result)?;
                buffer.write_u32::<LittleEndian>(*build_id)?;
            }
            FileToCli::BuildIdUpdate { build_id } => {
                buffer.write_u32::<LittleEndian>(FILE2CLI_BUILD_ID_UPDATE)?;
                buffer.write_u32::<LittleEndian>(*build_id)?;
            }
            FileToCli::ManifestReply { trans_id, result, reader_id, manifest } => {
                buffer.write_u32::<LittleEndian>(FILE2CLI_MANIFEST_REPLY)?;
                buffer.write_u32::<LittleEndian>(*trans_id)?;
                buffer.write_u32::<LittleEndian>(*result)?;
                buffer.write_u32::<LittleEndian>(*reader_id)?;
                todo!();
            }
            FileToCli::FileDownloadReply { trans_id, result, reader_id, file_size,
                                           file_data } => {
                buffer.write_u32::<LittleEndian>(FILE2CLI_FILE_DOWNLOAD_REPLY)?;
                buffer.write_u32::<LittleEndian>(*trans_id)?;
                buffer.write_u32::<LittleEndian>(*result)?;
                buffer.write_u32::<LittleEndian>(*reader_id)?;
                buffer.write_u32::<LittleEndian>(*file_size)?;
                buffer.write_u32::<LittleEndian>(file_data.len() as u32)?;
                buffer.write_all(file_data.as_slice())?;
            }
        }

        // Update the message size and send it as a single chunk of data
        let buf_size = buffer.position() as u32;
        buffer.set_position(0);
        buffer.write_u32::<LittleEndian>(buf_size)?;
        stream.write_all(buffer.get_ref())?;
        Ok(())
    }
}
