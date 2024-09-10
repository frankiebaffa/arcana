//! Error types for the Arcana Templating Engine.
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

use {
    crate::{
        context::Alias,
        file::Coordinate,
    },
    std::{
        error::Error as StdError,
        fmt::{
            Display,
            Formatter,
            Result as FmtResult,
        },
        io::Error as IOError,
        path::PathBuf,
        result::Result as StdResult,
    },
    serde_json::Error as JsonError,
};

/// The error type for the Arcana Templating Engine.
#[derive(Debug)]
pub enum Error {
    IO(IOError, PathBuf),
    JsonParse(JsonError, PathBuf),
    IllegalRelativePath(PathBuf),
    IllegalDirPath(PathBuf),
    NoScopedPath(Alias),
    NotAMap(PathBuf),
    UnterminatedTag(String, Coordinate, PathBuf),
    IllegalCharacter(String, char, Coordinate, PathBuf),
    IllegalCharacterAfterExtends(char, Coordinate, PathBuf),
    AlreadyExtending(Coordinate, PathBuf, PathBuf),
    ExtendsFileDoesNotExist(Coordinate, PathBuf),
    IllegalExtendsPosition(Coordinate, PathBuf),
    UnterminatedPath(Coordinate, PathBuf),
    UnterminatedAlias(Coordinate, PathBuf),
    EmptyAlias(Coordinate, PathBuf),
    ValueNotArray(Alias),
    ValueNotString(Alias),
    ValueNotPath(Alias),
    ValuesNotPath(Alias),
    ValueNotFound(Alias),
    ValueNotObject(Alias),
    ContextEmpty(Coordinate, PathBuf),
    IllegalSplit(usize, usize, Coordinate, PathBuf),
    CannotCompare(Alias, Alias),
}

impl Display for Error {
    fn fmt(&self, fmtr: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::IO(e, p) => fmtr.write_fmt(format_args!("IO error in {:?} {:?}", p, e)),
            Self::JsonParse(e, p) => fmtr.write_fmt(format_args!("Json error in {:?} {:?}", p, e)),
            Self::IllegalRelativePath(p) => fmtr.write_fmt(
                format_args!("Expected absolute path was relative {:?}", p)
            ),
            Self::IllegalDirPath(p) => fmtr.write_fmt(
                format_args!("Expected file path was directory {:?}", p)
            ),
            Self::NoScopedPath(a) => fmtr.write_fmt(
                format_args!("No scoped path found in alias {}", a)
            ),
            Self::NotAMap(p) => fmtr.write_fmt(
                format_args!("Context at {:?} was not a json object", p)
            ),
            Self::UnterminatedTag(name, c, p) => fmtr.write_fmt(format_args!(
                "Unterminated {} in {:?} at line {} position {}",
                name,
                p,
                c.line() + 1,
                c.position() + 1,
            )),
            Self::IllegalCharacter(name, ch, c, p) => fmtr.write_fmt(format_args!(
                "Illegal '{}' character in {} tag in {:?} at line {} position {}",
                ch,
                name,
                p,
                c.line() + 1,
                c.position() + 1,
            )),
            Self::IllegalCharacterAfterExtends(ch, c, p) => fmtr.write_fmt(format_args!(
                "Illegal '{}' character after extends in {:?} at line {} position {}",
                ch,
                p,
                c.line() + 1,
                c.position() + 1,
            )),
            Self::IllegalExtendsPosition(c, p) => fmtr.write_fmt(format_args!(
                "Extends file {:?} defined at line {} position {} is in an illegal position",
                p,
                c.line() + 1,
                c.position() + 1
            )),
            Self::AlreadyExtending(c, p1, p2) => fmtr.write_fmt(format_args!(
                "Template is already extending {:?}, but was commanded to extend {:?} at line {} position {}",
                p1,
                p2,
                c.line() + 1,
                c.position() + 1,
            )),
            Self::ExtendsFileDoesNotExist(c, p) => fmtr.write_fmt(format_args!(
                "Extends file {:?} defined at line {} position {} does not exist",
                p,
                c.line() + 1,
                c.position() + 1
            )),
            Self::UnterminatedPath(c, p) => fmtr.write_fmt(format_args!(
                "Unterminated path in {:?} at line {} position {}",
                p,
                c.line() + 1,
                c.position() + 1
            )),
            Self::UnterminatedAlias(c, p) => fmtr.write_fmt(format_args!(
                "Unterminated alias in {:?} at line {} position {}",
                p,
                c.line() + 1,
                c.position() + 1
            )),
            Self::EmptyAlias(c, p) => fmtr.write_fmt(format_args!(
                "Empty alias in {:?} at line {} position {}",
                p,
                c.line() + 1,
                c.position() + 1
            )),
            Self::ValueNotArray(a) => fmtr.write_fmt(format_args!(
                "Value at {} was not an array",
                a
            )),
            Self::ValueNotString(a) => fmtr.write_fmt(format_args!(
                "Value at {} was not a string",
                a
            )),
            Self::ValueNotPath(a) => fmtr.write_fmt(format_args!(
                "Value at {} was not a path",
                a
            )),
            Self::ValuesNotPath(a) => fmtr.write_fmt(format_args!(
                "A value in array {} was not a path",
                a
            )),
            Self::ValueNotFound(a) => fmtr.write_fmt(format_args!(
                "Value at {} does not exist",
                a
            )),
            Self::ValueNotObject(a) => fmtr.write_fmt(format_args!(
                "Value at {} was not an object and cannot be copied to $root",
                a,
            )),
            Self::ContextEmpty(c, p) => fmtr.write_fmt(format_args!(
                "Context was unexpectedly empty in {:?} at line {} position {}",
                p,
                c.line() + 1,
                c.position() + 1
            )),
            Self::IllegalSplit(into, idx, c, f) => fmtr.write_fmt(format_args!(
                "Split modifier was invalid for {into} parts and index {idx} in {:?} at line {} position {}",
                f,
                c.line() + 1,
                c.position() + 1
            )),
            Self::CannotCompare(a, b) => fmtr.write_fmt(format_args!(
                "Cannot compare non-similar data-type {a} to {b}"
            )),
        }
    }
}

impl StdError for Error {}

/// The result type for the Arcana Templating Engine.
pub type Result<T> = StdResult<T, Error>;
