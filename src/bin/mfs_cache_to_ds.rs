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

// This program converts a moulars .mfs_cache file into a DirtSand .mfs on
// stdout.  It can be useful for debugging, as well as porting data back to
// DirtSand for compatibility/comparison.

use std::io::Result;
use std::path::Path;

use moulars::file_srv::Manifest;

fn main() -> Result<()> {
    let mut args = std::env::args();
    let _ = args.next();

    for arg in args {
        let manifest = Manifest::from_cache(Path::new(&arg))?;
        for file in manifest.files() {
            println!("{}", file.as_ds_mfs());
        }
    }

    Ok(())
}
