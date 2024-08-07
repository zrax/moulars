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

#![deny(clippy::all)]
#![deny(clippy::pedantic)]

// TODO: Clean up some or all of these exceptions
#![allow(clippy::if_not_else)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::unreadable_literal)]

pub mod auth_srv;
pub mod file_srv;
pub mod gate_keeper;
pub mod plasma;
pub mod sdl;
pub mod vault;

pub mod api;
pub mod config;
pub mod hashes;
pub mod lobby;
pub mod net_crypt;
pub mod netcli;
pub mod path_utils;
