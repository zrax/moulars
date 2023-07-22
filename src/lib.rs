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

pub mod auth_srv;
pub mod file_srv;
pub mod gate_keeper;
pub mod plasma;
pub mod vault;

pub mod config;
pub mod lobby;
pub mod net_crypt;
pub mod netcli;

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
