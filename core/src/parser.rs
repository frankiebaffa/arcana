//! The core parsing logic for the Arcana Templating Engine.
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

mod consts;

use {
    crate::{
        context::{
            Alias,
            JsonContext,
        },
        error::{
            Error,
            Result,
        },
        file::{
            Coordinate,
            Source,
            read_file,
        },
    },
    nfm_core::Parser as NfmParser,
    serde_json::Value as JsonValue,
    std::{
        env::current_dir,
        path::{
            Path,
            PathBuf,
        },
    },
};

#[derive(PartialEq)]
enum IncludeContentMod {
    Upper,
    Lower,
    Path,
    Replace(String, String),
    Split(usize, usize),
    Trim,
}

#[derive(PartialEq)]
enum IncludeFileMod {
    Md,
    Raw,
}

#[derive(Clone)]
enum ForFileMod {
    Ext(String),
    Reverse,
}

#[derive(Clone)]
enum ForItemMod {
    Reverse,
    Paths,
}

#[derive(Clone)]
enum IfCondition {
    Exists,
    Empty,
}

#[derive(PartialEq)]
enum SetValueMod {
    Array,
    Path,
}

#[derive(PartialEq)]
enum UnsetItemMod {
    Pop,
}

/// The parser for Arcana templates.
#[derive(Debug)]
pub
struct Parser {
    path: PathBuf,
    context: Option<JsonContext>,
    extends: Option<PathBuf>,
    can_extend: bool,
    source: Source,
    output: String,
}

impl Parser {
    fn new_internal<P>(path: P, ctx: Option<JsonContext>) -> Result<Self>
    where
        P: AsRef<Path>
    {
        let abs_path = Self::normalize_initial_path(path)?;

        Ok(Self {
            path: abs_path.clone(),
            context: ctx,
            extends: None,
            can_extend: true,
            source: Source::read_file(abs_path)?,
            output: String::new(),
        })
    }

    fn spawn_parser<P, F>(&mut self, p: P, f: F) -> Result<String>
    where
        P: AsRef<Path>,
        F: FnOnce(&mut Parser) -> Result<()>,
    {
        // take context from this parser
        let ctx = std::mem::take(&mut self.context);
        // initialize new parser at path with context and parse
        let mut scoped_parser = Self::new_internal(p, ctx)?;
        f(&mut scoped_parser)?;
        // deconstruct new parser into context and output
        let Parser { mut context, output, .. } = scoped_parser;
        // place context back into this parser
        std::mem::swap(&mut self.context, &mut context);
        // return output of scoped parser
        Ok(output)
    }

    fn spawn_sealed_parser<P, F>(&self, p: P, f: F) -> Result<String>
    where
        P: AsRef<Path>,
        F: FnOnce(&mut Parser) -> Result<()>,
    {
        // clone context from this parser
        let new_ctx = self.context.clone();
        // initialize new parser with cloned context and parse
        let mut scoped_parser = Self::new_internal(p, new_ctx)?;
        f(&mut scoped_parser)?;
        // deconstruct new parser into output
        let Parser { output, .. } = scoped_parser;
        // return output of scoped parser
        Ok(output)
    }

    fn spawn_sealed_internal_parser<F>(&mut self, f: F) -> Result<String>
    where
        F: FnOnce(&mut Parser) -> Result<()>
    {
        let mut internal_parser = Self {
            path: self.path.clone(),
            context: self.context.clone(),
            extends: self.extends.clone(),
            can_extend: false,
            source: Source::default(),
            output: String::new(),
        };

        // swap in the existing source
        std::mem::swap(&mut self.source, &mut internal_parser.source);
        f(&mut internal_parser)?;
        // swap the source back
        std::mem::swap(&mut self.source, &mut internal_parser.source);
        // deconstruct internal parser into output
        let Parser { output, .. } = internal_parser;
        // return the output of the internal parser
        Ok(output)
    }

    pub(crate)
    fn file(&self) -> &PathBuf {
        &self.path
    }

    pub(crate)
    fn directory(&self) -> PathBuf {
        let mut file = self.file().to_owned();
        file.pop();
        file
    }

    pub(crate)
    fn ctx(&self) -> &Option<JsonContext> {
        &self.context
    }

    pub(crate)
    fn ctx_mut(&mut self) -> &mut Option<JsonContext> {
        &mut self.context
    }

    pub(crate)
    fn src(&self) -> &Source {
        &self.source
    }

    pub(crate)
    fn src_mut(&mut self) -> &mut Source {
        &mut self.source
    }

    fn normalize_path_internal<B, P>(b: B, p: P) -> PathBuf
    where
        B: AsRef<Path>,
        P: AsRef<Path>
    {
        let path: PathBuf = p.as_ref().into();
        if path.is_absolute() {
            return path;
        }

        let mut base: PathBuf = b.as_ref().into();
        if !base.is_dir() {
            base.pop();
        }

        base.push(path);

        base
    }

    fn normalize_initial_path<P>(p: P) -> Result<PathBuf>
    where
        P: AsRef<Path>
    {
        let current_dir = current_dir().map_err(|e| Error::IO(e, p.as_ref().into()))?;
        Ok(Self::normalize_path_internal(current_dir, p))
    }

    fn normalize_path<P>(&self, p: P) -> PathBuf
    where
        P: AsRef<Path>
    {
        Self::normalize_path_internal(self.directory(), p)
    }

    fn read_ctx_in_internal<P, A>(&mut self, path: P, alias: Option<A>) -> Result<()>
    where
        P: AsRef<Path>,
        A: Into<Alias>
    {
        let path = self.normalize_path(path);

        if let Some(alias) = alias {
            if let Some(context) = &mut self.context {
                context.read_in_as(path, alias)?;
            }
            else {
                self.context = Some(JsonContext::read_as(path, alias)?);
            }
        }
        else if let Some(context) = &mut self.context {
            context.read_in(path)?;
        }
        else {
            self.context = Some(JsonContext::read(path)?);
        }

        Ok(())
    }

    pub(crate)
    fn read_ctx_in<P>(&mut self, path: P) -> Result<()>
    where
        P: AsRef<Path>
    {
        self.read_ctx_in_internal::<P, Alias>(path, None)
    }

    pub(crate)
    fn read_ctx_in_as<P, A>(&mut self, path: P, alias: A) -> Result<()>
    where
        P: AsRef<Path>,
        A: Into<Alias>
    {
        self.read_ctx_in_internal(path, Some(alias))
    }

    /// Create a new parser.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the template.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use arcana_core::Parser;
    ///
    /// Parser::new("test/full/1/page.html").unwrap();
    /// ```
    pub
    fn new<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>
    {
        Self::new_internal(path, None)
    }

    fn esc_endblock(&mut self) {
        self.src_mut().take(consts::block::esc::ESCAPE.len());
        let taken = self.src_mut().take(1).unwrap();
        self.output.push_str(&taken);
    }

    fn illegal_character<S>(&mut self, tag_name: S) -> Error
    where
        S: AsRef<str>
    {
        Error::IllegalCharacter(
            tag_name.as_ref().to_owned(),
            self.src().pos()[0..1].chars().next().unwrap(),
            self.src().coord(),
            self.src().file().to_owned(),
        )
    }

    fn until_end(&mut self, end: &str, error: Error) -> Result<()> {
        while !self.src().pos().starts_with(end) {
            if self.src().eof() {
                return Err(error);
            }

            self.src_mut().take(1);
        }

        self.src_mut().take(end.len()).unwrap();
        Ok(())
    }

    fn ignore(&mut self) -> Result<bool> {
        if self.src().pos().starts_with(consts::block::IGNORE) {
            self.until_end(&consts::block::ENDIGNORE, Error::UnterminatedTag(
                "ignore".to_owned(),
                self.src().coord(),
                self.src().file().to_owned(),
            ))?;
            self.src_mut().force_eof();

            Ok(true)
        }
        else {
            Ok(false)
        }
    }

    fn comment(&mut self) -> Result<bool> {
        if self.src().pos().starts_with(consts::block::COMMENT) {
            self.until_end(consts::block::ENDCOMMENT, Error::UnterminatedTag(
                "comment".to_owned(),
                self.src().coord(),
                self.src().file().to_owned(),
            ))?;

            Ok(true)
        }
        else {
            Ok(false)
        }
    }

    fn disallowed_after_extends(&mut self) -> Result<()> {
        if self.extends.is_none() {
            self.can_extend = false;
            return Ok(());
        }

        return Err(Error::IllegalCharacterAfterExtends(
            self.src().pos()[0..1].chars().next().unwrap(),
            self.src().coord(),
            self.src().file().to_owned(),
        ));
    }

    fn unexpected_eof<F>(&self, error: F) -> Result<()>
    where
        F: FnOnce() -> Error
    {
        if !self.src().eof() {
            return Ok(());
        }

        Err(error())
    }

    fn starts_with_alias_char(&self) -> bool {
        matches!(
            &self.src().pos()[0..1],
            "a"|"b"|"c"|"d"|"e"|"f"|"g"|"h"|"i"|"j"|"k"|"l"|"m"|"n"|
            "o"|"p"|"q"|"r"|"s"|"t"|"u"|"v"|"w"|"x"|"y"|"z"|"A"|"B"|
            "C"|"D"|"E"|"F"|"G"|"H"|"I"|"J"|"K"|"L"|"M"|"N"|"O"|"P"|
            "Q"|"R"|"S"|"T"|"U"|"V"|"W"|"X"|"Y"|"Z"|"1"|"2"|"3"|"4"|
            "5"|"6"|"7"|"8"|"9"|"0"|"_"|"-"|"."|"$"
        )
    }

    fn has_context(&self) -> Result<()> {
        if self.context.is_some() {
            return Ok(());
        }

        return Err(Error::ContextEmpty(
            self.src().coord(),
            self.src().file().to_owned()
        ));
    }

    pub(crate)
    fn enforce_context<T, F>(&mut self, f: F) -> Result<T>
    where
        F: FnOnce(&mut JsonContext) -> Result<T>
    {
        self.has_context()?;

        let ctx = self.context.as_mut().unwrap();
        f(ctx)
    }

    pub(crate)
    fn optional_context<T, F>(&mut self, f: F) -> Result<Option<T>>
    where
        F: FnOnce(&mut JsonContext) -> Result<Option<T>>
    {
        if self.context.is_none() {
            return Ok(None);
        }

        let ctx = self.context.as_mut().unwrap();
        f(ctx)
    }

    fn path(&mut self) -> Result<String> {
        let start = self.src().coord();

        self.src_mut().take(consts::PATH.len()).unwrap();

        let mut path_str = String::new();
        loop {
            self.unexpected_eof(|| Error::UnterminatedPath(
                start, self.src().file().to_owned(),
            ))?;

            if self.src().pos().starts_with(consts::PATH) {
                self.src_mut().take(consts::PATH.len());
                break;
            }

            path_str.push_str(&self.src_mut().take(1).unwrap());
        }

        Ok(path_str)
    }

    fn alias<S>(&mut self, tag_name: S) -> Result<String>
    where
        S: AsRef<str>
    {
        let mut alias_str = String::new();
        let start = self.src().coord();
        let mut first = true;

        loop {
            self.unexpected_eof(|| Error::UnterminatedAlias(
                start, self.src().file().to_owned(),
            ))?;

            if !self.starts_with_alias_char() {
                if first {
                    return Err(self.illegal_character(tag_name));
                }

                break;
            }

            alias_str.push_str(&self.src_mut().take(1).unwrap());
            first = false;
        }

        if alias_str.is_empty() {
            return Err(Error::EmptyAlias(
                start, self.src().file().to_owned()
            ));
        }

        Ok(alias_str)
    }

    fn pathlike<S>(&mut self, tag_name: S) -> Result<PathBuf>
    where
        S: AsRef<str>
    {
        // is a literal path
        if self.src().pos().starts_with(consts::PATH) {
            let path_str = self.path()?;

            Ok(PathBuf::from(path_str))
        }
        // is a context variable
        else {
            let alias_str = self.alias(tag_name)?;
            Ok(self.enforce_context(|ctx| ctx.get_path(alias_str))?)
        }
    }

    fn extends(&mut self) -> Result<bool> {
        if !self.src().pos().starts_with(consts::block::EXTENDS) {
            return Ok(false);
        }

        let start = self.src().coord();

        // make sure that the file can still extend another
        self.disallowed_after_extends()?;

        // take away the beginning of the block
        self.src_mut().take(consts::block::EXTENDS.len());

        // check for unexpected eof
        self.unexpected_eof(|| Error::UnterminatedTag(
            "extends".to_owned(), start, self.src().file().to_owned()
        ))?;

        // trim until the first characters
        self.src_mut().trim_start();

        let path = self.pathlike("extends")?;
        let path = self.normalize_path(path);

        // trim until the closing tag
        self.src_mut().trim_start();

        // check for unexpected eof
        self.unexpected_eof(|| Error::UnterminatedTag(
            "extends".to_owned(), start, self.src().file().to_owned()
        ))?;

        if !self.src().pos().starts_with(consts::block::ENDBLOCK) {
            return Err(self.illegal_character("extends"));
        }

        self.src_mut().take(1);

        self.extends = Some(path);

        Ok(true)
    }

    fn source(&mut self) -> Result<bool> {
        if !self.src().pos().starts_with(consts::block::SOURCE) {
            return Ok(false);
        }

        let start = self.src().coord();

        // pass beginning tag
        self.src_mut().take(consts::block::SOURCE.len());

        // trim
        self.src_mut().trim_start();

        self.unexpected_eof(|| Error::UnterminatedTag(
            "source".to_owned(),
            start,
            self.src().file().to_owned(),
        ))?;

        let path = self.pathlike("source")?;
        let path = self.normalize_path(path);

        self.src_mut().trim_start();
        self.unexpected_eof(|| Error::UnterminatedTag(
            "source".to_owned(),
            start,
            self.src().file().to_owned(),
        ))?;

        let as_name = if self.src().pos().starts_with(consts::block::MODIFIER) {
            self.src_mut().take(1);
            self.src_mut().trim_start();
            self.unexpected_eof(|| Error::UnterminatedTag(
                "source".to_owned(),
                start,
                self.src().file().to_owned(),
            ))?;

            if !self.src().pos().starts_with(consts::modif::AS) {
                return Err(self.illegal_character("source"));
            }

            self.src_mut().take(consts::modif::AS.len());
            self.src_mut().trim_start();
            self.unexpected_eof(|| Error::UnterminatedTag(
                "source".to_owned(),
                start,
                self.src().file().to_owned(),
            ))?;

            let alias = self.alias("source")?;

            self.src_mut().trim_start();
            self.unexpected_eof(|| Error::UnterminatedTag(
                "source".to_owned(),
                start,
                self.src().file().to_owned(),
            ))?;

            Some(alias)
        }
        else {
            None
        };

        if !self.src().pos().starts_with(consts::block::ENDBLOCK) {
            return Err(self.illegal_character("source"));
        }

        self.src_mut().take(1);

        if let Some(as_name) = as_name {
            self.read_ctx_in_as(path, as_name)?;
        }
        else {
            self.read_ctx_in(path)?;
        }

        Ok(true)
    }

    fn include_content_mod(&mut self, start: Coordinate) -> Result<Option<Vec<IncludeContentMod>>> {
        if !self.src().pos().starts_with(consts::block::MODIFIER) {
            return Ok(None);
        }

        let mut mods = Vec::new();

        while self.src().pos().starts_with(consts::block::MODIFIER) {
            self.src_mut().take(1);
            self.src_mut().trim_start();
            self.unexpected_eof(|| Error::UnterminatedTag(
                "include-content".to_owned(),
                start,
                self.src().file().to_owned(),
            ))?;

            if self.src().pos().starts_with(consts::modif::PATH) {
                self.src_mut().take(consts::modif::PATH.len());
                mods.push(IncludeContentMod::Path);
            }
            else if self.src().pos().starts_with(consts::modif::UPPER) {
                self.src_mut().take(consts::modif::UPPER.len());
                mods.push(IncludeContentMod::Upper);
            }
            else if self.src().pos().starts_with(consts::modif::LOWER) {
                self.src_mut().take(consts::modif::LOWER.len());
                mods.push(IncludeContentMod::Lower);
            }
            else if self.src().pos().starts_with(consts::modif::TRIM) {
                self.src_mut().take(consts::modif::TRIM.len());
                mods.push(IncludeContentMod::Trim);
            }
            else if self.src().pos().starts_with(consts::modif::SPLIT) {
                self.src_mut().take(consts::modif::SPLIT.len());
                self.src_mut().trim_start();
                self.unexpected_eof(|| Error::UnterminatedTag(
                    "include-content split-modifier".to_owned(),
                    start,
                    self.src().file().to_owned(),
                ))?;

                let mut split_into = String::new();
                while !self.src().eof() &&
                    self.src().pos().starts_with(&[
                        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9'
                    ])
                {
                    split_into.push_str(&self.src_mut().take(1).unwrap());
                }

                self.unexpected_eof(|| Error::UnterminatedTag(
                    "include-content split-modifier".to_owned(),
                    start,
                    self.src().file().to_owned(),
                ))?;

                if split_into.is_empty() {
                    return Err(Error::IllegalCharacter(
                        "include-content split-modifier".to_owned(),
                        self.src().pos().chars().next().unwrap(),
                        self.src().coord(),
                        self.src().file().to_owned()
                    ));
                }

                let split_into = split_into.parse::<usize>().unwrap();

                self.src_mut().trim_start();

                let mut split_idx = String::new();
                while !self.src().eof() &&
                    self.src().pos().starts_with(&[
                        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9'
                    ])
                {
                    split_idx.push_str(&self.src_mut().take(1).unwrap());
                }

                self.unexpected_eof(|| Error::UnterminatedTag(
                    "include-content split-modifier".to_owned(),
                    start,
                    self.src().file().to_owned(),
                ))?;

                if split_idx.is_empty() {
                    return Err(Error::IllegalCharacter(
                        "include-content split-modifier".to_owned(),
                        self.src().pos().chars().next().unwrap(),
                        self.src().coord(),
                        self.src().file().to_owned()
                    ));
                }

                let split_idx = split_idx.parse::<usize>().unwrap();

                if split_into < 2 || split_idx >= split_into {
                    return Err(Error::IllegalSplit(
                        split_into, split_idx, self.src().coord(), self.file().to_owned()
                    ));
                }

                mods.push(IncludeContentMod::Split(split_into, split_idx));
            }
            else if self.src().pos().starts_with(consts::modif::REPLACE) {
                self.src_mut().take(consts::modif::REPLACE.len());
                self.src_mut().trim_start();
                self.unexpected_eof(|| Error::UnterminatedTag(
                    "include-content".to_owned(),
                    start,
                    self.src().file().to_owned(),
                ))?;

                if !self.src().pos().starts_with(consts::PATH) {
                    return Err(self.illegal_character("include-content"));
                }

                let from = self.path()?;

                self.src_mut().trim_start();
                self.unexpected_eof(|| Error::UnterminatedTag(
                    "include-content".to_owned(),
                    start,
                    self.src().file().to_owned(),
                ))?;

                if !self.src().pos().starts_with(consts::PATH) {
                    return Err(self.illegal_character("include-content"));
                }

                let to = self.path()?;

                mods.push(IncludeContentMod::Replace(from, to));
            }
            else {
                return Err(self.illegal_character("include-content"));
            }

            self.src_mut().trim_start();
            self.unexpected_eof(|| Error::UnterminatedTag(
                "include-content".to_owned(),
                self.src().coord(),
                self.src().file().to_owned(),
            ))?;
        }

        Ok(Some(mods))
    }

    fn include_content(&mut self, bypass: bool) -> Result<bool> {
        if !self.src().pos().starts_with(consts::block::INCLUDE_CONTENT) {
            return Ok(false);
        }

        let start = self.src().coord();

        self.src_mut().take(consts::block::INCLUDE_CONTENT.len());
        self.src_mut().trim_start();

        self.unexpected_eof(|| Error::UnterminatedTag(
            "include-content".to_owned(),
            start,
            self.src().file().to_owned(),
        ))?;

        let alias = self.alias("include-content")?;
        let nullable = if self.src().pos().starts_with(consts::exp::NULLABLE) {
            self.src_mut().take(1);
            true
        }
        else {
            false
        };

        self.src_mut().trim_start();
        self.unexpected_eof(|| Error::UnterminatedTag(
            "include-content".to_owned(),
            start,
            self.src().file().to_owned(),
        ))?;

        let mods = self.include_content_mod(start)?;

        if !self.src().pos().starts_with(consts::block::ENDBLOCK) {
            return Err(self.illegal_character("include-content"));
        }

        self.src_mut().take(1).unwrap();

        let is_path = if let Some(mods) = mods.as_ref() {
            mods.iter().any(|v| v.eq(&IncludeContentMod::Path))
        }
        else {
            false
        };

        let mut value = if bypass {
            "".to_owned()
        }
        else if nullable {
            if is_path {
                self.optional_context(|ctx| ctx.get_path_opt(alias))?
                    .unwrap_or(PathBuf::new())
                    .to_str()
                    .unwrap_or("")
                    .to_owned()
            }
            else {
                self.optional_context(|ctx| ctx.get_stringlike_opt(alias))?
                    .unwrap_or(String::new())
            }
        }
        else if is_path {
            self.enforce_context(|ctx| ctx.get_path(alias))?
                .to_str()
                .unwrap_or("")
                .to_owned()
        }
        else {
            self.enforce_context(|ctx| ctx.get_stringlike(alias))?
        };

        if let Some(mods) = mods {
            for m in mods {
                value = match m {
                    IncludeContentMod::Upper => value.to_uppercase(),
                    IncludeContentMod::Lower => value.to_lowercase(),
                    IncludeContentMod::Replace(from, to) => value
                        .replace(&from, &to),
                    IncludeContentMod::Path => value,
                    IncludeContentMod::Split(into, idx) => {
                        let l = value.len();
                        if into > l {
                            return Err(Error::IllegalSplit(
                                into, idx, self.src().coord(), self.file().to_owned()
                            ));
                        }

                        let mut start_end = None;

                        let mut start_idx = 0;
                        for i in 0..into {
                            let end_idx;
                            if i == into - 1 {
                                end_idx = l;
                            }
                            else {
                                end_idx = start_idx + (l / into);
                            }

                            if i == idx {
                                start_end = Some((start_idx, end_idx));
                            }

                            start_idx = end_idx;
                        }

                        let start_end = start_end.unwrap();
                        value[start_end.0..start_end.1].to_owned()
                    },
                    IncludeContentMod::Trim => value.trim().to_owned(),
                }
            }
        }

        self.output.push_str(&value);

        Ok(true)
    }

    fn include_file_mods(&mut self) -> Result<Option<Vec<IncludeFileMod>>> {
        if !self.src().pos().starts_with(consts::block::MODIFIER) {
            return Ok(None);
        }

        let start = self.src().coord();
        let mut mods = Vec::new();

        while self.src().pos().starts_with(consts::block::MODIFIER) {
            self.src_mut().take(1);
            self.src_mut().trim_start();
            self.unexpected_eof(|| Error::UnterminatedTag(
                "include-file".to_owned(),
                start,
                self.src().file().to_owned(),
            ))?;

            if self.src().pos().starts_with(consts::modif::RAW) {
                self.src_mut().take(consts::modif::RAW.len());
                mods.push(IncludeFileMod::Raw);
            }
            else if self.src().pos().starts_with(consts::modif::MD) {
                self.src_mut().take(consts::modif::MD.len());
                mods.push(IncludeFileMod::Md);
            }
            else {
                return Err(self.illegal_character("include-file"));
            }

            self.src_mut().trim_start();
            self.unexpected_eof(|| Error::UnterminatedTag(
                "include-file".to_owned(),
                self.src().coord(),
                self.src().file().to_owned(),
            ))?;
        }

        Ok(Some(mods))
    }

    fn chain_start<S>(&mut self, tag: S, coord: Coordinate) -> Result<()>
    where
        S: AsRef<str>
    {
        if self.src().pos().starts_with(consts::block::CHAIN) {
            // take chain and trim until startblock
            self.src_mut().take(1);
            self.src_mut().trim_start_multiline();
        }

        self.unexpected_eof(|| Error::UnterminatedTag(
            tag.as_ref().to_owned(),
            coord,
            self.src().file().to_owned()
        ))?;

        if !self.src().pos().starts_with(consts::block::STARTBLOCK) {
            return Err(self.illegal_character(tag));
        }

        // take startblock
        self.src_mut().take(1);

        Ok(())
    }

    fn include_file_parse<P>(&mut self, path: P, is_raw: bool, is_md: bool, bypass: bool) -> Result<String>
    where
        P: AsRef<Path>
    {
        if bypass {
            Ok(String::new())
        }
        else if is_raw && is_md {
            NfmParser::parse_file(&path).map_err(|e| Error::IO(e, path.as_ref().into()))
        }
        else if is_raw {
            read_file(path)
        }
        else if is_md {
            let output = self.spawn_sealed_parser(path, |p| p.parse())?;
            Ok(NfmParser::parse_str(&output))
        }
        else {
            self.spawn_sealed_parser(path, |p| p.parse())
        }
    }

    fn include_file(&mut self, bypass: bool) -> Result<bool> {
        if !self.src().pos().starts_with(consts::block::INCLUDE_FILE) {
            return Ok(false);
        }

        const TAG_NAME: &str = "include-file";
        let start = self.src().coord();

        self.src_mut().take(consts::block::INCLUDE_FILE.len());
        self.src_mut().trim_start();

        fn unexpected_eof(p: &mut Parser, coord: Coordinate) -> Result<()> {
            p.unexpected_eof(|| Error::UnterminatedTag(
                TAG_NAME.to_owned(),
                coord,
                p.src().file().to_owned(),
            ))
        }

        unexpected_eof(self, start)?;

        let path = self.pathlike(TAG_NAME)?;
        let path = self.normalize_path(path);

        self.src_mut().trim_start();

        unexpected_eof(self, start)?;

        let mods = self.include_file_mods()?;

        let is_raw = if let Some(mods) = &mods {
            mods.iter().any(|m| m.eq(&IncludeFileMod::Raw))
        }
        else {
            false
        };

        let is_md = if let Some(mods) = &mods {
            mods.iter().any(|m| m.eq(&IncludeFileMod::Md))
        }
        else {
            false
        };

        if !self.src().pos().starts_with(consts::block::ENDBLOCK) {
            return Err(self.illegal_character(TAG_NAME));
        }

        self.src_mut().take(1);

        if self.src().pos().starts_with(consts::block::CHAIN) ||
            self.src().pos().starts_with(consts::block::STARTBLOCK)
        {
            self.chain_start(TAG_NAME, start)?;

            let output = self.spawn_sealed_internal_parser(|p| {
                while !p.src().eof() && !p.src().pos().starts_with(consts::block::ENDBLOCK) {
                    p.parse_next(bypass)?;
                }

                unexpected_eof(p, start)?;
                p.src_mut().take(1);

                let block_output = std::mem::take(&mut p.output);
                p.set_value(consts::CONTENT, block_output)?;

                let mut file_output = p.include_file_parse(path, is_raw, is_md, bypass)?;
                std::mem::swap(&mut file_output, &mut p.output);
                Ok(())
            })?;

            self.output.push_str(&output);
        }
        else {
            let output = self.include_file_parse(path, is_raw, is_md, bypass)?;
            self.output.push_str(&output);
        }


        Ok(true)
    }

    fn if_condition(&mut self) -> IfCondition {
        if self.src().pos().starts_with(consts::exp::EXISTS) {
            self.src_mut().take(consts::exp::EXISTS.len());
            IfCondition::Exists
        }
        else if self.src().pos().starts_with(consts::exp::EMPTY) {
            self.src_mut().take(consts::exp::EMPTY.len());
            IfCondition::Empty
        }
        else {
            IfCondition::Exists
        }
    }

    fn chain_or_end<S>(&mut self, tag: S, coord: Coordinate) -> Result<bool>
    where
        S: AsRef<str>
    {
        // if eof or no chain and no startblock, then it is a valid endpoint
        if self.src().eof() ||
            (
                !self.src().pos().starts_with(consts::block::CHAIN) &&
                !self.src().pos().starts_with(consts::block::STARTBLOCK)
            )
        {
            return Ok(true);
        }

        if self.src().pos().starts_with(consts::block::CHAIN) {
            // take chain
            self.src_mut().take(1);
            self.src_mut().trim_start_multiline();
            self.unexpected_eof(|| Error::UnterminatedTag(
                tag.as_ref().to_owned(),
                coord,
                self.src().file().to_owned()
            ))?;

            if !self.src().pos().starts_with(consts::block::STARTBLOCK) {
                return Err(self.illegal_character(format!("else-{}", tag.as_ref())));
            }
        }

        // must be startblock
        self.src_mut().take(1);

        Ok(false)
    }

    fn if_is_true<A>(&mut self, negate: bool, condition: IfCondition, alias: A) -> Result<bool>
    where
        A: Into<Alias>
    {
        // set pseudo value to avoid errors, output will be ditched if
        // the condition doesn't match
        match &condition {
            IfCondition::Empty => {
                match self.optional_context(|ctx| Ok(Some(ctx.is_empty(alias)?)))? {
                    // has value
                    Some(is_empty) => if !negate { Ok(is_empty) } else { Ok(!is_empty) },
                    // no value
                    None => if !negate { Ok(true) } else { Ok(false) },
                }
            },
            IfCondition::Exists => {
                match self.optional_context(|ctx| Ok(Some(ctx.exists(alias)?)))? {
                    // has value
                    Some(exists) => if !negate { Ok(exists) } else { Ok(!exists) },
                    // no value
                    None => if !negate { Ok(false) } else { Ok(true) },
                }
            },
        }
    }

    fn if_tag(&mut self, bypass: bool) -> Result<bool> {
        if !self.src().pos().starts_with(consts::block::IF) {
            return Ok(false);
        }

        let start = self.src().coord();

        fn unexpected_eof_if(p: &mut Parser, coords: Coordinate) -> Result<()> {
            p.unexpected_eof(|| Error::UnterminatedTag(
                "if".to_owned(),
                coords,
                p.src().file().to_owned(),
            ))
        }

        // take if block
        self.src_mut().take(consts::block::IF.len());
        self.src_mut().trim_start();
        unexpected_eof_if(self, start)?;

        let negate = if self.src().pos().starts_with(consts::exp::NOT) {
            self.src_mut().take(1);
            self.src_mut().trim_start();
            unexpected_eof_if(self, start)?;

            true
        }
        else {
            false
        };

        let alias = self.alias("if")?;

        self.src_mut().trim_start();
        unexpected_eof_if(self, start)?;

        let condition = self.if_condition();
        self.src_mut().trim_start();
        unexpected_eof_if(self, start)?;

        if !self.src().pos().starts_with(consts::block::ENDBLOCK) {
            return Err(self.illegal_character("if"));
        }

        self.src_mut().take(1);

        self.chain_start("if", start)?;

        // parse if contents
        let is_true = self.if_is_true(negate, condition, alias)?;

        let if_output = self.spawn_sealed_internal_parser(|p| {
            let start = p.src().coord();

            while !p.src().eof() && !p.src().pos().starts_with(consts::block::ENDBLOCK) {
                p.parse_next(bypass || !is_true)?;
            }

            unexpected_eof_if(p, start)?;
            p.src_mut().take(1);

            Ok(())
        })?;

        if is_true {
            self.output.push_str(&if_output);
        }

        let start = self.src().coord();
        fn unexpected_eof_else(p: &mut Parser, coords: Coordinate) -> Result<()> {
            p.unexpected_eof(|| Error::UnterminatedTag(
                "else".to_owned(),
                coords,
                p.src().file().to_owned(),
            ))
        }

        // if eof or no chain and no startblock, then it is a valid endpoint
        if self.chain_or_end("if", start)? {
            return Ok(true);
        }

        // parse else contents
        let else_output = self.spawn_sealed_internal_parser(|p| {
            let start = p.src().coord();

            while !p.src().eof() && !p.src().pos().starts_with(consts::block::ENDBLOCK) {
                p.parse_next(bypass || is_true)?;
            }

            unexpected_eof_else(p, start)?;
            p.src_mut().take(1);

            Ok(())
        })?;

        if !is_true {
            self.output.push_str(&else_output);
        }

        Ok(true)
    }

    fn for_file_mods(&mut self, start: Coordinate) -> Result<Option<Vec<ForFileMod>>> {
        if !self.src().pos().starts_with(consts::block::MODIFIER) {
            return Ok(None);
        }

        const TAG_NAME: &str = "for-file";

        fn unexpected_eof(p: &mut Parser, coord: Coordinate) -> Result<()> {
            p.unexpected_eof(|| Error::UnterminatedTag(
                TAG_NAME.to_owned(),
                coord,
                p.src().file().to_owned(),
            ))
        }

        let mut mods = Vec::new();

        while self.src().pos().starts_with(consts::block::MODIFIER) {
            self.src_mut().take(1);
            self.src_mut().trim_start();
            unexpected_eof(self, start)?;

            if self.src().pos().starts_with(consts::modif::EXT) {
                self.src_mut().take(consts::modif::EXT.len());
                self.src_mut().trim_start();
                let path = self.path()?;

                mods.push(ForFileMod::Ext(path));
            }
            else if self.src().pos().starts_with(consts::modif::REVERSE) {
                self.src_mut().take(consts::modif::REVERSE.len());
                mods.push(ForFileMod::Reverse);
            }
            else {
                return Err(self.illegal_character(TAG_NAME));
            }

            self.src_mut().trim_start();
            self.unexpected_eof(|| Error::UnterminatedTag(
                TAG_NAME.to_owned(),
                self.src().coord(),
                self.src().file().to_owned(),
            ))?;
        }

        Ok(Some(mods))
    }

    fn get_value<F>(&mut self, from: F) -> Result<&JsonValue>
    where
        F: Into<Alias>
    {
        if let Some(ctx) = self.ctx_mut() {
            Ok(ctx.get_value(from)?)
        }
        else {
            Err(Error::ValueNotFound(from.into()))
        }
    }

    fn clone_value<F, T>(&mut self, from: F, to: T) -> Result<()>
    where
        F: Into<Alias>,
        T: Into<Alias>
    {
        let value = self.get_value(from)?.to_owned();
        self.context.as_mut().unwrap().set_value(to, value)
    }

    fn set_value<A, S>(&mut self, alias: A, val: S) -> Result<()>
    where
        A: Into<Alias>,
        S: AsRef<str>
    {
        if let Some(ctx) = self.ctx_mut() {
            ctx.set_stringlike(alias, val)?;
        }
        else {
            let mut new_ctx = JsonContext::faux_context(&self.path)?;
            new_ctx.set_stringlike(alias, val)?;
            self.context = Some(new_ctx);
        }

        Ok(())
    }

    fn push_stringlike<A, S>(&mut self, alias: A, val: S) -> Result<()>
    where
        A: Into<Alias>,
        S: AsRef<str>,
    {
        if let Some(ctx) = self.ctx_mut() {
            ctx.push_stringlike(alias, val)?;
        }
        else {
            let mut new_ctx = JsonContext::faux_context(&self.path)?;
            new_ctx.push_stringlike(alias, val)?;
            self.context = Some(new_ctx);
        }

        Ok(())
    }

    fn push_pathlike<A, S, P>(&mut self, alias: A, val: S, path: P) -> Result<()>
    where
        A: Into<Alias>,
        S: AsRef<str>,
        P: AsRef<Path>,
    {
        if let Some(ctx) = self.ctx_mut() {
            ctx.push_pathlike(alias, val, path)?;
        }
        else {
            let mut new_ctx = JsonContext::faux_context(&self.path)?;
            new_ctx.push_pathlike(alias, val, path)?;
            self.context = Some(new_ctx);
        }

        Ok(())
    }

    fn pop_value<A>(&mut self, alias: A) -> Result<()>
    where
        A: Into<Alias>
    {
        if let Some(ctx) = self.ctx_mut() {
            ctx.pop_stringlike(alias)?;
        }

        Ok(())
    }

    fn set_path<A, P, V>(&mut self, path: P, alias: A, value: V) -> Result<()>
    where
        A: Into<Alias>,
        P: AsRef<Path>,
        V: AsRef<str>
    {
        if let Some(ctx) = self.ctx_mut() {
            ctx.set_path(path, alias, value)?;
        }
        else {
            let mut new_ctx = JsonContext::faux_context(&self.path)?;
            new_ctx.set_path(path, alias, value)?;
            self.context = Some(new_ctx);
        }

        Ok(())
    }

    fn set_json_value<A>(&mut self, alias: A, val: JsonValue) -> Result<()>
    where
        A: Into<Alias>
    {
        if let Some(ctx) = self.ctx_mut() {
            ctx.set_value(alias, val)?;
        }
        else {
            let mut new_ctx = JsonContext::faux_context(&self.path)?;
            new_ctx.set_value(alias, val)?;
            self.context = Some(new_ctx);
        }

        Ok(())
    }

    fn remove_value<A>(&mut self, alias: A)
    where
        A: Into<Alias>,
    {
        if self.ctx().is_none() {
            return;
        }

        self.ctx_mut().as_mut().unwrap().remove(alias);
    }

    fn in_keyword<S>(&mut self, tag: S) -> Result<()>
    where
        S: AsRef<str>
    {
        if !self.src().pos().starts_with(consts::exp::IN) {
            return Err(self.illegal_character(tag));
        }

        self.src_mut().take(consts::exp::IN.len());
        self.src_mut().trim_start();

        Ok(())
    }

    fn loop_context(&mut self, idx: usize, len: usize) -> Result<()> {
        self.set_value("$loop.index", idx.to_string())?;
        self.set_value("$loop.position", (idx + 1).to_string())?;
        self.set_value("$loop.length", len.to_string())?;
        self.set_value("$loop.max", (len - 1).to_string())?;

        if idx == 0 {
            self.set_value("$loop.first", true.to_string())?;
        }
        else {
            self.remove_value("$loop.first");
        }

        if idx == len - 1 {
            self.set_value("$loop.last", true.to_string())?;
        }
        else {
            self.remove_value("$loop.last");
        }

        Ok(())
    }

    fn for_file(&mut self, bypass: bool) -> Result<bool> {
        if !self.src().pos().starts_with(consts::block::FOR_FILE) {
            return Ok(false);
        }

        let start = self.src().coord();

        const TAG_NAME: &str = "for-file";
        fn unexpected_eof_for(p: &mut Parser, coord: Coordinate) -> Result<()> {
            p.unexpected_eof(|| Error::UnterminatedTag(
                TAG_NAME.to_owned(),
                coord,
                p.src().file().to_owned()
            ))
        }

        self.src_mut().take(consts::block::FOR_FILE.len());
        self.src_mut().trim_start();
        unexpected_eof_for(self, start)?;

        let alias = self.alias(TAG_NAME)?;
        self.src_mut().trim_start();
        unexpected_eof_for(self, start)?;

        // take "in"
        self.in_keyword(TAG_NAME)?;
        unexpected_eof_for(self, start)?;

        let path = self.pathlike(TAG_NAME)?;
        let path = self.normalize_path(path);

        self.src_mut().trim_start();
        let mods = self.for_file_mods(start)?;
        self.src_mut().trim_start();
        unexpected_eof_for(self, start)?;

        if !self.src().pos().starts_with(consts::block::ENDBLOCK) {
            return Err(self.illegal_character(TAG_NAME));
        }

        // take endblock
        self.src_mut().take(1);

        // handle chain and startblock
        self.chain_start("for-file", start)?;

        let for_start = self.src().coord();

        let extensions = mods.as_ref()
            .map(|m| m.iter()
                .filter_map(|m| if let ForFileMod::Ext(s) = m {
                    Some(s.to_owned())
                }
                else {
                    None
                })
                .collect::<Vec<String>>()
            )
            .unwrap_or(Vec::new());

        let reverse = mods
            .map(|m| m.iter()
                .filter_map(|m| if let ForFileMod::Reverse = m {
                    Some(())
                }
                else {
                    None
                })
                .collect::<Vec<()>>()
            )
            .unwrap_or(Vec::new())
            .len() % 2 != 0;

        let mut items = if bypass {
            vec![]
        }
        else {
            let p = path.clone();
            path.read_dir().map_err(|e| Error::IO(e, p.clone()))?
                .map(|entry_res| {
                    let entry = entry_res.map_err(|e| Error::IO(e, p.clone()))?;
                    let path = entry.path();
                    let ext = if let Some(ext) = path.extension() {
                        if let Some(ext) = ext.to_str() {
                            ext
                        }
                        else {
                            ""
                        }
                    }
                    else {
                        ""
                    };

                    if !path.is_file() ||
                        !extensions.is_empty() && !extensions.contains(&ext.to_owned())
                    {
                        return Ok(None);
                    }

                    Ok(Some(path))
                })
                .collect::<Result<Vec<Option<PathBuf>>>>()?
                .into_iter()
                .flatten()
                .collect::<Vec<PathBuf>>()
        };

        items.sort_unstable();

        if reverse {
            items.reverse();
        }

        let len = items.len();

        let has_items = if items.is_empty() {
            items = vec![ PathBuf::new(), ];
            false
        }
        else {
            true
        };

        let alias_cl = alias.clone();
        for (idx, item) in items.into_iter().enumerate() {
            // revert back to start of loop
            self.src_mut().set_coord(for_start);

            let item_str = if let Some(item_str) = item.to_str() {
                item_str.to_owned()
            }
            else {
                "".to_owned()
            };

            let for_output = self.spawn_sealed_internal_parser(|p| {
                // place value into map
                p.set_value(alias_cl.clone(), item_str)?;

                // setup loop context
                if !bypass && has_items {
                    p.loop_context(idx, len)?;
                }

                // parse next until endblock.
                while !p.src().eof() && !p.src().pos().starts_with(consts::block::ENDBLOCK) {
                    p.parse_next(bypass || !has_items)?;
                }

                // check for eof
                unexpected_eof_for(p, start)?;

                // take endblock char
                p.src_mut().take(1);

                Ok(())
            })?;

            if !bypass && has_items {
                self.output.push_str(&for_output);
            }

            if self.chain_or_end("for-file", start)? {
                continue;
            }

            fn unexpected_eof_else(p: &mut Parser, coord: Coordinate) -> Result<()> {
                p.unexpected_eof(|| Error::UnterminatedTag(
                    "else-for-file".to_owned(),
                    coord,
                    p.src().file().to_owned(),
                ))
            }

            let else_output = self.spawn_sealed_internal_parser(|p| {
                // parse next until endblock.
                while !p.src().eof() && !p.src().pos().starts_with(consts::block::ENDBLOCK) {
                    p.parse_next(bypass || has_items)?;
                }

                // check for eof
                unexpected_eof_else(p, start)?;

                // take endblock char
                p.src_mut().take(1);

                Ok(())
            })?;

            if !bypass && !has_items {
                self.output.push_str(&else_output);
            }
        }

        Ok(true)
    }

    fn for_item_mods(&mut self, start: Coordinate) -> Result<Option<Vec<ForItemMod>>> {
        if !self.src().pos().starts_with(consts::block::MODIFIER) {
            return Ok(None);
        }

        const TAG_NAME: &str = "for-item";

        fn unexpected_eof(p: &mut Parser, coord: Coordinate) -> Result<()> {
            p.unexpected_eof(|| Error::UnterminatedTag(
                TAG_NAME.to_owned(),
                coord,
                p.src().file().to_owned(),
            ))
        }

        let mut mods = Vec::new();

        while self.src().pos().starts_with(consts::block::MODIFIER) {
            self.src_mut().take(1);
            self.src_mut().trim_start();
            unexpected_eof(self, start)?;

            if self.src().pos().starts_with(consts::modif::REVERSE) {
                self.src_mut().take(consts::modif::REVERSE.len());
                mods.push(ForItemMod::Reverse);
            }
            else if self.src().pos().starts_with(consts::modif::PATHS) {
                self.src_mut().take(consts::modif::PATHS.len());
                mods.push(ForItemMod::Paths);
            }
            else {
                return Err(self.illegal_character(TAG_NAME));
            }

            self.src_mut().trim_start();
            self.unexpected_eof(|| Error::UnterminatedTag(
                TAG_NAME.to_owned(),
                self.src().coord(),
                self.src().file().to_owned(),
            ))?;
        }

        Ok(Some(mods))
    }

    fn for_item(&mut self, bypass: bool) -> Result<bool> {
        if !self.src().pos().starts_with(consts::block::FOR_ITEM) {
            return Ok(false);
        }

        let start = self.src().coord();

        const TAG_NAME: &str = "for-item";
        fn unexpected_eof_for(p: &mut Parser, coord: Coordinate) -> Result<()> {
            p.unexpected_eof(|| Error::UnterminatedTag(
                TAG_NAME.to_owned(),
                coord,
                p.src().file().to_owned()
            ))
        }

        self.src_mut().take(consts::block::FOR_ITEM.len());
        self.src_mut().trim_start();
        unexpected_eof_for(self, start)?;

        let alias = self.alias(TAG_NAME)?;

        let nullable = if self.src().pos().starts_with(consts::exp::NULLABLE) {
            self.src_mut().take(1);
            true
        }
        else {
            false
        };

        self.src_mut().trim_start();
        unexpected_eof_for(self, start)?;

        // take "in"
        self.in_keyword(TAG_NAME)?;
        unexpected_eof_for(self, start)?;

        let in_alias = self.alias(TAG_NAME)?;

        self.src_mut().trim_start();
        let mods = self.for_item_mods(start)?;
        self.src_mut().trim_start();
        unexpected_eof_for(self, start)?;

        if !self.src().pos().starts_with(consts::block::ENDBLOCK) {
            return Err(self.illegal_character(TAG_NAME));
        }

        // take endblock
        self.src_mut().take(1);

        // handle chain and startblock
        self.chain_start(TAG_NAME, start)?;

        let for_start = self.src().coord();

        let reverse = mods.as_ref()
            .map(|m| m.iter()
                .filter_map(|m| if let ForItemMod::Reverse = m {
                    Some(())
                }
                else {
                    None
                })
                .collect::<Vec<()>>()
            )
            .unwrap_or(Vec::new())
            .len() % 2 != 0;

        let as_paths = !mods
            .map(|m| m.iter()
                .filter_map(|m| if let ForItemMod::Paths = m {
                    Some(())
                }
                else {
                    None
                })
                .collect::<Vec<()>>()
            )
            .unwrap_or(Vec::new())
            .is_empty();

        let mut items = if bypass {
            vec![]
        }
        else if let Some(ctx) = self.ctx_mut() {
            if as_paths {
                if nullable {
                    ctx.get_array_opt_as_paths(in_alias)?
                }
                else {
                    ctx.get_array_as_paths(in_alias)?
                }
            }
            else if nullable {
                ctx.get_array_opt(in_alias)?
            }
            else {
                ctx.get_array(in_alias)?
            }
        }
        else {
            vec![]
        };

        if reverse {
            items.reverse();
        }

        let len = items.len();

        let has_items = if items.is_empty() {
            items = vec![ JsonValue::String("".to_owned()), ];
            false
        }
        else {
            true
        };

        let alias_cl = alias.clone();
        for (idx, item) in items.into_iter().enumerate() {
            // revert back to start of loop
            self.src_mut().set_coord(for_start);

            let for_output = self.spawn_sealed_internal_parser(|p| {
                // place value into map
                p.set_json_value(alias_cl.clone(), item)?;

                // setup loop context
                if !bypass && has_items {
                    p.loop_context(idx, len)?;
                }

                // parse next until endblock.
                while !p.src().eof() && !p.src().pos().starts_with(consts::block::ENDBLOCK) {
                    p.parse_next(bypass || !has_items)?;
                }

                // check for eof
                unexpected_eof_for(p, start)?;

                // take endblock char
                p.src_mut().take(1);

                Ok(())
            })?;

            if !bypass && has_items {
                self.output.push_str(&for_output);
            }

            if self.chain_or_end(TAG_NAME, start)? {
                continue;
            }

            fn unexpected_eof_else(p: &mut Parser, coord: Coordinate) -> Result<()> {
                p.unexpected_eof(|| Error::UnterminatedTag(
                    format!("else-{TAG_NAME}"),
                    coord,
                    p.src().file().to_owned(),
                ))
            }

            let else_output = self.spawn_sealed_internal_parser(|p| {
                // parse next until endblock.
                while !p.src().eof() && !p.src().pos().starts_with(consts::block::ENDBLOCK) {
                    p.parse_next(bypass || has_items)?;
                }

                // check for eof
                unexpected_eof_else(p, start)?;

                // take endblock char
                p.src_mut().take(1);

                Ok(())
            })?;

            if !bypass && !has_items {
                self.output.push_str(&else_output);
            }
        }

        Ok(true)
    }

    fn set_item(&mut self, bypass: bool) -> Result<bool> {
        if !self.src().pos().starts_with(consts::block::SET_ITEM) {
            return Ok(false);
        }

        const TAG_NAME: &str = "set-item";

        self.src_mut().take(consts::block::SET_ITEM.len());
        self.src_mut().trim_start();

        fn unexpected_eof(p: &mut Parser, coord: Coordinate) -> Result<()> {
            p.unexpected_eof(|| Error::UnterminatedTag(
                TAG_NAME.to_owned(),
                coord,
                p.src().file().to_owned(),
            ))
        }

        let start = self.src().coord();
        unexpected_eof(self, start)?;

        let alias = self.alias(TAG_NAME)?;

        self.src_mut().trim_start();
        unexpected_eof(self, start)?;

        let mut mods = Vec::new();
        while self.src().pos().starts_with(consts::block::MODIFIER) {
            self.src_mut().take(1);
            self.src_mut().trim_start();
            unexpected_eof(self, start)?;

            if self.src().pos().starts_with(consts::modif::PATH) {
                self.src_mut().take(consts::modif::PATH.len());
                mods.push(SetValueMod::Path);
            }
            else if self.src().pos().starts_with(consts::modif::ARRAY) {
                self.src_mut().take(consts::modif::ARRAY.len());
                mods.push(SetValueMod::Array);
            }
            else {
                return Err(self.illegal_character(TAG_NAME));
            }

            self.src_mut().trim_start();
        }

        unexpected_eof(self, start)?;

        if !self.src().pos().starts_with(consts::block::ENDBLOCK) {
            return Err(self.illegal_character(TAG_NAME));
        }

        let is_arr = mods.iter().any(|v| v.eq(&SetValueMod::Array));
        let is_path = mods.iter().any(|v| v.eq(&SetValueMod::Path));

        self.src_mut().take(1);

        unexpected_eof(self, start)?;

        if !self.src().pos().starts_with(consts::block::CHAIN) &&
            !self.src().pos().starts_with(consts::block::STARTBLOCK) &&
            !self.src().pos().starts_with(consts::block::SIPHON)
        {
            return Err(self.illegal_character(TAG_NAME));
        }

        // if we are siphoning, clone the value from the siphon's alias to the
        // previously defined alias
        if self.src().pos().starts_with(consts::block::SIPHON) {
            self.src_mut().take(consts::block::SIPHON.len());
            self.src_mut().trim_start();
            unexpected_eof(self, start)?;

            let clone_from = self.alias(TAG_NAME)?;
            self.src_mut().trim_start();
            unexpected_eof(self, start)?;
            if !self.src().pos().starts_with(consts::block::ENDBLOCK) {
                return Err(self.illegal_character(TAG_NAME));
            }
            self.src_mut().take(1);

            if alias.eq(consts::ROOT) && !bypass {
                match self.get_value(&clone_from)?.to_owned() {
                    JsonValue::Object(obj) => {
                        for (k, v) in obj.into_iter() {
                            self.set_json_value(k, v.to_owned())?;
                        }
                    },
                    _ => {
                        return Err(Error::ValueNotObject(clone_from.into()));
                    },
                }
            }
            else {
                self.clone_value(clone_from, alias)?;
            }

            return Ok(true);
        }

        while !self.src().eof() &&
            (
                self.src().pos().starts_with(consts::block::CHAIN) ||
                self.src().pos().starts_with(consts::block::STARTBLOCK)
            )
        {
            self.chain_start(TAG_NAME, start)?;

            let output = self.spawn_sealed_internal_parser(|p| {
                while !p.src().eof() && !p.src().pos().starts_with(consts::block::ENDBLOCK) {
                    p.parse_next(bypass)?;
                }

                unexpected_eof(p, start)?;

                p.src_mut().take(1);

                Ok(())
            })?;

            if is_arr {
                if is_path {
                    self.push_pathlike(alias.clone(), output, self.src().file().clone())?;
                }
                else {
                    self.push_stringlike(alias.clone(), output)?;
                }
            }
            else {
                if is_path {
                    self.set_path(self.src().file().clone(), alias.clone(), output)?;
                }
                else {
                    self.set_value(alias.clone(), output)?;
                }
                break;
            }
        }

        Ok(true)
    }

    fn unset_item(&mut self) -> Result<bool> {
        if !self.src().pos().starts_with(consts::block::UNSET_ITEM) {
            return Ok(false);
        }

        const TAG_NAME: &str = "unset-item";

        self.src_mut().take(consts::block::UNSET_ITEM.len());
        self.src_mut().trim_start();

        fn unexpected_eof(p: &mut Parser, coord: Coordinate) -> Result<()> {
            p.unexpected_eof(|| Error::UnterminatedTag(
                TAG_NAME.to_owned(),
                coord,
                p.src().file().to_owned()
            ))
        }

        let start = self.src().coord();
        unexpected_eof(self, start)?;

        let alias = self.alias(TAG_NAME)?;

        self.src_mut().trim_start();
        unexpected_eof(self, start)?;

        let mut mods = Vec::new();

        while self.src().pos().starts_with(consts::block::MODIFIER) {
            self.src_mut().take(1);
            self.src_mut().trim_start();
            unexpected_eof(self, start)?;

            if self.src().pos().starts_with(consts::modif::POP) {
                self.src_mut().take(consts::modif::POP.len());
                mods.push(UnsetItemMod::Pop);
            }
            else {
                return Err(self.illegal_character(TAG_NAME));
            }

            self.src_mut().trim_start();
        }

        unexpected_eof(self, start)?;

        if !self.src().pos().starts_with(consts::block::ENDBLOCK) {
            return Err(self.illegal_character(TAG_NAME));
        }

        self.src_mut().take(1);

        let is_pop = mods.iter().any(|v| v.eq(&UnsetItemMod::Pop));

        if is_pop {
            self.pop_value(alias)?;
        }
        else {
            self.remove_value(alias);
        }

        Ok(true)
    }

    /// Consume the parser and take its output.
    pub
    fn as_output(self) -> String {
        self.output
    }

    /// Borrow the parser's output.
    pub
    fn output(&self) -> &str {
        &self.output
    }

    fn parse_next(&mut self, bypass: bool) -> Result<()> {
        // is escaped (2 char pattern)
        if self.src().pos().starts_with(consts::block::esc::MODIFIER) ||
            self.src().pos().starts_with(consts::block::esc::IGNORE) ||
            self.src().pos().starts_with(consts::block::esc::COMMENT) ||
            self.src().pos().starts_with(consts::block::esc::EXTENDS) ||
            self.src().pos().starts_with(consts::block::esc::SOURCE) ||
            self.src().pos().starts_with(consts::block::esc::INCLUDE_FILE) ||
            self.src().pos().starts_with(consts::block::esc::INCLUDE_CONTENT) ||
            self.src().pos().starts_with(consts::block::esc::EXPRESSION) ||
            self.src().pos().starts_with(consts::block::esc::SET_ITEM) ||
            self.src().pos().starts_with(consts::block::esc::UNSET_ITEM)
        {
            self.src_mut().take(consts::block::esc::ESCAPE.len());
            let taken = self.src_mut().take(2).unwrap();
            self.output.push_str(&taken);
        }
        // is escaped (1 char pattern)
        else if self.src().pos().starts_with(consts::block::esc::BLOCK) ||
            self.src().pos().starts_with(consts::block::esc::ENDBLOCK)
        {
            self.esc_endblock();
        }
        // is ignored
        else if self.ignore()? ||
            // is a comment
            self.comment()? ||
            // is extending
            self.extends()? ||
            // is sourcing
            self.source()? ||
            // is include-file
            self.include_file(bypass)? ||
            // is include-content
            self.include_content(bypass)? ||
            // is if
            self.if_tag(bypass)? ||
            // is for-file
            self.for_file(bypass)? ||
            // is for-item
            self.for_item(bypass)? ||
            // is set-item
            self.set_item(bypass)? ||
            // is remove-item
            self.unset_item()?
        {
            // no action required
        }
        else {
            let taken = self.src_mut().take(1).unwrap();
            self.output.push_str(&taken);
        }

        Ok(())
    }

    /// Parse the template with which the parser was initialized.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use arcana_core::Parser;
    ///
    /// let mut parser = Parser::new("test/full/1/page.html").unwrap();
    /// parser.parse().unwrap();
    /// assert!(!parser.output().is_empty());
    /// ```
    pub
    fn parse(&mut self) -> Result<()> {
        while !self.src().eof() {
            self.parse_next(false)?;
        }

        if let Some(extends) = self.extends.to_owned() {
            if !self.output.is_empty() {
                let orig_output = std::mem::take(&mut self.output);
                self.set_value(consts::CONTENT, orig_output)?;
            }
            let output = self.spawn_parser(extends, |p| p.parse())?;
            self.output.push_str(&output);
        }

        Ok(())
    }
}
