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

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufReader, Result};
use std::path::Path;
use std::sync::Arc;

use log::warn;
use unicase::UniCase;

use crate::plasma::file_crypt::EncryptedReader;
use super::{StateDescriptor, Parser};

type DescriptorMap = HashMap<UniCase<String>, BTreeMap<u32, Arc<StateDescriptor>>>;

pub struct DescriptorDb {
    descriptors: DescriptorMap,
}

fn merge_descriptors(db: &mut DescriptorMap, descriptors: Vec<StateDescriptor>) {
    for desc in descriptors {
        db.entry(UniCase::new(desc.name().clone()))
            .or_insert(BTreeMap::new())
            .entry(desc.version())
            .and_modify(|_| warn!("Duplicate descriptor found for {} version {}",
                                  desc.name(), desc.version()))
            .or_insert(Arc::new(desc));
    }
}

impl DescriptorDb {
    pub fn from_dir(path: &Path, key: &[u32; 4]) -> Result<Self> {
        let mut descriptors = DescriptorMap::new();
        for entry in path.read_dir()? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_file() && entry.path().extension() == Some(OsStr::new("sdl")) {
                let file_reader = BufReader::new(File::open(entry.path())?);
                let stream = BufReader::new(EncryptedReader::new(file_reader, key)?);
                let mut parser = Parser::new(stream);
                merge_descriptors(&mut descriptors, parser.parse()?);
            }
        }

        Ok(Self { descriptors })
    }

    pub fn get_version(&self, name: &str, version: u32) -> Option<Arc<StateDescriptor>> {
        if let Some(ver_map) = self.descriptors.get(&UniCase::new(name.to_string())) {
            ver_map.get(&version).cloned()
        } else {
            None
        }
    }

    pub fn get_latest(&self, name: &str) -> Option<Arc<StateDescriptor>> {
        if let Some(ver_map) = self.descriptors.get(&UniCase::new(name.to_string())) {
            ver_map.iter().next_back().map(|(_, desc)| desc).cloned()
        } else {
            None
        }
    }

    pub fn descriptor_names(&self) -> Vec<&str> {
        self.descriptors.keys().map(|k| k.as_str()).collect()
    }
}
