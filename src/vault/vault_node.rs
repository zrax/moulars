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

use std::fmt::{Debug, Formatter};
use std::io::{BufRead, Write, Cursor};
use std::mem::size_of;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use data_encoding::HEXLOWER;
use paste::paste;
use uuid::Uuid;

use crate::plasma::{StreamRead, StreamWrite};
use super::vnode_access::{
    VaultPlayerNode, VaultAgeNode, VaultFolderNode, VaultPlayerInfoNode,
    VaultSystemNode, VaultImageNode, VaultTextNoteNode, VaultSdlNode,
    VaultAgeLinkNode, VaultChronicleNode, VaultPlayerInfoListNode,
    VaultAgeInfoNode, VaultAgeInfoListNode, VaultMarkerGameNode
};

#[repr(i32)]
pub enum NodeType {
    _Invalid,
    _VNodeMgrLow, Player, Age,
    _VNodeMgrHigh = 21,
    Folder, PlayerInfo, System, Image, TextNote, Sdl, AgeLink, Chronicle,
    PlayerInfoList, _Unused01, _Unused02, AgeInfo, AgeInfoList, MarkerGame,
}

#[repr(i32)]
pub enum StandardNode {
    UserDefined, InboxFolder, BuddyListFolder, IgnoreListFolder,
    PeopleIKnowAboutFolder, VaultMgrGlobalDataFolder, ChronicleFolder,
    AvatarOutfitFolder, AgeTypeJournalFolder, SubAgesFolder, DeviceInboxFolder,
    HoodMembersFolder, AllPlayersFolder, AgeMembersFolder, AgeJournalsFolder,
    AgeDevicesFolder, AgeInstanceSDLNode, AgeGlobalSDLNode, CanVisitFolder,
    AgeOwnersFolder, AllAgeGlobalSDLNodesFolder, PlayerInfoNode,
    PublicAgesFolder, AgesIOwnFolder, AgesICanVisitFolder, AvatarClosetFolder,
    AgeInfoNode, SystemNode, PlayerInviteFolder, CCRPlayersFolder,
    GlobalInboxFolder, ChildAgesFolder, GameScoresFolder,
}

#[derive(Clone, Default)]
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

macro_rules! node_field {
    ($field_name:ident, String) => {
        paste! {
            pub fn $field_name(&self) -> &String {
                &self.$field_name
            }
            pub fn [<has_ $field_name>](&self) -> bool {
                (self.fields & [<FIELD_ $field_name:upper>]) != 0
            }
            pub fn [<set_ $field_name>](&mut self, value: &str) {
                self.fields |= [<FIELD_ $field_name:upper>];
                self.$field_name = value.to_string();
            }
        }
    };
    ($field_name:ident, Uuid) => {
        paste! {
            pub fn $field_name(&self) -> &Uuid {
                &self.$field_name
            }
            pub fn [<has_ $field_name>](&self) -> bool {
                (self.fields & [<FIELD_ $field_name:upper>]) != 0
            }
            pub fn [<set_ $field_name>](&mut self, value: &Uuid) {
                self.fields |= [<FIELD_ $field_name:upper>];
                self.$field_name = *value;
            }
        }
    };
    ($field_name:ident, Vec<u8>) => {
        paste! {
            pub fn $field_name(&self) -> &Vec<u8> {
                &self.$field_name
            }
            pub fn [<has_ $field_name>](&self) -> bool {
                (self.fields & [<FIELD_ $field_name:upper>]) != 0
            }
            pub fn [<set_ $field_name>](&mut self, value: &[u8]) {
                self.fields |= [<FIELD_ $field_name:upper>];
                self.$field_name = value.to_vec();
            }
        }
    };
    ($field_name:ident, $value_type:ty) => {
        paste! {
            pub fn $field_name(&self) -> $value_type {
                self.$field_name
            }
            pub fn [<has_ $field_name>](&self) -> bool {
                (self.fields & [<FIELD_ $field_name:upper>]) != 0
            }
            pub fn [<set_ $field_name>](&mut self, value: $value_type) {
                self.fields |= [<FIELD_ $field_name:upper>];
                self.$field_name = value;
            }
        }
    };
}

impl VaultNode {
    node_field!(node_id, u32);
    node_field!(create_time, u32);
    node_field!(modify_time, u32);
    node_field!(create_age_name, String);
    node_field!(create_age_uuid, Uuid);
    node_field!(creator_uuid, Uuid);
    node_field!(creator_id, u32);
    node_field!(node_type, i32);
    node_field!(int32_1, i32);
    node_field!(int32_2, i32);
    node_field!(int32_3, i32);
    node_field!(int32_4, i32);
    node_field!(uint32_1, u32);
    node_field!(uint32_2, u32);
    node_field!(uint32_3, u32);
    node_field!(uint32_4, u32);
    node_field!(uuid_1, Uuid);
    node_field!(uuid_2, Uuid);
    node_field!(uuid_3, Uuid);
    node_field!(uuid_4, Uuid);
    node_field!(string64_1, String);
    node_field!(string64_2, String);
    node_field!(string64_3, String);
    node_field!(string64_4, String);
    node_field!(string64_5, String);
    node_field!(string64_6, String);
    node_field!(istring64_1, String);
    node_field!(istring64_2, String);
    node_field!(text_1, String);
    node_field!(text_2, String);
    node_field!(blob_1, Vec<u8>);
    node_field!(blob_2, Vec<u8>);

    pub fn as_player_node(self: &Arc<Self>) -> Option<VaultPlayerNode> {
        if self.node_type == NodeType::Player as i32 {
            Some(VaultPlayerNode { node: self.clone() })
        } else {
            None
        }
    }

    pub fn as_age_node(self: &Arc<Self>) -> Option<VaultAgeNode> {
        if self.node_type == NodeType::Age as i32 {
            Some(VaultAgeNode { node: self.clone() })
        } else {
            None
        }
    }

    pub fn as_folder_node(self: &Arc<Self>) -> Option<VaultFolderNode> {
        if self.node_type == NodeType::Folder as i32
                || self.node_type == NodeType::PlayerInfoList as i32
                || self.node_type == NodeType::AgeInfoList as i32 {
            Some(VaultFolderNode { node: self.clone() })
        } else {
            None
        }
    }

    pub fn as_player_info_node(self: &Arc<Self>) -> Option<VaultPlayerInfoNode> {
        if self.node_type == NodeType::PlayerInfo as i32 {
            Some(VaultPlayerInfoNode { node: self.clone() })
        } else {
            None
        }
    }

    pub fn as_system_node(self: &Arc<Self>) -> Option<VaultSystemNode> {
        if self.node_type == NodeType::System as i32 {
            Some(VaultSystemNode { node: self.clone() })
        } else {
            None
        }
    }

    pub fn as_image_node(self: &Arc<Self>) -> Option<VaultImageNode> {
        if self.node_type == NodeType::Image as i32 {
            Some(VaultImageNode { node: self.clone() })
        } else {
            None
        }
    }

    pub fn as_text_note_node(self: &Arc<Self>) -> Option<VaultTextNoteNode> {
        if self.node_type == NodeType::TextNote as i32 {
            Some(VaultTextNoteNode { node: self.clone() })
        } else {
            None
        }
    }

    pub fn as_sdl_node(self: &Arc<Self>) -> Option<VaultSdlNode> {
        if self.node_type == NodeType::Sdl as i32 {
            Some(VaultSdlNode { node: self.clone() })
        } else {
            None
        }
    }

    pub fn as_age_link_node(self: &Arc<Self>) -> Option<VaultAgeLinkNode> {
        if self.node_type == NodeType::AgeLink as i32 {
            Some(VaultAgeLinkNode { node: self.clone() })
        } else {
            None
        }
    }

    pub fn as_chronicle_node(self: &Arc<Self>) -> Option<VaultChronicleNode> {
        if self.node_type == NodeType::Chronicle as i32 {
            Some(VaultChronicleNode { node: self.clone() })
        } else {
            None
        }
    }

    pub fn as_player_info_list_node(self: &Arc<Self>) -> Option<VaultPlayerInfoListNode> {
        if self.node_type == NodeType::PlayerInfoList as i32 {
            Some(VaultPlayerInfoListNode { node: self.clone() })
        } else {
            None
        }
    }

    pub fn as_age_info_node(self: &Arc<Self>) -> Option<VaultAgeInfoNode> {
        if self.node_type == NodeType::AgeInfo as i32 {
            Some(VaultAgeInfoNode { node: self.clone() })
        } else {
            None
        }
    }

    pub fn as_age_info_list_node(self: &Arc<Self>) -> Option<VaultAgeInfoListNode> {
        if self.node_type == NodeType::AgeInfoList as i32 {
            Some(VaultAgeInfoListNode { node: self.clone() })
        } else {
            None
        }
    }

    pub fn as_marker_game_node(self: &Arc<Self>) -> Option<VaultMarkerGameNode> {
        if self.node_type == NodeType::MarkerGame as i32 {
            Some(VaultMarkerGameNode { node: self.clone() })
        } else {
            None
        }
    }

    pub fn from_blob(blob: &[u8]) -> Result<Self> {
        let mut stream = Cursor::new(blob);
        Self::stream_read(&mut stream)
    }

    pub fn to_blob(&self) -> Result<Vec<u8>> {
        let mut stream = Cursor::new(Vec::new());
        self.stream_write(&mut stream)?;
        Ok(stream.into_inner())
    }
}

const FIELD_NODE_ID: u64            = 1 << 0;
const FIELD_CREATE_TIME: u64        = 1 << 1;
const FIELD_MODIFY_TIME: u64        = 1 << 2;
const FIELD_CREATE_AGE_NAME: u64    = 1 << 3;
const FIELD_CREATE_AGE_UUID: u64    = 1 << 4;
const FIELD_CREATOR_UUID: u64       = 1 << 5;
const FIELD_CREATOR_ID: u64         = 1 << 6;
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

macro_rules! debug_field {
    ($fmt:ident, $fields:ident, $field_name:ident, $value:expr) => {
        paste! {
            if ($fields & [<FIELD_ $field_name:upper>]) != 0 {
                $fields &= ![<FIELD_ $field_name:upper>];
                if $fields != 0 {
                    write!($fmt, " {}: {},", stringify!($field_name), $value)?;
                } else {
                    write!($fmt, " {}: {}", stringify!($field_name), $value)?;
                }
            }
        }
    }
}

impl Debug for VaultNode {
    // Simplify the default debug format by using the fields mask to make
    // nodes easier to read
    fn fmt(&self, fmt: &mut Formatter) -> std::fmt::Result {
        write!(fmt, "VaultNode {{")?;
        let mut fields = self.fields;
        debug_field!(fmt, fields, node_id, self.node_id);
        debug_field!(fmt, fields, create_time, self.create_time);
        debug_field!(fmt, fields, modify_time, self.modify_time);
        debug_field!(fmt, fields, create_age_name, self.create_age_name);
        debug_field!(fmt, fields, create_age_uuid, self.create_age_uuid);
        debug_field!(fmt, fields, creator_uuid, self.creator_uuid);
        debug_field!(fmt, fields, creator_id, self.creator_id);
        debug_field!(fmt, fields, node_type, self.node_type);
        debug_field!(fmt, fields, int32_1, self.int32_1);
        debug_field!(fmt, fields, int32_2, self.int32_2);
        debug_field!(fmt, fields, int32_3, self.int32_3);
        debug_field!(fmt, fields, int32_4, self.int32_4);
        debug_field!(fmt, fields, uint32_1, self.uint32_1);
        debug_field!(fmt, fields, uint32_2, self.uint32_2);
        debug_field!(fmt, fields, uint32_3, self.uint32_3);
        debug_field!(fmt, fields, uint32_4, self.uint32_4);
        debug_field!(fmt, fields, uuid_1, self.uuid_1);
        debug_field!(fmt, fields, uuid_2, self.uuid_2);
        debug_field!(fmt, fields, uuid_3, self.uuid_3);
        debug_field!(fmt, fields, uuid_4, self.uuid_4);
        debug_field!(fmt, fields, string64_1, self.string64_1);
        debug_field!(fmt, fields, string64_2, self.string64_2);
        debug_field!(fmt, fields, string64_3, self.string64_3);
        debug_field!(fmt, fields, string64_4, self.string64_4);
        debug_field!(fmt, fields, string64_5, self.string64_5);
        debug_field!(fmt, fields, string64_6, self.string64_6);
        debug_field!(fmt, fields, istring64_1, self.istring64_1);
        debug_field!(fmt, fields, istring64_2, self.istring64_2);
        debug_field!(fmt, fields, text_1, self.text_1);
        debug_field!(fmt, fields, text_2, self.text_2);
        debug_field!(fmt, fields, blob_1, HEXLOWER.encode(&self.blob_1));
        debug_field!(fmt, fields, blob_2, HEXLOWER.encode(&self.blob_2));
        write!(fmt, " }}")
    }
}

// Strings in vault nodes use UTF-16, but store the number of BYTES taken up
// by the string, including the terminating nul character.
fn read_vault_string<S>(stream: &mut S) -> Result<String>
    where S: BufRead
{
    let size = stream.read_u32::<LittleEndian>()? as usize;
    if (size % size_of::<u16>()) != 0 || size < size_of::<u16>() {
        return Err(anyhow!("Bad UTF-16 data size ({} bytes)", size));
    }
    let mut buffer = vec![0; (size - 1) / size_of::<u16>()];
    stream.read_u16_into::<LittleEndian>(&mut buffer)?;
    if stream.read_u16::<LittleEndian>()? != 0 {
        return Err(anyhow!("Vault string was not nul-terminated"));
    }

    Ok(String::from_utf16_lossy(&buffer))
}

fn write_vault_string(stream: &mut dyn Write, value: &str) -> Result<()> {
    let buffer: Vec<u16> = value.encode_utf16().collect();
    let buffer_size = u32::try_from((buffer.len() + 1) * size_of::<u16>())
            .context("Buffer too large for stream")?;
    stream.write_u32::<LittleEndian>(buffer_size)?;
    for ch in buffer {
        stream.write_u16::<LittleEndian>(ch)?;
    }
    Ok(stream.write_u16::<LittleEndian>(0)?)
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
        #![allow(clippy::similar_names)]

        let fields = stream.read_u64::<LittleEndian>()?;

        let node_id = f_read_u32!(stream, fields, FIELD_NODE_ID);
        let create_time = f_read_u32!(stream, fields, FIELD_CREATE_TIME);
        let modify_time = f_read_u32!(stream, fields, FIELD_MODIFY_TIME);
        let create_age_name = f_read_string!(stream, fields, FIELD_CREATE_AGE_NAME);
        let create_age_uuid = f_read_uuid!(stream, fields, FIELD_CREATE_AGE_UUID);
        let creator_uuid = f_read_uuid!(stream, fields, FIELD_CREATOR_UUID);
        let creator_id = f_read_u32!(stream, fields, FIELD_CREATOR_ID);
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
            $stream.write_u32::<LittleEndian>(u32::try_from($value.len())
                    .context("Blob too large for stream")?)?;
            $stream.write_all($value.as_slice())?;
        }
    }
}

impl StreamWrite for VaultNode {
    fn stream_write(&self, stream: &mut dyn Write) -> Result<()> {
        stream.write_u64::<LittleEndian>(self.fields)?;

        f_write_u32!(stream, self.fields, FIELD_NODE_ID, self.node_id);
        f_write_u32!(stream, self.fields, FIELD_CREATE_TIME, self.create_time);
        f_write_u32!(stream, self.fields, FIELD_MODIFY_TIME, self.modify_time);
        f_write_string!(stream, self.fields, FIELD_CREATE_AGE_NAME, self.create_age_name);
        f_write_uuid!(stream, self.fields, FIELD_CREATE_AGE_UUID, self.create_age_uuid);
        f_write_uuid!(stream, self.fields, FIELD_CREATOR_UUID, self.creator_uuid);
        f_write_u32!(stream, self.fields, FIELD_CREATOR_ID, self.creator_id);
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
