//! File handling utilities for the Arcana Templating Engine.
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
    crate::error::{
        Error,
        Result,
    },
    std::{
        fmt::{ Display, Formatter, Result as FmtResult, },
        fs::read_to_string,
        path::{ Path, PathBuf, },
    },
};

const SPACE: char = ' ';
const TAB: char = '\t';
const NEWLINE: char = '\n';

pub(crate)
fn read_file<P: AsRef<Path>>(p: P) -> Result<String> {
    let mut output = String::new();

    let mut dlim = "";
    for line in read_to_string(&p).map_err(|e| Error::IO(e, p.as_ref().into()))?.lines() {
        output.push_str(&format!("{dlim}{line}"));
        if dlim.is_empty() {
            dlim = "\n";
        }
    }

    Ok(output)
}

pub(crate)
fn lines_from_string(content: String) -> Vec<String> {
    let mut lines = content.lines()
        // put the line breaks back
        .map(|l| format!("{l}\n"))
        .collect::<Vec<String>>();
    // remove the final line break
    lines.last_mut().unwrap().pop();
    lines
}

pub(crate)
fn read_file_lines<P>(p: P) -> Result<Vec<String>>
where
    P: AsRef<Path>
{
    let content = read_to_string(&p).map_err(|e| Error::IO(e, p.as_ref().into()))?;
    if content.is_empty() {
        return Ok(Vec::new());
    }

    Ok(lines_from_string(content))
}

/// The current read-position of a file source.
#[derive(Debug, Default, Clone, Copy)]
pub
struct Coordinate {
    line: usize,
    position: usize,
}

impl Coordinate {
    pub(crate)
    fn line(&self) -> usize {
        self.line
    }

    pub(crate)
    fn position(&self) -> usize {
        self.position
    }
}

/// A file that has been read into memory.
#[derive(Debug)]
pub
struct Source {
    file: PathBuf,
    coord: Coordinate,
    content: Vec<String>,
}

impl Default for Source {
    fn default() -> Self {
        Self {
            file: PathBuf::new(),
            coord: Coordinate { line: 0, position: 0, },
            content: Vec::new(),
        }
    }
}

impl Source {
    pub(crate)
    fn faux_source<P, S>(p: P, content: S) -> Self
    where
        P: AsRef<Path>,
        S: AsRef<str>,
    {
        Self {
            file: p.as_ref().into(),
            coord: Coordinate::default(),
            content: lines_from_string(content.as_ref().to_owned()),
        }
    }

    pub(crate)
    fn read_file<P>(p: P) -> Result<Self>
    where
        P: AsRef<Path>
    {
        Ok(Self {
            file: p.as_ref().into(),
            coord: Coordinate::default(),
            content: read_file_lines(p)?,
        })
    }

    pub(crate)
    fn pos(&self) -> &str {
        &self.content[self.coord.line][self.coord.position..]
    }

    pub(crate)
    fn file(&self) -> &PathBuf {
        &self.file
    }

    pub(crate)
    fn coord(&self) -> Coordinate {
        self.coord
    }

    pub(crate)
    fn set_coord(&mut self, coord: Coordinate) {
        self.coord = coord;
    }

    fn eol(&self) -> bool {
        self.coord.position == self.content[self.coord.line].len()
    }

    pub(crate)
    fn eof(&self) -> bool {
        self.eol() && self.coord.line == self.content.len() - 1
    }

    pub(crate)
    fn force_eof(&mut self) {
        if self.eof() {
            return;
        }

        self.coord.line = self.content.len() - 1;
        self.coord.position = self.content[self.coord.line].len();
    }

    pub(crate)
    fn skip_internal(&mut self) -> Option<char> {
        // still characters to read
        if !self.eof() {
            let b = self.content[self.coord.line][self.coord.position..self.coord.position+1]
                .as_bytes()[0];

            self.coord.position += 1;
            // if eol 
            if self.eol() && !self.eof() {
                self.coord.position = 0;
                self.coord.line += 1;
            }

            Some(b as char)
        }
        // file ended
        else {
            None
        }
    }

    pub(crate)
    fn take(&mut self, positions: usize) -> Option<String> {
        if self.eof() {
            return None;
        }
        else if positions == 0 {
            return Some(String::new());
        }

        let mut i = 0;
        let mut output = String::new();
        while i < positions && !self.eof() {
            if let Some(c) = self.skip_internal() {
                output.push(c);
            }
            else {
                break;
            }
            i += 1;
        }

        Some(output)
    }

    pub(crate)
    fn trim_start(&mut self) {
        while self.pos().starts_with(SPACE) || self.pos().starts_with(TAB) {
            self.skip_internal().unwrap();
        }
    }

    pub(crate)
    fn trim_start_multiline(&mut self) {
        while self.pos().starts_with(SPACE) || self.pos().starts_with(TAB) ||
            self.pos().starts_with(NEWLINE)
        {
            self.skip_internal().unwrap();
        }
    }
}

impl Display for Source {
    fn fmt(&self, fmtr: &mut Formatter<'_>) -> FmtResult {
        fmtr.write_fmt(format_args!(
            "{:?} line {} position {}",
            self.file,
            self.coord.line + 1,
            self.coord.position + 1
        ))
    }
}
