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

type DescriptorMap = HashMap<UniCase<String>, BTreeMap<u16, Arc<StateDescriptor>>>;

pub struct DescriptorDb {
    descriptors: DescriptorMap,
}

fn merge_descriptors(db: &mut DescriptorMap, descriptors: Vec<StateDescriptor>) {
    for desc in descriptors {
        db.entry(UniCase::new(desc.name().clone())).or_default()
            .entry(desc.version())
            .and_modify(|_| warn!("Duplicate descriptor found for {} version {}",
                                  desc.name(), desc.version()))
            .or_insert_with(|| Arc::new(desc));
    }
}

impl DescriptorDb {
    pub fn empty() -> Self {
        Self { descriptors: HashMap::new() }
    }

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

    #[cfg(test)]
    pub fn from_string(input: &str) -> Result<Self> {
        let mut descriptors = DescriptorMap::new();
        let stream = std::io::Cursor::new(input);
        let mut parser = Parser::new(stream);
        merge_descriptors(&mut descriptors, parser.parse()?);
        Ok(Self { descriptors })
    }

    pub fn get_version(&self, name: &str, version: u16) -> Option<Arc<StateDescriptor>> {
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

#[cfg(test)]
pub(super) const TEST_DESCRIPTORS: &str = r#"
    STATEDESC Test
    {
        VERSION 1

        VAR BOOL    bTestVar1[1]    DEFAULT=0
        VAR BOOL    bTestVar2[1]    DEFAULT=1    DEFAULTOPTION=VAULT
        VAR INT     iTestVar3[1]    DEFAULT=0    DEFAULTOPTION=VAULT
        VAR INT     iTestVar4[1]    DEFAULT=100
    }

    STATEDESC Test
    {
        VERSION 2

        VAR BOOL    bTestVar1[1]    DEFAULT=0
        VAR BOOL    bTestVar2[1]    DEFAULT=1    DEFAULTOPTION=VAULT
        VAR INT     iTestVar3[1]    DEFAULT=0    DEFAULTOPTION=VAULT
        VAR INT     iTestVar4[1]    DEFAULT=100

        VAR BYTE    bTestVar5[1]    DEFAULT=50
    }

    STATEDESC Barney
    {
        VERSION 1
    }
"#;

#[cfg(test)]
macro_rules! check_var_descriptor {
    ($statedesc:ident, $var_name:literal, $var_type:expr, $default:expr) => {
        let var_desc = $statedesc.get_var($var_name)
                .expect(concat!("Could not find variable ", $var_name, " in Descriptor"));
        assert_eq!(var_desc.name(), $var_name);
        assert_eq!(var_desc.var_type(), &$var_type);
        assert_eq!(var_desc.default(), Some(&$default));
    };
}

#[test]
fn test_descriptors() -> Result<()> {
    use super::{VarType, VarDefault};

    let _ = env_logger::builder().is_test(true).filter_level(log::LevelFilter::Debug)
                .format_timestamp(None).format_target(false).try_init();

    let db = DescriptorDb::from_string(TEST_DESCRIPTORS)?;
    let v1 = db.get_version("Test", 1).expect("Could not get StateDesc Test v1");
    let v2 = db.get_version("Test", 2).expect("Could not get StateDesc Test v2");

    assert_eq!(v1.version(), 1);
    assert_eq!(v1.name().as_str(), "Test");
    assert_eq!(v1.vars().len(), 4);
    check_var_descriptor!(v1, "bTestVar1", VarType::Bool, VarDefault::Bool(false));
    check_var_descriptor!(v1, "bTestVar2", VarType::Bool, VarDefault::Bool(true));
    check_var_descriptor!(v1, "iTestVar3", VarType::Int, VarDefault::Int(0));
    check_var_descriptor!(v1, "iTestVar4", VarType::Int, VarDefault::Int(100));

    assert_eq!(v2.version(), 2);
    assert_eq!(v2.name().as_str(), "Test");
    assert_eq!(v2.vars().len(), 5);
    check_var_descriptor!(v2, "bTestVar1", VarType::Bool, VarDefault::Bool(false));
    check_var_descriptor!(v2, "bTestVar2", VarType::Bool, VarDefault::Bool(true));
    check_var_descriptor!(v2, "iTestVar3", VarType::Int, VarDefault::Int(0));
    check_var_descriptor!(v2, "iTestVar4", VarType::Int, VarDefault::Int(100));
    check_var_descriptor!(v2, "bTestVar5", VarType::Byte, VarDefault::Byte(50));

    Ok(())
}
