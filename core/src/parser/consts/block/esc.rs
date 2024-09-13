//! Constant strings representing escaped blocks/tags for the Arcana Templating
//! Engine.
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

pub(crate)
const ESCAPE: &str = "\\";

pub(crate)
const MODIFIER: &str = "\\|";

pub(crate)
const IGNORE: &str = "\\!{";

pub(crate)
const COMMENT: &str = "\\#{";

pub(crate)
const EXTENDS: &str = "\\+{";

pub(crate)
const SOURCE: &str = "\\.{";

pub(crate)
const INCLUDE_FILE: &str = "\\&{";

pub(crate)
const INCLUDE_CONTENT: &str = "\\${";

pub(crate)
const EXPRESSION: &str = "\\%{";

pub(crate)
const SET_ITEM: &str = "\\={";

pub(crate)
const UNSET_ITEM: &str = "\\/{";

pub(crate)
const TAG: &str = "\\{";

pub(crate)
const ENDTAG: &str = "\\}";

pub(crate)
const BLOCK: &str = "\\(";

pub(crate)
const ENDBLOCK: &str = "\\)";
