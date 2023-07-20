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
use std::mem::size_of;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use uuid::Uuid;

use crate::general_error;
use crate::plasma::{StreamRead, StreamWrite};

#[derive(Debug, Default)]
pub struct VaultNode {
    fields: u64,

    node_id: u32,
    create_time: u32,
    modify_time: u32,
    create_age_name: String,
    create_age_uuid: Uuid,
    creator_uuid: Uuid,
    creator_id: u32,
    node_type: i32,
    int32_1: i32,
    int32_2: i32,
    int32_3: i32,
    int32_4: i32,
    uint32_1: u32,
    uint32_2: u32,
    uint32_3: u32,
    uint32_4: u32,
    uuid_1: Uuid,
    uuid_2: Uuid,
    uuid_3: Uuid,
    uuid_4: Uuid,
    string64_1: String,
    string64_2: String,
    string64_3: String,
    string64_4: String,
    string64_5: String,
    string64_6: String,
    istring64_1: String,
    istring64_2: String,
    text_1: String,
    text_2: String,
    blob_1: Vec<u8>,
    blob_2: Vec<u8>,
}

impl VaultNode {
    pub fn new() -> Self {
        Self { .. Default::default() }
    }
}

const FIELD_NODE_IDX: u64           = 1 << 0;
const FIELD_CREATE_TIME: u64        = 1 << 1;
const FIELD_MODIFY_TIME: u64        = 1 << 2;
const FIELD_CREATE_AGE_NAME: u64    = 1 << 3;
const FIELD_CREATE_AGE_UUID: u64    = 1 << 4;
const FIELD_CREATOR_UUID: u64       = 1 << 5;
const FIELD_CREATOR_IDX: u64        = 1 << 6;
const FIELD_NODE_TYPE: u64          = 1 << 7;
const FIELD_INT32_1: u64            = 1 << 8;
const FIELD_INT32_2: u64            = 1 << 9;
const FIELD_INT32_3: u64            = 1 << 10;
const FIELD_INT32_4: u64            = 1 << 11;
const FIELD_UINT32_1: u64           = 1 << 12;
const FIELD_UINT32_2: u64           = 1 << 13;
const FIELD_UINT32_3: u64           = 1 << 14;
const FIELD_UINT32_4: u64           = 1 << 15;
const FIELD_UUID_1: u64             = 1 << 16;
const FIELD_UUID_2: u64             = 1 << 17;
const FIELD_UUID_3: u64             = 1 << 18;
const FIELD_UUID_4: u64             = 1 << 19;
const FIELD_STRING64_1: u64         = 1 << 20;
const FIELD_STRING64_2: u64         = 1 << 21;
const FIELD_STRING64_3: u64         = 1 << 22;
const FIELD_STRING64_4: u64         = 1 << 23;
const FIELD_STRING64_5: u64         = 1 << 24;
const FIELD_STRING64_6: u64         = 1 << 25;
const FIELD_ISTRING64_1: u64        = 1 << 26;
const FIELD_ISTRING64_2: u64        = 1 << 27;
const FIELD_TEXT_1: u64             = 1 << 28;
const FIELD_TEXT_2: u64             = 1 << 29;
const FIELD_BLOB_1: u64             = 1 << 30;
const FIELD_BLOB_2: u64             = 1 << 31;

// Strings in vault nodes use UTF-16, but store the number of BYTES taken up
// by the string, including the terminating nul character.
fn read_vault_string<S>(stream: &mut S) -> Result<String>
    where S: BufRead
{
    let size = stream.read_u32::<LittleEndian>()? as usize;
    if (size % size_of::<u16>()) != 0 || size < size_of::<u16>() {
        return Err(general_error!("Bad UTF-16 data size ({} bytes)", size));
    }
    let mut buffer = vec![0; (size - 1) / size_of::<u16>()];
    stream.read_u16_into::<LittleEndian>(&mut buffer)?;
    if stream.read_u16::<LittleEndian>()? != 0 {
        return Err(general_error!("Vault string was not nul-terminated"));
    }

    Ok(String::from_utf16_lossy(&buffer))
}

fn write_vault_string<S>(stream: &mut S, value: &str) -> Result<()>
    where S: Write
{
    let buffer: Vec<u16> = value.encode_utf16().collect();
    stream.write_u32::<LittleEndian>(((buffer.len() + 1) * size_of::<u16>()) as u32)?;
    for ch in buffer {
        stream.write_u16::<LittleEndian>(ch)?;
    }
    stream.write_u16::<LittleEndian>(0)
}

macro_rules! f_read_i32 {
    ($stream:ident, $fields:ident, $field:ident) => {
        if ($fields & $field) != 0 {
            $stream.read_i32::<LittleEndian>()?
        } else {
            0
        }
    }
}

macro_rules! f_read_u32 {
    ($stream:ident, $fields:expr, $field:ident) => {
        if ($fields & $field) != 0 {
            $stream.read_u32::<LittleEndian>()?
        } else {
            0
        }
    }
}

macro_rules! f_read_string {
    ($stream:ident, $fields:expr, $field:ident) => {
        if ($fields & $field) != 0 {
            read_vault_string($stream)?
        } else {
            String::new()
        }
    }
}

macro_rules! f_read_uuid {
    ($stream:ident, $fields:expr, $field:ident) => {
        if ($fields & $field) != 0 {
            Uuid::stream_read($stream)?
        } else {
            Uuid::nil()
        }
    }
}

macro_rules! f_read_blob {
    ($stream:ident, $fields:expr, $field:ident) => {
        if ($fields & $field) != 0 {
            let size = $stream.read_u32::<LittleEndian>()?;
            let mut blob = vec![0; size as usize];
            $stream.read_exact(blob.as_mut_slice())?;
            blob
        } else {
            Vec::new()
        }
    }
}

impl StreamRead for VaultNode {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let fields = stream.read_u64::<LittleEndian>()?;

        let node_id = f_read_u32!(stream, fields, FIELD_NODE_IDX);
        let create_time = f_read_u32!(stream, fields, FIELD_CREATE_TIME);
        let modify_time = f_read_u32!(stream, fields, FIELD_MODIFY_TIME);
        let create_age_name = f_read_string!(stream, fields, FIELD_CREATE_AGE_NAME);
        let create_age_uuid = f_read_uuid!(stream, fields, FIELD_CREATE_AGE_UUID);
        let creator_uuid = f_read_uuid!(stream, fields, FIELD_CREATOR_UUID);
        let creator_id = f_read_u32!(stream, fields, FIELD_CREATOR_IDX);
        let node_type = f_read_i32!(stream, fields, FIELD_NODE_TYPE);
        let int32_1 = f_read_i32!(stream, fields, FIELD_INT32_1);
        let int32_2 = f_read_i32!(stream, fields, FIELD_INT32_2);
        let int32_3 = f_read_i32!(stream, fields, FIELD_INT32_3);
        let int32_4 = f_read_i32!(stream, fields, FIELD_INT32_4);
        let uint32_1 = f_read_u32!(stream, fields, FIELD_UINT32_1);
        let uint32_2 = f_read_u32!(stream, fields, FIELD_UINT32_2);
        let uint32_3 = f_read_u32!(stream, fields, FIELD_UINT32_3);
        let uint32_4 = f_read_u32!(stream, fields, FIELD_UINT32_4);
        let uuid_1 = f_read_uuid!(stream, fields, FIELD_UUID_1);
        let uuid_2 = f_read_uuid!(stream, fields, FIELD_UUID_2);
        let uuid_3 = f_read_uuid!(stream, fields, FIELD_UUID_3);
        let uuid_4 = f_read_uuid!(stream, fields, FIELD_UUID_4);
        let string64_1 = f_read_string!(stream, fields, FIELD_STRING64_1);
        let string64_2 = f_read_string!(stream, fields, FIELD_STRING64_2);
        let string64_3 = f_read_string!(stream, fields, FIELD_STRING64_3);
        let string64_4 = f_read_string!(stream, fields, FIELD_STRING64_4);
        let string64_5 = f_read_string!(stream, fields, FIELD_STRING64_5);
        let string64_6 = f_read_string!(stream, fields, FIELD_STRING64_6);
        let istring64_1 = f_read_string!(stream, fields, FIELD_ISTRING64_1);
        let istring64_2 = f_read_string!(stream, fields, FIELD_ISTRING64_2);
        let text_1 = f_read_string!(stream, fields, FIELD_TEXT_1);
        let text_2 = f_read_string!(stream, fields, FIELD_TEXT_2);
        let blob_1 = f_read_blob!(stream, fields, FIELD_BLOB_1);
        let blob_2 = f_read_blob!(stream, fields, FIELD_BLOB_2);

        Ok(Self {
            fields,
            node_id, create_time, modify_time,
            create_age_name, create_age_uuid,
            creator_uuid, creator_id,
            node_type,
            int32_1, int32_2, int32_3, int32_4,
            uint32_1, uint32_2, uint32_3, uint32_4,
            uuid_1, uuid_2, uuid_3, uuid_4,
            string64_1, string64_2, string64_3, string64_4, string64_5, string64_6,
            istring64_1, istring64_2,
            text_1, text_2,
            blob_1, blob_2,
        })
    }
}

macro_rules! f_write_i32 {
    ($stream:ident, $fields:expr, $field:ident, $value:expr) => {
        if ($fields & $field) != 0 {
            $stream.write_i32::<LittleEndian>($value)?
        }
    }
}

macro_rules! f_write_u32 {
    ($stream:ident, $fields:expr, $field:ident, $value:expr) => {
        if ($fields & $field) != 0 {
            $stream.write_u32::<LittleEndian>($value)?
        }
    }
}

macro_rules! f_write_string {
    ($stream:ident, $fields:expr, $field:ident, $value:expr) => {
        if ($fields & $field) != 0 {
            write_vault_string($stream, &$value)?
        }
    }
}

macro_rules! f_write_uuid {
    ($stream:ident, $fields:expr, $field:ident, $value:expr) => {
        if ($fields & $field) != 0 {
            $value.stream_write($stream)?
        }
    }
}

macro_rules! f_write_blob {
    ($stream:ident, $fields:expr, $field:ident, $value:expr) => {
        if ($fields & $field) != 0 {
            $stream.write_u32::<LittleEndian>($value.len() as u32)?;
            $stream.write_all($value.as_slice())?;
        }
    }
}

impl StreamWrite for VaultNode {
    fn stream_write<S>(&self, stream: &mut S) -> Result<()>
        where S: Write
    {
        stream.write_u64::<LittleEndian>(self.fields)?;

        f_write_u32!(stream, self.fields, FIELD_NODE_IDX, self.node_id);
        f_write_u32!(stream, self.fields, FIELD_CREATE_TIME, self.create_time);
        f_write_u32!(stream, self.fields, FIELD_MODIFY_TIME, self.modify_time);
        f_write_string!(stream, self.fields, FIELD_CREATE_AGE_NAME, self.create_age_name);
        f_write_uuid!(stream, self.fields, FIELD_CREATE_AGE_UUID, self.create_age_uuid);
        f_write_uuid!(stream, self.fields, FIELD_CREATOR_UUID, self.creator_uuid);
        f_write_u32!(stream, self.fields, FIELD_CREATOR_IDX, self.creator_id);
        f_write_i32!(stream, self.fields, FIELD_NODE_TYPE, self.node_type);
        f_write_i32!(stream, self.fields, FIELD_INT32_1, self.int32_1);
        f_write_i32!(stream, self.fields, FIELD_INT32_2, self.int32_2);
        f_write_i32!(stream, self.fields, FIELD_INT32_3, self.int32_3);
        f_write_i32!(stream, self.fields, FIELD_INT32_4, self.int32_4);
        f_write_u32!(stream, self.fields, FIELD_UINT32_1, self.uint32_1);
        f_write_u32!(stream, self.fields, FIELD_UINT32_2, self.uint32_2);
        f_write_u32!(stream, self.fields, FIELD_UINT32_3, self.uint32_3);
        f_write_u32!(stream, self.fields, FIELD_UINT32_4, self.uint32_4);
        f_write_uuid!(stream, self.fields, FIELD_UUID_1, self.uuid_1);
        f_write_uuid!(stream, self.fields, FIELD_UUID_2, self.uuid_2);
        f_write_uuid!(stream, self.fields, FIELD_UUID_3, self.uuid_3);
        f_write_uuid!(stream, self.fields, FIELD_UUID_4, self.uuid_4);
        f_write_string!(stream, self.fields, FIELD_STRING64_1, self.string64_1);
        f_write_string!(stream, self.fields, FIELD_STRING64_2, self.string64_2);
        f_write_string!(stream, self.fields, FIELD_STRING64_3, self.string64_3);
        f_write_string!(stream, self.fields, FIELD_STRING64_4, self.string64_4);
        f_write_string!(stream, self.fields, FIELD_STRING64_5, self.string64_5);
        f_write_string!(stream, self.fields, FIELD_STRING64_6, self.string64_6);
        f_write_string!(stream, self.fields, FIELD_ISTRING64_1, self.istring64_1);
        f_write_string!(stream, self.fields, FIELD_ISTRING64_2, self.istring64_2);
        f_write_string!(stream, self.fields, FIELD_TEXT_1, self.text_1);
        f_write_string!(stream, self.fields, FIELD_TEXT_2, self.text_2);
        f_write_blob!(stream, self.fields, FIELD_BLOB_1, self.blob_1);
        f_write_blob!(stream, self.fields, FIELD_BLOB_2, self.blob_2);

        Ok(())
    }
}
