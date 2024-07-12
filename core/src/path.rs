//! Path handling utility functions for the Arcana Templating Engine.
// Copyright (C) 2024  Frankie Baffa
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use std::path::{
    Component,
    Path,
    PathBuf,
};

pub(crate)
fn clean_path<P: AsRef<Path>>(path: P) -> PathBuf {
    let mut out = Vec::new();

    for comp in path.as_ref().components() {
        match comp {
            Component::CurDir => (),
            Component::ParentDir => match out.last() {
                Some(Component::RootDir) => (),
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                None | Some(Component::CurDir) | Some(Component::ParentDir) |
                Some(Component::Prefix(_)) => out.push(comp),
            },
            comp => out.push(comp),
        }
    }

    if !out.is_empty() {
        out.iter().collect()
    }
    else {
        PathBuf::from(".")
    }
}
