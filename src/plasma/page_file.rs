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

use std::collections::HashMap;
use std::io::{BufRead, Seek, SeekFrom, Cursor, Result};
use std::sync::Arc;

use byteorder::{LittleEndian, ReadBytesExt};

use crate::general_error;
use crate::plasma::{Creatable, StreamRead};
use crate::plasma::key::{Uoid, Location};
use crate::plasma::safe_string::{read_safe_str, StringFormat};

// We don't need to do any processing on these, but we do need to be able to
// extract some objects (e.g. Sound Buffers) in order to determine the correct
// flags to use for manifest generation.
pub struct PageFile {
    location: Location,
    age_name: String,
    page_name: String,
    page_version: u16,
    key_index: HashMap<u16, Vec<IndexKey>>,
}

struct IndexKey {
    uoid: Arc<Uoid>,
    offset: u32,
    size: u32,
}

impl PageFile {
    pub fn read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead + Seek
    {
        let page_version = stream.read_u32::<LittleEndian>()?;
        if page_version != 6 {
            return Err(general_error!("Unexpected page version {}", page_version));
        }
        let location = Location::stream_read(stream)?;
        let age_name = read_safe_str(stream, StringFormat::Utf8)?;
        let page_name = read_safe_str(stream, StringFormat::Utf8)?;
        let page_version = stream.read_u16::<LittleEndian>()?;
        let _ = stream.read_u32::<LittleEndian>()?;     // Checksum
        let _ = stream.read_u32::<LittleEndian>()?;     // Data start
        let index_start = stream.read_u32::<LittleEndian>()?;

        // Skip to and read the object index
        stream.seek(SeekFrom::Start(index_start as u64))?;
        let num_types = stream.read_u32::<LittleEndian>()?;
        let mut key_index = HashMap::with_capacity(num_types as usize);
        for _ in 0..num_types {
            let class_id = stream.read_u16::<LittleEndian>()?;
            let _ = stream.read_u32::<LittleEndian>()?;   // Size of this sub-list in bytes
            let _ = stream.read_u8()?;                    // Flags -- ignored

            let num_keys = stream.read_u32::<LittleEndian>()?;
            let mut key_list = Vec::with_capacity(num_keys as usize);
            for _ in 0..num_keys {
                key_list.push(IndexKey::stream_read(stream)?);
            }
            let _ = key_index.insert(class_id, key_list);
        }

        Ok(PageFile { location, age_name, page_name, page_version, key_index })
    }

    pub fn location(&self) -> &Location { &self.location }
    pub fn age_name(&self) -> &String { &self.age_name }
    pub fn page_name(&self) -> &String { &self.page_name }
    pub fn page_version(&self) -> u16 { self.page_version }

    pub fn has_keys(&self, class_id: u16) -> bool {
        match self.key_index.get(&class_id) {
            Some(keys) => !keys.is_empty(),
            None => false,
        }
    }

    pub fn get_keys(&self, class_id: u16) -> Vec<Arc<Uoid>> {
        match self.key_index.get(&class_id) {
            Some(keys) => keys.iter().map(|key| key.uoid.clone()).collect(),
            None => Vec::new(),
        }
    }

    pub fn read_object<S, ObType>(&self, stream: &mut S, uoid: &Uoid) -> Result<ObType>
        where S: BufRead + Seek,
              ObType: Creatable
    {
        assert_eq!(ObType::static_class_id(), uoid.obj_type());

        if let Some(keys) = self.key_index.get(&uoid.obj_type()) {
            if let Some(index_key) = keys.iter().find(|k| k.uoid.as_ref() == uoid) {
                let _ = stream.seek(SeekFrom::Start(index_key.offset as u64))?;

                // Ensure the object is streamed from within the bounds of the stored data
                let mut obj_buffer = vec![0; index_key.size as usize];
                stream.read_exact(&mut obj_buffer)?;
                let mut obj_stream = Cursor::new(obj_buffer);

                let stream_class = obj_stream.read_u16::<LittleEndian>()?;
                if stream_class != ObType::static_class_id() {
                    return Err(general_error!("Unexpected class ID 0x{:04x} encountered",
                                              stream_class));
                }
                return ObType::stream_read(&mut obj_stream)
            }
        }
        Err(general_error!("Could not find object {:?} in this page file", uoid))
    }
}

impl StreamRead for IndexKey {
    fn stream_read<S>(stream: &mut S) -> Result<Self>
        where S: BufRead
    {
        let uoid = Uoid::stream_read(stream)?;
        let offset = stream.read_u32::<LittleEndian>()?;
        let size = stream.read_u32::<LittleEndian>()?;
        Ok(Self { uoid: Arc::new(uoid), offset, size })
    }
}
