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

mod message;
pub use message::{Message, NetSafety};

mod anim_cmd_msg;
pub use anim_cmd_msg::AnimCmdMsg;

mod linking_mgr_msg;
pub use linking_mgr_msg::LinkingMgrMsg;

mod message_with_callbacks;
pub use message_with_callbacks::MessageWithCallbacks;
