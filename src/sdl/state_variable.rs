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

use std::io::{Cursor, BufRead, Write};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use log::warn;
use paste::paste;

use crate::plasma::{Uoid, Creatable, UnifiedTime, Factory, StreamRead, StreamWrite};
use crate::plasma::color::{Color32, ColorRGBA};
use crate::plasma::creatable::ClassID;
use crate::plasma::geometry::{Quaternion, Vector3};
use crate::plasma::safe_string::{ReadSafeStr, WriteSafeStr, StringFormat};
use super::state::{State, read_compressed_size, write_compressed_size};
use super::{DescriptorDb, StateDescriptor, VarDescriptor, VarType, VarDefault};
use super::{HAS_NOTIFICATION_INFO, HAS_TIMESTAMP, SAME_AS_DEFAULT, HAS_DIRTY_FLAG, WANT_TIMESTAMP};

#[derive(Clone, Debug)]
enum VarValues {
    AgeTimeOfDay(usize),    // No stored value
    Bool(Vec<bool>),
    Byte(Vec<u8>),
    Creatable(Vec<Option<Arc<dyn Creatable>>>),
    Double(Vec<f64>),
    Float(Vec<f32>),
    Int(Vec<i32>),
    Key(Vec<Uoid>),
    Point3(Vec<Vector3>),
    Quat(Vec<Quaternion>),
    Rgb(Vec<ColorRGBA>),
    Rgb8(Vec<Color32>),
    Rgba(Vec<ColorRGBA>),
    Rgba8(Vec<Color32>),
    Short(Vec<i16>),
    String32(Vec<String>),
    Time(Vec<UnifiedTime>),
    Vector3(Vec<Vector3>),
    StateDesc(Vec<State>),
}

#[derive(Clone, Debug)]
pub struct Variable {
    descriptor: Arc<VarDescriptor>,
    values: VarValues,
    timestamp: UnifiedTime,
    notification_hint: String,
    dirty: bool,
}

macro_rules! get_default {
    ($descriptor:expr, VarDefault::$default_type:ident, $default_value:expr) => {
        match $descriptor.default() {
            None => $default_value,
            Some(VarDefault::$default_type(value)) => *value,
            Some(_) => unreachable!("Wrong VarDefault type"),
        }
    };

    ($descriptor:expr, VarDefault::$default_type:ident, $default_value:expr, CLONE) => {
        match $descriptor.default() {
            None => $default_value,
            Some(VarDefault::$default_type(value)) => value.clone(),
            Some(_) => unreachable!("Wrong VarDefault type"),
        }
    };
}

macro_rules! check_default {
    ($values:expr, VarValues::$val_type:ident, $default_value:expr) => {
        if let VarValues::$val_type(values) = $values {
            #[allow(clippy::float_cmp)]
            values.iter().all(|value| value == &$default_value)
        } else {
            unreachable!("Wrong VarValues type")
        }
    };
}

macro_rules! var_accessors {
    ($type_name:ident, VarValues::$value_type:ident, $real_type:ty) => {
        paste! {
            pub fn [<get_ $type_name>](&self, index: usize) -> Result<$real_type> {
                match &self.values {
                    VarValues::$value_type(values) => {
                        if let Some(element) = values.get(index) {
                            Ok(*element)
                        } else {
                            Err(anyhow!("Variable index {} out of range", index))
                        }
                    }
                    _ => Err(anyhow!("Cannot get {} from {:?} variable",
                             stringify!($type_name), self.descriptor.var_type()))
                }
            }
            pub fn [<set_ $type_name>](&mut self, index: usize, value: $real_type) -> Result<()> {
                match &mut self.values {
                    VarValues::$value_type(values) => {
                        if let Some(element) = values.get_mut(index) {
                            *element = value;
                            self.dirty = true;
                            Ok(())
                        } else {
                            Err(anyhow!("Variable index {} out of range", index))
                        }
                    }
                    _ => Err(anyhow!("Cannot assign {} to {:?} variable",
                             stringify!($type_name), self.descriptor.var_type()))
                }
            }
        }
    };
}

impl Variable {
    pub fn from_defaults(descriptor: Arc<VarDescriptor>, db: &DescriptorDb) -> Self {
        let count = descriptor.count().unwrap_or(0);
        let values = match descriptor.var_type() {
            VarType::AgeTimeOfDay => VarValues::AgeTimeOfDay(count),
            VarType::Creatable => {
                // Trickier than it should be due to clone requirements in the
                // implementation of the vec!() macro...
                let mut creatables = Vec::with_capacity(count);
                for _ in 0..count {
                    creatables.push(None);
                }
                VarValues::Creatable(creatables)
            }
            VarType::Key => VarValues::Key(vec![Uoid::invalid(); count]),
            VarType::Bool => {
                let default = get_default!(descriptor, VarDefault::Bool, false);
                VarValues::Bool(vec![default; count])
            }
            VarType::Byte => {
                let default = get_default!(descriptor, VarDefault::Byte, 0);
                VarValues::Byte(vec![default; count])
            }
            VarType::Double => {
                let default = get_default!(descriptor, VarDefault::Double, 0_f64);
                VarValues::Double(vec![default; count])
            }
            VarType::Float => {
                let default = get_default!(descriptor, VarDefault::Float, 0_f32);
                VarValues::Float(vec![default; count])
            }
            VarType::Int => {
                let default = get_default!(descriptor, VarDefault::Int, 0);
                VarValues::Int(vec![default; count])
            }
            VarType::Point3 => {
                let default = get_default!(descriptor, VarDefault::Vector3, Vector3::default());
                VarValues::Point3(vec![default; count])
            }
            VarType::Quat => {
                let default = get_default!(descriptor, VarDefault::Quat, Quaternion::default());
                VarValues::Quat(vec![default; count])
            }
            VarType::Rgb => {
                let default = get_default!(descriptor, VarDefault::Rgba, ColorRGBA::default());
                VarValues::Rgb(vec![default; count])
            }
            VarType::Rgb8 => {
                let default = get_default!(descriptor, VarDefault::Rgba8, Color32::default());
                VarValues::Rgb8(vec![default; count])
            }
            VarType::Rgba => {
                let default = get_default!(descriptor, VarDefault::Rgba, ColorRGBA::default());
                VarValues::Rgba(vec![default; count])
            }
            VarType::Rgba8 => {
                let default = get_default!(descriptor, VarDefault::Rgba8, Color32::default());
                VarValues::Rgba8(vec![default; count])
            }
            VarType::Short => {
                let default = get_default!(descriptor, VarDefault::Short, 0);
                VarValues::Short(vec![default; count])
            }
            VarType::String32 => {
                let default = get_default!(descriptor, VarDefault::String32, String::new(), CLONE);
                VarValues::String32(vec![default; count])
            }
            VarType::Time => {
                let default = get_default!(descriptor, VarDefault::Time, UnifiedTime::default());
                VarValues::Time(vec![default; count])
            }
            VarType::Vector3 => {
                let default = get_default!(descriptor, VarDefault::Vector3, Vector3::default());
                VarValues::Vector3(vec![default; count])
            }
            VarType::StateDesc(name) => {
                if let Some(descriptor) = db.get_latest(name) {
                    let mut states = Vec::with_capacity(count);
                    for _ in 0..count {
                        states.push(State::from_defaults(descriptor.clone(), db));
                    }
                    VarValues::StateDesc(states)
                } else {
                    panic!("Unknown state descriptor '{name}'");
                }
            }
        };

        Self {
            descriptor,
            values,
            timestamp: UnifiedTime::default(),
            notification_hint: String::new(),
            dirty: false
        }
    }

    pub fn is_default(&self) -> bool {
        if self.descriptor.count().is_none() {
            // Variable length state vars are never considered at default,
            // since the size is part of what's stored
            return false;
        }
        match self.descriptor.var_type() {
            VarType::AgeTimeOfDay => true,
            VarType::Creatable => {
                if let VarValues::Creatable(creatables) = &self.values {
                    creatables.iter().all(Option::is_none)
                } else {
                    unreachable!()
                }
            }
            VarType::Key => {
                let invalid_uoid = Uoid::invalid();
                check_default!(&self.values, VarValues::Key, invalid_uoid)
            }
            VarType::Bool => {
                let default = get_default!(self.descriptor, VarDefault::Bool, false);
                check_default!(&self.values, VarValues::Bool, default)
            }
            VarType::Byte => {
                let default = get_default!(self.descriptor, VarDefault::Byte, 0);
                check_default!(&self.values, VarValues::Byte, default)
            }
            VarType::Double => {
                let default = get_default!(self.descriptor, VarDefault::Double, 0_f64);
                check_default!(&self.values, VarValues::Double, default)
            }
            VarType::Float => {
                let default = get_default!(self.descriptor, VarDefault::Float, 0_f32);
                check_default!(&self.values, VarValues::Float, default)
            }
            VarType::Int => {
                let default = get_default!(self.descriptor, VarDefault::Int, 0);
                check_default!(&self.values, VarValues::Int, default)
            }
            VarType::Point3 => {
                let default = get_default!(self.descriptor, VarDefault::Vector3, Vector3::default());
                check_default!(&self.values, VarValues::Vector3, default)
            }
            VarType::Quat => {
                let default = get_default!(self.descriptor, VarDefault::Quat, Quaternion::default());
                check_default!(&self.values, VarValues::Quat, default)
            }
            VarType::Rgb => {
                let default = get_default!(self.descriptor, VarDefault::Rgba, ColorRGBA::default());
                check_default!(&self.values, VarValues::Rgb, default)
            }
            VarType::Rgb8 => {
                let default = get_default!(self.descriptor, VarDefault::Rgba8, Color32::default());
                check_default!(&self.values, VarValues::Rgb8, default)
            }
            VarType::Rgba => {
                let default = get_default!(self.descriptor, VarDefault::Rgba, ColorRGBA::default());
                check_default!(&self.values, VarValues::Rgba, default)
            }
            VarType::Rgba8 => {
                let default = get_default!(self.descriptor, VarDefault::Rgba8, Color32::default());
                check_default!(&self.values, VarValues::Rgba8, default)
            }
            VarType::Short => {
                let default = get_default!(self.descriptor, VarDefault::Short, 0);
                check_default!(&self.values, VarValues::Short, default)
            }
            VarType::String32 => {
                let default = get_default!(self.descriptor, VarDefault::String32, String::new(), CLONE);
                check_default!(&self.values, VarValues::String32, default)
            }
            VarType::Time => {
                let default = get_default!(self.descriptor, VarDefault::Time, UnifiedTime::default());
                check_default!(&self.values, VarValues::Time, default)
            }
            VarType::Vector3 => {
                let default = get_default!(self.descriptor, VarDefault::Vector3, Vector3::default());
                check_default!(&self.values, VarValues::Vector3, default)
            }
            VarType::StateDesc(_) => {
                if let VarValues::StateDesc(children) = &self.values {
                    children.iter().all(State::is_default)
                } else {
                    unreachable!()
                }
            }
        }
    }

    pub fn descriptor(&self) -> &VarDescriptor { &self.descriptor }
    pub fn is_dirty(&self) -> bool { self.dirty }

    var_accessors!(bool, VarValues::Bool, bool);
    var_accessors!(byte, VarValues::Byte, u8);
    var_accessors!(int, VarValues::Int, i32);

    pub fn read<S>(&mut self, stream: &mut S, db: &DescriptorDb) -> Result<()>
        where S: BufRead
    {
        let read_flags = stream.read_u8()?;
        self.notification_hint = if (read_flags & HAS_NOTIFICATION_INFO) != 0 {
            stream.read_u8()?;  // Unused: notification info read flags
            stream.read_safe_str(StringFormat::Latin1)?
        } else {
            String::new()
        };

        match self.descriptor.var_type() {
            VarType::StateDesc(name) => {
                let Some(statedesc) = db.get_latest(name) else {
                    return Err(anyhow!("No such descriptor {name}"));
                };
                self.read_statedesc(stream, db, &statedesc)?;
            }
            _ => self.read_simple(stream)?,
        }

        self.dirty = true;
        Ok(())
    }

    fn read_var_count<S>(&self, stream: &mut S) -> Result<usize>
        where S: BufRead
    {
        let var_count = match self.descriptor.count() {
            Some(count) => count,
            None => stream.read_u32::<LittleEndian>()? as usize
        };
        if var_count >= 10000 {
            Err(anyhow!("Too many elements in SDL variable ({var_count})"))
        } else {
            Ok(var_count)
        }
    }

    fn read_simple<S>(&mut self, stream: &mut S) -> Result<()>
        where S: BufRead
    {
        let read_flags = stream.read_u8()?;
        if (read_flags & HAS_TIMESTAMP) != 0 {
            self.timestamp = UnifiedTime::stream_read(stream)?;
        } else if (read_flags & HAS_DIRTY_FLAG) != 0 && (read_flags & WANT_TIMESTAMP) != 0 {
            self.timestamp = UnifiedTime::now()?;
        }

        if (read_flags & SAME_AS_DEFAULT) == 0 {
            let total_count = self.read_var_count(stream)?;
            self.values = match self.descriptor.var_type() {
                VarType::AgeTimeOfDay => VarValues::AgeTimeOfDay(total_count),
                VarType::Bool => {
                    let mut values = Vec::with_capacity(total_count);
                    for _ in 0..total_count {
                        values.push(stream.read_u8()? != 0);
                    }
                    VarValues::Bool(values)
                }
                VarType::Byte => {
                    let mut values = vec![0; total_count];
                    stream.read_exact(values.as_mut_slice())?;
                    VarValues::Byte(values)
                }
                VarType::Creatable => {
                    let mut values = Vec::with_capacity(total_count);
                    for _ in 0..total_count {
                        values.push(Self::read_creatable(stream)?);
                    }
                    VarValues::Creatable(values)
                }
                VarType::Double => {
                    let mut values = vec![0_f64; total_count];
                    stream.read_f64_into::<LittleEndian>(values.as_mut_slice())?;
                    VarValues::Double(values)
                }
                VarType::Float => {
                    let mut values = vec![0_f32; total_count];
                    stream.read_f32_into::<LittleEndian>(values.as_mut_slice())?;
                    VarValues::Float(values)
                }
                VarType::Int => {
                    let mut values = vec![0; total_count];
                    stream.read_i32_into::<LittleEndian>(values.as_mut_slice())?;
                    VarValues::Int(values)
                }
                VarType::Key => {
                    let mut values = Vec::with_capacity(total_count);
                    for _ in 0..total_count {
                        values.push(Uoid::stream_read(stream)?);
                    }
                    VarValues::Key(values)
                }
                VarType::Point3 => {
                    let mut values = Vec::with_capacity(total_count);
                    for _ in 0..total_count {
                        values.push(Vector3::stream_read(stream)?);
                    }
                    VarValues::Point3(values)
                }
                VarType::Quat => {
                    let mut values = Vec::with_capacity(total_count);
                    for _ in 0..total_count {
                        values.push(Quaternion::stream_read(stream)?);
                    }
                    VarValues::Quat(values)
                }
                VarType::Rgb => {
                    let mut values = Vec::with_capacity(total_count);
                    for _ in 0..total_count {
                        let mut v = [0_f32; 3];
                        stream.read_f32_into::<LittleEndian>(&mut v)?;
                        values.push(ColorRGBA { r: v[0], g: v[1], b: v[2], a: 1_f32 });
                    }
                    VarValues::Rgb(values)
                }
                VarType::Rgb8 => {
                    let mut values = Vec::with_capacity(total_count);
                    for _ in 0..total_count {
                        let mut v = [0; 3];
                        stream.read_exact(&mut v)?;
                        values.push(Color32 { b: v[0], g: v[1], r: v[2], a: 255 });
                    }
                    VarValues::Rgb8(values)
                }
                VarType::Rgba => {
                    let mut values = Vec::with_capacity(total_count);
                    for _ in 0..total_count {
                        let mut v = [0_f32; 4];
                        stream.read_f32_into::<LittleEndian>(&mut v)?;
                        values.push(ColorRGBA { r: v[0], g: v[1], b: v[2], a: v[3] });
                    }
                    VarValues::Rgba(values)
                }
                VarType::Rgba8 => {
                    let mut values = Vec::with_capacity(total_count);
                    for _ in 0..total_count {
                        let mut v = [0; 4];
                        stream.read_exact(&mut v)?;
                        values.push(Color32 { b: v[0], g: v[1], r: v[2], a: v[3] });
                    }
                    VarValues::Rgba8(values)
                }
                VarType::Short => {
                    let mut values = vec![0; total_count];
                    stream.read_i16_into::<LittleEndian>(values.as_mut_slice())?;
                    VarValues::Short(values)
                }
                VarType::String32 => {
                    let mut values = Vec::with_capacity(total_count);
                    for _ in 0..total_count {
                        let mut buf = [0; 32];
                        stream.read_exact(&mut buf)?;
                        values.push(String::from_utf8_lossy(
                                buf.split(|ch| ch == &0).next().unwrap()).to_string());
                    }
                    VarValues::String32(values)
                }
                VarType::Time => {
                    let mut values = Vec::with_capacity(total_count);
                    for _ in 0..total_count {
                        values.push(UnifiedTime::stream_read(stream)?);
                    }
                    VarValues::Time(values)
                }
                VarType::Vector3 => {
                    let mut values = Vec::with_capacity(total_count);
                    for _ in 0..total_count {
                        values.push(Vector3::stream_read(stream)?);
                    }
                    VarValues::Vector3(values)
                }
                VarType::StateDesc(_) => unreachable!(),
            }
        }

        Ok(())
    }

    fn read_creatable<S>(stream: &mut S) -> Result<Option<Arc<dyn Creatable>>>
        where S: BufRead
    {
        let class_id = stream.read_u16::<LittleEndian>()?;
        if class_id == ClassID::Nil as u16 {
            return Ok(None);
        }

        let creatable_size = stream.read_u32::<LittleEndian>()?;
        let mut creatable_buf = vec![0; creatable_size as usize];
        stream.read_exact(creatable_buf.as_mut_slice())?;
        let mut creatable_stream = Cursor::new(creatable_buf);
        let object = Factory::read_creatable_as(&mut creatable_stream, class_id)?;
        #[allow(clippy::cast_possible_truncation)]
        if creatable_stream.position() as usize != creatable_stream.get_ref().len() {
            warn!("Creatable 0x{:04x} was not fully parsed in SDL blob ({} of {} bytes read)",
                  class_id, creatable_stream.position(), creatable_stream.get_ref().len());
        }
        Ok(object.map(Arc::from))
    }

    fn read_statedesc<S>(&mut self, stream: &mut S, db: &DescriptorDb,
                         statedesc: &Arc<StateDescriptor>) -> Result<()>
        where S: BufRead
    {
        stream.read_u8()?;  // Unused: SD Var read flags
        let total_count = self.read_var_count(stream)?;
        let mut values = Vec::with_capacity(total_count);
        for _ in 0..total_count {
            values.push(State::from_defaults(statedesc.clone(), db));
        }
        let max_hint = self.descriptor.count().unwrap_or(0);
        let dirty_count = read_compressed_size(stream, max_hint)?;
        let read_indices = dirty_count != total_count;
        for idx in 0..dirty_count {
            let idx = if read_indices {
                read_compressed_size(stream, max_hint)?
            } else {
                idx
            };
            if idx >= values.len() {
                return Err(anyhow!("Invalid value index {idx}"));
            }
            values[idx].read(stream, db)?;
        }
        self.values = VarValues::StateDesc(values);
        Ok(())
    }

    pub fn write(&self, stream: &mut dyn Write) -> Result<()> {
        if !self.notification_hint.is_empty() {
            stream.write_u8(HAS_NOTIFICATION_INFO)?;
            stream.write_u8(0)?;    // Unused: notification info read flags
            stream.write_safe_str(&self.notification_hint, StringFormat::Latin1)?;
        } else {
            stream.write_u8(0)?;    // No read flags
        }

        match self.descriptor.var_type() {
            VarType::StateDesc(_) => self.write_statedesc(stream)?,
            _ => self.write_simple(stream)?,
        }

        Ok(())
    }

    pub fn write_statedesc(&self, stream: &mut dyn Write) -> Result<()> {
        stream.write_u8(0)?;    // Unused: SD Var read flags

        let VarValues::StateDesc(values) = &self.values else {
            unreachable!()
        };
        let num_values = u32::try_from(values.len())
                .map_err(|_| anyhow!("Too many values for stream: {}", values.len()))?;

        if self.descriptor.count().is_none() {
            stream.write_u32::<LittleEndian>(num_values)?;
        }
        let mut dirty_list = Vec::with_capacity(values.len());
        for (idx, state) in values.iter().enumerate() {
            if state.is_dirty() {
                dirty_list.push((idx, state));
            }
        }
        let max_hint = self.descriptor.count().unwrap_or(0);
        write_compressed_size(stream, max_hint, dirty_list.len())?;
        let write_indices = dirty_list.len() != values.len();
        for (idx, state) in dirty_list {
            if write_indices {
                write_compressed_size(stream, max_hint, idx)?;
            }
            state.write(stream)?;
        }

        Ok(())
    }

    pub fn write_simple(&self, stream: &mut dyn Write) -> Result<()> {
        let mut write_flags = 0;
        if self.timestamp != UnifiedTime::default() {
            write_flags |= HAS_TIMESTAMP;
        }
        if self.is_default() {
            write_flags |= SAME_AS_DEFAULT;
        }
        if self.is_dirty() {
            write_flags |= HAS_DIRTY_FLAG;
        }
        stream.write_u8(write_flags)?;

        if (write_flags & SAME_AS_DEFAULT) == 0 {
            match &self.values {
                VarValues::AgeTimeOfDay(count) => {
                    self.write_var_count(stream, *count)?;
                }
                VarValues::Bool(values) => {
                    self.write_var_count(stream, values.len())?;
                    for val in values {
                        stream.write_u8(u8::from(*val))?;
                    }
                }
                VarValues::Byte(values) => {
                    self.write_var_count(stream, values.len())?;
                    stream.write_all(values.as_slice())?;
                }
                VarValues::Creatable(values) => {
                    self.write_var_count(stream, values.len())?;
                    for creatable in values {
                        Self::write_creatable(stream, creatable.as_ref())?;
                    }
                }
                VarValues::Double(values) => {
                    self.write_var_count(stream, values.len())?;
                    for val in values {
                        stream.write_f64::<LittleEndian>(*val)?;
                    }
                }
                VarValues::Float(values) => {
                    self.write_var_count(stream, values.len())?;
                    for val in values {
                        stream.write_f32::<LittleEndian>(*val)?;
                    }
                }
                VarValues::Int(values) => {
                    self.write_var_count(stream, values.len())?;
                    for val in values {
                        stream.write_i32::<LittleEndian>(*val)?;
                    }
                }
                VarValues::Key(values) => {
                    self.write_var_count(stream, values.len())?;
                    for key in values {
                        key.stream_write(stream)?;
                    }
                }
                VarValues::Point3(values) | VarValues::Vector3(values) => {
                    self.write_var_count(stream, values.len())?;
                    for val in values {
                        val.stream_write(stream)?;
                    }
                }
                VarValues::Quat(values) => {
                    self.write_var_count(stream, values.len())?;
                    for val in values {
                        val.stream_write(stream)?;
                    }
                }
                VarValues::Rgb(values) => {
                    self.write_var_count(stream, values.len())?;
                    for val in values {
                        stream.write_f32::<LittleEndian>(val.r)?;
                        stream.write_f32::<LittleEndian>(val.g)?;
                        stream.write_f32::<LittleEndian>(val.b)?;
                    }
                }
                VarValues::Rgb8(values) => {
                    self.write_var_count(stream, values.len())?;
                    for val in values {
                        stream.write_u8(val.b)?;
                        stream.write_u8(val.g)?;
                        stream.write_u8(val.r)?;
                    }
                }
                VarValues::Rgba(values) => {
                    self.write_var_count(stream, values.len())?;
                    for val in values {
                        stream.write_f32::<LittleEndian>(val.r)?;
                        stream.write_f32::<LittleEndian>(val.g)?;
                        stream.write_f32::<LittleEndian>(val.b)?;
                        stream.write_f32::<LittleEndian>(val.a)?;
                    }
                }
                VarValues::Rgba8(values) => {
                    self.write_var_count(stream, values.len())?;
                    for val in values {
                        stream.write_u8(val.b)?;
                        stream.write_u8(val.g)?;
                        stream.write_u8(val.r)?;
                        stream.write_u8(val.a)?;
                    }
                }
                VarValues::Short(values) => {
                    self.write_var_count(stream, values.len())?;
                    for val in values {
                        stream.write_i16::<LittleEndian>(*val)?;
                    }
                }
                VarValues::String32(values) => {
                    self.write_var_count(stream, values.len())?;
                    for val in values {
                        for ch in val.bytes().chain(std::iter::repeat(0u8)).take(31) {
                            stream.write_u8(ch)?;
                        }
                        stream.write_u8(0)?;    // Force nul terminator
                    }
                }
                VarValues::Time(values) => {
                    self.write_var_count(stream, values.len())?;
                    for val in values {
                        val.stream_write(stream)?;
                    }
                }
                VarValues::StateDesc(_) => unreachable!(),
            }
        }

        Ok(())
    }

    fn write_var_count(&self, stream: &mut dyn Write, count: usize) -> Result<()> {
        if count > 10000 {
            return Err(anyhow!("Too many elements in SDL variable ({count})"));
        }
        if self.descriptor.count().is_none() {
            #[allow(clippy::cast_possible_truncation)]
            Ok(stream.write_u32::<LittleEndian>(count as u32)?)
        } else {
            Ok(())
        }
    }

    fn write_creatable(stream: &mut dyn Write,
                       creatable: Option<&Arc<dyn Creatable>>) -> Result<()>
    {
        if let Some(creatable) = creatable {
            stream.write_u16::<LittleEndian>(creatable.class_id())?;
            let mut creatable_stream = Cursor::new(Vec::new());
            creatable.stream_write(&mut creatable_stream)?;
            let creatable_buf = creatable_stream.into_inner();
            let creatable_size = u32::try_from(creatable_buf.len())
                    .context("Creatable too large for stream")?;
            stream.write_u32::<LittleEndian>(creatable_size)?;
            stream.write_all(creatable_buf.as_slice())?;
        } else {
            stream.write_u16::<LittleEndian>(ClassID::Nil as u16)?;
        }

        Ok(())
    }

    pub fn upgrade_from(&mut self, old_var: &Variable, db: &DescriptorDb) {
        if old_var.is_default() {
            if old_var.descriptor.default() != self.descriptor.default() {
                warn!("{}: Default changed from {:?} to {:?}", self.descriptor.name(),
                      old_var.descriptor.default(), self.descriptor.default());
            }
            // Automatically use the new default value.  This assumes we were
            // created in a default state as well.
            debug_assert!(self.is_default());
            return;
        }
        if self.descriptor.var_type() != old_var.descriptor.var_type() {
            // TODO: Support some type conversions where they make sense
            // (e.g. Byte -> Int)
            warn!("Type conversion (from {:?} to {:?}) is not supported.  \
                  Reverting to defaults.",
                  old_var.descriptor.var_type(), self.descriptor.var_type());
            return;
        }
        if self.descriptor.count() != old_var.descriptor.count() {
            // TODO: It should be allowable to resize during and upgrade, but
            // so far no descriptors actually do this.
            warn!("Variable resizing (from {:?} to {:?}) is not supported.  \
                  Reverting to defaults.",
                  old_var.descriptor.count(), self.descriptor.count());
            return;
        }

        self.values = old_var.values.clone();
        if let VarValues::StateDesc(values) = &mut self.values {
            for state in values {
                if let Some(upgraded) = state.upgrade(db) {
                    *state = upgraded;
                }
            }
        }
    }
}
