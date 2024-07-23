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

use std::io::{Cursor, Write};

use anyhow::Result;
use byteorder::{LittleEndian, WriteBytesExt};
use once_cell::sync::Lazy;
use regex::Regex;

use crate::hashes::ShaDigest;
use crate::plasma::StreamWrite;

fn write_truncated_utf16(value: &str, stream: &mut dyn Write) -> Result<()> {
    let wide_value: Vec<u16> = value.encode_utf16().collect();
    if !wide_value.is_empty() {
        for ch in wide_value.iter().take(wide_value.len() - 1) {
            stream.write_u16::<LittleEndian>(*ch)?;
        }
        stream.write_u16::<LittleEndian>(0)?;
    }
    Ok(())
}

pub fn use_email_auth(account_name: &str) -> bool {
    static RE_DOMAIN: Lazy<Regex> = Lazy::new(|| {
        Regex::new("[^@]+@([^.]+\\.)*([^.]+)\\.[^.]+").unwrap()
    });

    if let Some(caps) = RE_DOMAIN.captures(account_name) {
        !caps[2].eq_ignore_ascii_case("gametap")
    } else {
        false
    }
}

pub fn create_pass_hash(account_name: &str, password: &str) -> Result<ShaDigest> {
    if use_email_auth(account_name) {
        // This is the broken hash mechanism which truncates one character
        // from both the account name and the password.
        let mut buffer = Cursor::new(Vec::new());
        write_truncated_utf16(password, &mut buffer)?;
        write_truncated_utf16(account_name, &mut buffer)?;
        Ok(ShaDigest::sha0(&buffer.into_inner()))
    } else {
        // Just store a SHA-1 hash of the password
        Ok(ShaDigest::sha1(password.as_bytes()))
    }
}

pub fn hash_password_challenge(client_challenge: u32, server_challenge: u32,
                               pass_hash: ShaDigest) -> Result<ShaDigest>
{
    let mut buffer = Cursor::new(Vec::new());
    buffer.write_u32::<LittleEndian>(client_challenge)?;
    buffer.write_u32::<LittleEndian>(server_challenge)?;
    pass_hash.stream_write(&mut buffer)?;
    Ok(ShaDigest::sha0(&buffer.into_inner()))
}
