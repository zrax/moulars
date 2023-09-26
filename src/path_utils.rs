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

use std::ffi::OsStr;
use std::path::{Path, PathBuf, MAIN_SEPARATOR};

// Rust makes this surprisingly difficult...
pub fn append_extension(path: impl AsRef<Path>, ext: impl AsRef<OsStr>) -> PathBuf {
    let path = path.as_ref();
    let ext = ext.as_ref();

    if ext.is_empty() {
        return path.to_owned();
    }

    match path.extension() {
        Some(cur_ext) => {
            let mut new_ext = cur_ext.to_os_string();
            new_ext.push(".");
            new_ext.push(ext);
            path.with_extension(new_ext)
        }
        None => path.with_extension(ext)
    }
}

#[test]
fn test_append_extension() {
    assert_eq!(append_extension(Path::new("/foo/bar"), "gz"), Path::new("/foo/bar.gz"));
    assert_eq!(append_extension(Path::new("/foo/bar.exe"), "gz"), Path::new("/foo/bar.exe.gz"));
    assert_eq!(append_extension(Path::new("bar"), "gz"), Path::new("bar.gz"));
    assert_eq!(append_extension(Path::new("bar.exe"), "gz"), Path::new("bar.exe.gz"));
    assert_eq!(append_extension(Path::new("/foo/bar"), ""), Path::new("/foo/bar"));
    assert_eq!(append_extension(Path::new("/foo/bar.exe"), ""), Path::new("/foo/bar.exe"));
}

#[must_use]
pub fn to_windows(path: &str) -> String {
    debug_assert!(MAIN_SEPARATOR.is_ascii());

    let mut result = path.to_string();
    // SAFETY: We are only replacing one ASCII character with another.  The
    // size and UTF-8 validity of the returned string are not affected.
    if MAIN_SEPARATOR != '\\' {
        unsafe {
            for ch in result.as_bytes_mut() {
                if *ch == MAIN_SEPARATOR as u8 {
                    *ch = b'\\';
                }
            }
        }
    }
    result
}

#[must_use]
pub fn to_native(path: &str) -> String {
    debug_assert!(MAIN_SEPARATOR.is_ascii());

    let mut result = path.to_string();
    // SAFETY: We are only replacing one ASCII character with another.  The
    // size and UTF-8 validity of the returned string are not affected.
    if MAIN_SEPARATOR != '\\' {
        unsafe {
            for ch in result.as_bytes_mut() {
                if *ch == b'\\' {
                    *ch = MAIN_SEPARATOR as u8;
                }
            }
        }
    }
    result
}
