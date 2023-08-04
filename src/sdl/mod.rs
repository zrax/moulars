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

mod descriptor;
pub use descriptor::{VarType, VarDefault, VarDescriptor, StateDescriptor};

mod descriptor_db;
pub use descriptor_db::DescriptorDb;

mod parser;
pub use parser::Parser;

mod state;
pub use state::State;

mod state_variable;
pub use state_variable::Variable;

// Read/Write flags
const HAS_UOID: u16             = 1 << 0;
const VAR_LENGTH_IO: u16        = 1 << 15;

const HAS_NOTIFICATION_INFO: u8 = 1 << 1;
const HAS_TIMESTAMP: u8         = 1 << 2;
const SAME_AS_DEFAULT: u8       = 1 << 3;
const HAS_DIRTY_FLAG: u8        = 1 << 4;
const WANT_TIMESTAMP: u8        = 1 << 5;
