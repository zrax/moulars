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

use crate::plasma::color::{Color32, ColorRGBA};
use crate::plasma::geometry::{Quaternion, Vector3};

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum VarType {
    AgeTimeOfDay, Bool, Byte, Creatable, Double, Float, Int, Key, Point3,
    Quat, Rgb, Rgb8, Rgba, Rgba8, Short, String32, Time, Vector3,
    StateDesc(String),
}

#[derive(PartialEq, Debug)]
pub enum VarDefault {
    Null,
    Bool(bool),
    Byte(u8),
    Short(i16),
    Int(i32),
    Time(u32),
    Float(f32),
    Double(f64),
    String32(String),
    Quat(Quaternion),
    Vector3(Vector3),
    Rgba(ColorRGBA),
    Rgba8(Color32),
}

#[derive(Debug)]
pub struct VarDescriptor {
    name: String,
    var_type: VarType,
    count: Option<usize>,
    default: Option<VarDefault>,
}

#[derive(Debug)]
pub struct StateDescriptor {
    name: String,
    version: u32,
    vars: Vec<VarDescriptor>,
}

impl VarDescriptor {
    pub fn new(name: String, var_type: VarType, count: Option<usize>,
               default: Option<VarDefault>) -> Self
    {
        Self { name, var_type, count, default }
    }

    pub fn name(&self) -> &String { &self.name }
    pub fn var_type(&self) -> &VarType { &self.var_type }
    pub fn count(&self) -> Option<usize> { self.count }
    pub fn default(&self) -> Option<&VarDefault> { self.default.as_ref() }
}

impl StateDescriptor {
    pub fn new(name: String, version: u32, vars: Vec<VarDescriptor>) -> Self {
        Self { name, version, vars }
    }

    pub fn name(&self) -> &String { &self.name }
    pub fn version(&self) -> u32 { self.version }
    pub fn vars(&self) -> &Vec<VarDescriptor> { &self.vars }
}
