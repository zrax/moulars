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

// Intentional nonsense...  Maybe these can be reworked later to return the
// access node types with an Into trait.  For simplicity though, they return
// VaultNode directly for now.
#![allow(clippy::new_ret_no_self)]

use std::sync::Arc;

use uuid::Uuid;

use super::vault_node::{VaultNode, NodeType, StandardNode};

macro_rules! vnode_access {
    ($struct_name:ident { $($name:ident: $type:ty => $field:ident),* $(,)? }) => {
        pub struct $struct_name {
            pub(super) node: Arc<VaultNode>
        }
        impl $struct_name {
            pub fn node_id(&self) -> u32 { self.node.node_id() }
            $(pub fn $name(&self) -> $type { self.node.$field() })*
        }
    };
}

vnode_access!(VaultPlayerNode {
    player_name_ci: &str => istring64_1,
    avatar_shape: &str => string64_1,
    disabled: i32 => int32_1,
    explorer: i32 => int32_2,
    online_time: u32 => uint32_1,
    account_id: &Uuid => uuid_1,
    invite_uuid: &Uuid => uuid_2,
});

impl VaultPlayerNode {
    pub fn new(account_id: &Uuid, player_name: &str, avatar_shape: &str,
               explorer: i32) -> VaultNode
    {
        let mut node = VaultNode::default();
        node.set_node_type(NodeType::Player as i32);
        node.set_creator_uuid(account_id);
        node.set_int32_2(explorer);
        node.set_uuid_1(account_id);
        node.set_string64_1(avatar_shape);
        node.set_istring64_1(player_name);
        node
    }
}

vnode_access!(VaultAgeNode {
    age_instance_uuid: &Uuid => uuid_1,
    parent_age_instance_uuid: &Uuid => uuid_2,
    age_name: &str => string64_1,
});

impl VaultAgeNode {
    pub fn new(instance_id: &Uuid, parent_uuid: &Uuid, age_filename: &str) -> VaultNode {
        let mut node = VaultNode::default();
        node.set_node_type(NodeType::Age as i32);
        node.set_creator_uuid(instance_id);
        node.set_uuid_1(instance_id);
        if !parent_uuid.is_nil() {
            node.set_uuid_2(parent_uuid);
        }
        node.set_string64_1(age_filename);
        node
    }

    pub fn new_lookup(instance_id: Option<&Uuid>) -> VaultNode {
        let mut node = VaultNode::default();
        node.set_node_type(NodeType::Age as i32);
        if let Some(uuid) = instance_id {
            node.set_uuid_1(uuid);
        }
        node
    }
}

vnode_access!(VaultFolderNode {
    folder_type: i32 => int32_1,
    folder_name: &str => string64_1,
});

impl VaultFolderNode {
    pub fn new(creator_uuid: &Uuid, creator_id: u32,
               folder_type: StandardNode) -> VaultNode {
        let mut node = VaultNode::default();
        node.set_node_type(NodeType::Folder as i32);
        node.set_creator_uuid(creator_uuid);
        node.set_creator_id(creator_id);
        node.set_int32_1(folder_type as i32);
        node
    }
}

vnode_access!(VaultPlayerInfoNode {
    player_id: u32 => uint32_1,
    player_name_ci: &str => istring64_1,
    age_instance_name: &str => string64_1,
    age_instance_uuid: &Uuid => uuid_1,
    online: i32 => int32_1,
    ccr_level: i32 => int32_2,
});

impl VaultPlayerInfoNode {
    pub fn new(creator_uuid: &Uuid, player_id: u32, player_name: &str) -> VaultNode {
        let mut node = VaultNode::default();
        node.set_node_type(NodeType::PlayerInfo as i32);
        node.set_creator_uuid(creator_uuid);
        node.set_creator_id(player_id);
        node.set_uint32_1(player_id);
        node.set_istring64_1(player_name);
        node
    }

    pub fn new_lookup(online: Option<i32>) -> VaultNode {
        let mut node = VaultNode::default();
        node.set_node_type(NodeType::PlayerInfo as i32);
        if let Some(value) = online {
            node.set_int32_1(value);
        }
        node
    }

    pub fn new_update(node_id: u32, online: i32, age_instance_name: &str,
                      age_instance_uuid: &Uuid) -> VaultNode
    {
        let mut node = VaultNode::default();
        node.set_node_id(node_id);
        node.set_int32_1(online);
        node.set_string64_1(age_instance_name);
        node.set_uuid_1(age_instance_uuid);
        node
    }
}

vnode_access!(VaultSystemNode {
    ccr_status: i32 => int32_1,
});

impl VaultSystemNode {
    pub fn new() -> VaultNode {
        let mut node = VaultNode::default();
        node.set_node_type(NodeType::System as i32);
        node
    }
}

vnode_access!(VaultImageNode {
    image_type: i32 => int32_1,
    image_title: &str => string64_1,
    image_data: &[u8] => blob_1,
});

impl VaultImageNode {
    // pub fn new() -> VaultNode { ... }
}

vnode_access!(VaultTextNoteNode {
    note_type: i32 => int32_1,
    note_subtype: i32 => int32_2,
    note_title: &str => string64_1,
    note_text: &str => text_1,
});

impl VaultTextNoteNode {
    // pub fn new() -> VaultNode { ... }
}

vnode_access!(VaultSdlNode {
    sdl_name: &str => string64_1,
    sdl_ident: i32 => int32_1,
    sdl_data: &[u8] => blob_1,
});

impl VaultSdlNode {
    pub fn new(creator_uuid: &Uuid, creator_id: u32, sdl_name: &str,
               sdl_blob: &[u8]) -> VaultNode {
        let mut node = VaultNode::default();
        node.set_node_type(NodeType::Sdl as i32);
        node.set_creator_uuid(creator_uuid);
        node.set_creator_id(creator_id);
        node.set_string64_1(sdl_name);
        node.set_blob_1(sdl_blob);
        node
    }
}

vnode_access!(VaultAgeLinkNode {
    unlocked: i32 => int32_1,
    volatile: i32 => int32_2,
    spawn_points: &[u8] => blob_1,
});

impl VaultAgeLinkNode {
    pub fn new(creator_uuid: &Uuid, creator_id: u32, link: &[u8]) -> VaultNode {
        let mut node = VaultNode::default();
        node.set_node_type(NodeType::AgeLink as i32);
        node.set_creator_uuid(creator_uuid);
        node.set_creator_id(creator_id);
        node.set_blob_1(link);
        node
    }
}

vnode_access!(VaultChronicleNode {
    entry_type: i32 => int32_1,
    entry_name: &str => string64_1,
    entry_value: &str => text_1,
});

impl VaultChronicleNode {
    // pub fn new() -> VaultNode { ... }
}

vnode_access!(VaultPlayerInfoListNode {
    folder_type: i32 => int32_1,
    folder_name: &str => string64_1,
});

impl VaultPlayerInfoListNode {
    pub fn new(creator_uuid: &Uuid, creator_id: u32,
               folder_type: StandardNode) -> VaultNode {
        let mut node = VaultFolderNode::new(creator_uuid, creator_id, folder_type);
        node.set_node_type(NodeType::PlayerInfoList as i32);
        node
    }
}

vnode_access!(VaultAgeInfoNode {
    age_filename: &str => string64_2,
    age_instance_name: &str => string64_3,
    age_user_defined_name: &str => string64_4,
    age_instance_uuid: &Uuid => uuid_1,
    parent_age_instance_uuid: &Uuid => uuid_2,
    age_description: &str => text_1,
    age_sequence_number: i32 => int32_1,
    age_language: i32 => int32_2,
    age_id: u32 => uint32_1,
    age_czar_id: u32 => uint32_2,
    age_info_flags: u32 => uint32_3,
    is_public: i32 => int32_2,
});

impl VaultAgeInfoNode {
    #[allow(clippy::too_many_arguments)]
    pub fn new(instance_id: &Uuid, age_id: u32, seq_number: i32,
               public: bool, language: i32, parent_uuid: &Uuid,
               age_filename: &str, instance_name: &str, user_name: &str,
               description: &str) -> VaultNode {
        let mut node = VaultNode::default();
        node.set_node_type(NodeType::AgeInfo as i32);
        node.set_creator_uuid(instance_id);
        node.set_creator_id(age_id);
        node.set_int32_1(seq_number);
        node.set_int32_2(i32::from(public));
        node.set_int32_3(language);
        node.set_uint32_1(age_id);
        node.set_uint32_2(0);   // Czar ID
        node.set_uint32_3(0);   // Flags
        node.set_uuid_1(instance_id);
        if !parent_uuid.is_nil() {
            node.set_uuid_2(parent_uuid);
        }
        node.set_string64_2(age_filename);
        if !instance_name.is_empty() {
            node.set_string64_3(instance_name);
        }
        if !user_name.is_empty() {
            node.set_string64_4(user_name);
        }
        if !description.is_empty() {
            node.set_text_1(description);
        }
        node
    }

    pub fn new_lookup(instance_id: Option<&Uuid>) -> VaultNode {
        let mut node = VaultNode::default();
        node.set_node_type(NodeType::AgeInfo as i32);
        if let Some(uuid) = instance_id {
            node.set_uuid_1(uuid);
        }
        node
    }
}

vnode_access!(VaultAgeInfoListNode {
    folder_type: i32 => int32_1,
    folder_name: &str => string64_1,
});

impl VaultAgeInfoListNode {
    pub fn new(creator_uuid: &Uuid, creator_id: u32,
               folder_type: StandardNode) -> VaultNode {
        let mut node = VaultFolderNode::new(creator_uuid, creator_id, folder_type);
        node.set_node_type(NodeType::AgeInfoList as i32);
        node
    }
}

vnode_access!(VaultMarkerGameNode {
    game_name: &str => text_1,
    reward: &str => text_2,
    game_uuid: &Uuid => uuid_1,
});

impl VaultMarkerGameNode {
    // pub fn new() -> VaultNode { ... }
}
