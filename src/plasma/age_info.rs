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

use std::fs::File;
use std::io::{BufRead, BufReader, Result};
use std::path::Path;

use crate::general_error;
use super::file_crypt::{self, EncryptedReader};
use super::UnifiedTime;

#[derive(Default)]
pub struct PageInfo {
    name: String,
    seq_suffix: u32,
    flags: u32,
}

#[derive(Default)]
pub struct AgeInfo {
    start_date_time: UnifiedTime,
    day_length: f32,
    max_capacity: u32,
    linger_time: u32,
    seq_prefix: i32,
    release_version: u32,
    pages: Vec<PageInfo>,
}

impl PageInfo {
    // Page flags
    pub const NO_AUTO_LOAD: u32         = 1 << 0;
    pub const LOAD_SDL_IF_PRESENT: u32  = 1 << 1;
    pub const LOCAL_ONLY: u32           = 1 << 2;
    pub const VOLATILE: u32             = 1 << 3;

    pub fn name(&self) -> &String { &self.name }
    pub fn seq_suffix(&self) -> u32 { self.seq_suffix }
    pub fn flags(&self) -> u32 { self.flags }
}

impl AgeInfo {
    pub fn from_file(path: &Path) -> Result<Self> {
        // Anything not specified gets defaults
        let mut info = AgeInfo::default();

        // The EncryptedReader will also read from unencrypted files
        let file_reader = BufReader::new(File::open(path)?);
        let reader = BufReader::new(EncryptedReader::new(file_reader, &file_crypt::DEFAULT_KEY)?);
        for line in reader.lines() {
            let line = line?;
            let line = line.split('#').next().unwrap();
            if line.is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.splitn(2, '=').collect();
            if parts.len() != 2 {
                return Err(general_error!("Malformed line in .age file: {}", line));
            }
            if parts[0].eq_ignore_ascii_case("StartDateTime") {
                let value = parts[1].parse::<u32>()
                        .map_err(|_| general_error!("Invalid StartDateTime value: {}", parts[1]))?;
                info.start_date_time = UnifiedTime::from_secs(value);
            } else if parts[0].eq_ignore_ascii_case("DayLength") {
                let value = parts[1].parse::<f32>()
                        .map_err(|_| general_error!("Invalid DayLength value: {}", parts[1]))?;
                info.day_length = value;
            } else if parts[0].eq_ignore_ascii_case("MaxCapacity") {
                let value = parts[1].parse::<u32>()
                        .map_err(|_| general_error!("Invalid MaxCapacity value: {}", parts[1]))?;
                info.max_capacity = value;
            } else if parts[0].eq_ignore_ascii_case("LingerTime") {
                let value = parts[1].parse::<u32>()
                        .map_err(|_| general_error!("Invalid LingerTime value: {}", parts[1]))?;
                info.linger_time = value;
            } else if parts[0].eq_ignore_ascii_case("SequencePrefix") {
                let value = parts[1].parse::<i32>()
                        .map_err(|_| general_error!("Invalid SequencePrefix value: {}", parts[1]))?;
                info.seq_prefix = value;
            } else if parts[0].eq_ignore_ascii_case("ReleaseVersion") {
                let value = parts[1].parse::<u32>()
                        .map_err(|_| general_error!("Invalid ReleaseVersion value: {}", parts[1]))?;
                info.release_version = value;
            } else if parts[0].eq_ignore_ascii_case("Page") {
                let page_parts: Vec<&str> = parts[1].split(',').collect();
                let name = page_parts[0];
                let seq_suffix_str = *page_parts.get(1).unwrap_or(&"0");
                let seq_suffix = seq_suffix_str.parse::<u32>()
                        .map_err(|_| general_error!("Invalid Page sequence: {}", seq_suffix_str))?;
                let flags_str = *page_parts.get(2).unwrap_or(&"0");
                let flags = flags_str.parse::<u32>()
                        .map_err(|_| general_error!("Invalid Page flags: {}", flags_str))?;
                info.pages.push(PageInfo { name: name.to_string(), seq_suffix, flags })
            } else {
                return Err(general_error!("Invalid AgeInfo line: {}", line));
            }
        }

        Ok(info)
    }

    pub fn pages(&self) -> &Vec<PageInfo> { &self.pages }
}
