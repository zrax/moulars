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
#![allow(clippy::uninlined_format_args)]    // Added in Rust 1.66

// TODO: Clean up some or all of these exceptions
#![allow(clippy::if_not_else)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::unreadable_literal)]
#![warn(clippy::cast_possible_truncation)]
#![warn(clippy::cast_sign_loss)]
#![warn(clippy::must_use_candidate)]

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

// Shortcut for generating (optionally formatted) general errors as std::io::Error
#[macro_export]
macro_rules! general_error {
    ($message:literal) => (
        ::std::io::Error::new(::std::io::ErrorKind::Other, $message)
    );
    ($message:literal, $($arg:expr),+) => (
        ::std::io::Error::new(::std::io::ErrorKind::Other, format!($message, $($arg),+))
    );
}
