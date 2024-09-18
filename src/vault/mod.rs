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

mod db_interface;
pub use db_interface::{PlayerInfo, GameServer};

mod db_memory;

pub mod messages;

mod node_ref;
pub use node_ref::NodeRef;

mod server;
pub use server::VaultServer;

mod vault_node;
pub use vault_node::{VaultNode, StandardNode};

mod vnode_access;
pub use vnode_access::{
    VaultPlayerNode, VaultAgeNode, VaultFolderNode, VaultPlayerInfoNode,
    VaultSystemNode, VaultImageNode, VaultTextNoteNode, VaultSdlNode,
    VaultAgeLinkNode, VaultChronicleNode, VaultPlayerInfoListNode,
    VaultAgeInfoNode, VaultAgeInfoListNode, VaultMarkerGameNode
};
