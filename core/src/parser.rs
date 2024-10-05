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

pub(crate) mod consts;

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
    Filename,
    Upper,
    Lower,
    Path,
    Replace(String, String),
    Split(usize, usize),
    Trim,
    Json,
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
    Files,
    Dirs,
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
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Truthy
}

#[derive(Default)]
struct LoopFile {
    path: PathBuf,
    is_dir: bool,
    is_file: bool,
    ext: Option<String>,
    stem: Option<String>,
    name: Option<String>,
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
    fn new_internal<P>(path: P, content: Option<String>, ctx: Option<JsonContext>) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let abs_path = Self::normalize_initial_path(path)?;
        let source = if let Some(c) = content {
            Source::faux_source(&abs_path, c)
        }
        else {
            Source::read_file(&abs_path)?
        };

        Ok(Self {
            path: abs_path,
            context: ctx,
            extends: None,
            can_extend: true,
            source,
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
        let mut scoped_parser = Self::new_internal(p, None, ctx)?;
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
        let mut scoped_parser = Self::new_internal(p, None, new_ctx)?;
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
        Self::new_internal(path, None, None)
    }

    /// Create a new parser with a specific context.
    ///
    /// # Arguments
    ///
    /// * `template` - The path to the template.
    /// * `context` - The context.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use {
    ///     arcana_core::{ JsonContext, Parser, },
    ///     std::fs::canonicalize,
    /// };
    ///
    /// let ctx = JsonContext::read(canonicalize("test/full/1/page.json").unwrap()).unwrap();
    /// Parser::new_with_context("test/full/1/page.html", ctx).unwrap();
    /// ```
    pub
    fn new_with_context<T>(template: T, context: JsonContext) -> Result<Self>
    where
        T: AsRef<Path>
    {
        Self::new_internal(template, None, Some(context))
    }

    /// Create a new parser with a specific context read from path.
    ///
    /// # Arguments
    ///
    /// * `template` - The path to the template.
    /// * `context` - The path to the context.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use {
    ///     arcana_core::Parser,
    ///     std::fs::canonicalize,
    /// };
    ///
    /// Parser::new_with_context_path("test/full/1/page.html", canonicalize("test/full/1/page.json").unwrap()).unwrap();
    /// ```
    pub
    fn new_with_context_path<T, C>(template: T, context: C) -> Result<Self>
    where
        T: AsRef<Path>,
        C: AsRef<Path>
    {
        let ctx = JsonContext::read(context)?;
        Self::new_internal(template, None, Some(ctx))
    }

    /// Create a new parser with an input string, pseudo-path, and a specific context.
    ///
    /// # Arguments
    ///
    /// * `template` - The path to the template.
    /// * `context` - The path to the context.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use {
    ///     arcana_core::{ JsonContext, Parser },
    ///     std::fs::canonicalize,
    /// };
    ///
    /// let mut p = Parser::from_string_and_path_with_context(
    ///     "./fake.path",
    ///     "${title|replace \" \" \"\"|lower}".to_owned(),
    ///     JsonContext::read(canonicalize("test/full/1/page.json").unwrap()).unwrap()
    /// ).unwrap();
    /// p.parse().unwrap();
    /// assert_eq!("fulltest1", p.as_output());
    /// ```
    pub
    fn from_string_and_path_with_context<T>(template: T, content: String, context: JsonContext) -> Result<Self>
    where
        T: AsRef<Path>,
    {
        Self::new_internal(template, Some(content), Some(context))
    }

    /// Create a new parser with an input string and pseudo-path.
    ///
    /// # Arguments
    ///
    /// * `template` - The path to the template.
    /// * `context` - The path to the context.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use {
    ///     arcana_core::{ JsonContext, Parser },
    ///     std::fs::canonicalize,
    /// };
    ///
    /// let mut p = Parser::from_string_and_path(
    ///     "./fake.path",
    ///     concat!(
    ///         "={title}(\"Here is the title\")\\\n",
    ///         "${title|replace \" \" \"\"|lower}",
    ///     ).to_owned(),
    /// ).unwrap();
    /// p.parse().unwrap();
    /// assert_eq!("hereisthetitle", p.as_output());
    /// ```
    pub
    fn from_string_and_path<T>(template: T, content: String) -> Result<Self>
    where
        T: AsRef<Path>,
    {
        Self::new_internal(template, Some(content), None)
    }

    fn esc_endblock(&mut self) {
        self.src_mut().take(1);
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

    fn path(&mut self, bypass: bool) -> Result<String> {
        let start = self.src().coord();

        self.src_mut().take(consts::PATH.len()).unwrap();

        let output = self.spawn_sealed_internal_parser(|p| {
            while !p.src().eof() && !p.src().pos().starts_with(consts::PATH) {
                // if an escaped quote is encountered
                if p.src().pos().starts_with(consts::esc::PATH) {
                    // take the backslash
                    p.src_mut().take(1);
                }

                p.parse_next(bypass)?;
            }

            p.unexpected_eof(|| Error::UnterminatedPath(
                start, p.src().file().to_owned(),
            ))?;

            p.src_mut().take(consts::PATH.len());

            Ok(())
        })?;

        Ok(output)
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

    fn pathlike<S>(&mut self, tag_name: S, bypass: bool) -> Result<PathBuf>
    where
        S: AsRef<str>
    {
        // is a literal path
        if self.src().pos().starts_with(consts::PATH) {
            let path_str = self.path(bypass)?;

            Ok(PathBuf::from(path_str))
        }
        // is a context variable
        else {
            let alias_str = self.alias(tag_name)?;
            Ok(self.enforce_context(|ctx| ctx.get_path(alias_str))?)
        }
    }

    fn extends(&mut self, bypass: bool) -> Result<bool> {
        if !self.src().pos().starts_with(consts::block::EXTENDS) {
            return Ok(false);
        }

        const TAG_NAME: &str = "extends";

        let start = self.src().coord();

        // make sure that the file can still extend another
        self.disallowed_after_extends()?;

        // take away the beginning of the block
        self.src_mut().take(consts::block::EXTENDS.len());

        // check for unexpected eof
        self.unexpected_eof(|| Error::UnterminatedTag(
            TAG_NAME.to_owned(), start, self.src().file().to_owned()
        ))?;

        // trim until the first characters
        self.src_mut().trim_start();

        let path = self.pathlike(TAG_NAME, bypass)?;
        let path = self.normalize_path(path);

        // trim until the closing tag
        self.src_mut().trim_start();

        // check for unexpected eof
        self.unexpected_eof(|| Error::UnterminatedTag(
            TAG_NAME.to_owned(), start, self.src().file().to_owned()
        ))?;

        if !self.src().pos().starts_with(consts::block::ENDTAG) {
            return Err(self.illegal_character(TAG_NAME));
        }

        self.src_mut().take(1);

        self.extends = Some(path);

        Ok(true)
    }

    fn source(&mut self, bypass: bool) -> Result<bool> {
        if !self.src().pos().starts_with(consts::block::SOURCE) {
            return Ok(false);
        }

        const TAG_NAME: &str = "source";

        let start = self.src().coord();

        // pass beginning tag
        self.src_mut().take(consts::block::SOURCE.len());

        // trim
        self.src_mut().trim_start();

        self.unexpected_eof(|| Error::UnterminatedTag(
            TAG_NAME.to_owned(),
            start,
            self.src().file().to_owned(),
        ))?;

        let path = self.pathlike(TAG_NAME, bypass)?;
        let path = self.normalize_path(path);

        self.src_mut().trim_start();
        self.unexpected_eof(|| Error::UnterminatedTag(
            TAG_NAME.to_owned(),
            start,
            self.src().file().to_owned(),
        ))?;

        let as_name = if self.src().pos().starts_with(consts::block::MODIFIER) {
            self.src_mut().take(1);
            self.src_mut().trim_start();
            self.unexpected_eof(|| Error::UnterminatedTag(
                TAG_NAME.to_owned(),
                start,
                self.src().file().to_owned(),
            ))?;

            if !self.src().pos().starts_with(consts::modif::AS) {
                return Err(self.illegal_character(TAG_NAME));
            }

            self.src_mut().take(consts::modif::AS.len());
            self.src_mut().trim_start();
            self.unexpected_eof(|| Error::UnterminatedTag(
                TAG_NAME.to_owned(),
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

        if !self.src().pos().starts_with(consts::block::ENDTAG) {
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

    fn include_content_mod(&mut self, start: Coordinate, bypass: bool) -> Result<Option<Vec<IncludeContentMod>>> {
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
            else if self.src().pos().starts_with(consts::modif::FILENAME) {
                self.src_mut().take(consts::modif::FILENAME.len());
                mods.push(IncludeContentMod::Filename);
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
            else if self.src().pos().starts_with(consts::modif::JSON) {
                self.src_mut().take(consts::modif::JSON.len());
                mods.push(IncludeContentMod::Json);
            }
            else if self.src().pos().starts_with(consts::modif::SPLIT) {
                self.src_mut().take(consts::modif::SPLIT.len());
                self.src_mut().trim_start();
                self.unexpected_eof(|| Error::UnterminatedTag(
                    "include-content split-modifier".to_owned(),
                    start,
                    self.src().file().to_owned(),
                ))?;

                const ZERO_THRU_NINE: [char; 10] = [
                    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
                ];

                let mut split_into = String::new();
                while !self.src().eof() &&
                    self.src().pos().starts_with(ZERO_THRU_NINE)
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
                    self.src().pos().starts_with(ZERO_THRU_NINE)
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

                let from = self.path(bypass)?;

                self.src_mut().trim_start();
                self.unexpected_eof(|| Error::UnterminatedTag(
                    "include-content".to_owned(),
                    start,
                    self.src().file().to_owned(),
                ))?;

                if !self.src().pos().starts_with(consts::PATH) {
                    return Err(self.illegal_character("include-content"));
                }

                let to = self.path(bypass)?;

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

        let mods = self.include_content_mod(start, bypass)?;

        if !self.src().pos().starts_with(consts::block::ENDTAG) {
            return Err(self.illegal_character("include-content"));
        }

        self.src_mut().take(1).unwrap();

        let is_path = if let Some(mods) = mods.as_ref() {
            mods.iter().any(|v| v.eq(&IncludeContentMod::Path))
        }
        else {
            false
        };

        let is_json = if let Some(mods) = mods.as_ref() {
            mods.iter().any(|m| m.eq(&IncludeContentMod::Json))
        }
        else {
            false
        };

        let mut value = if bypass {
            "".to_owned()
        }
        else if nullable && is_json {
            self.optional_context(|ctx| Ok(Some(ctx.get_value(alias)?.clone())))?
                .unwrap_or(JsonValue::Null)
                .to_string()
        }
        else if nullable && is_path {
            self.optional_context(|ctx| ctx.get_path_opt(alias))?
                .unwrap_or(PathBuf::new())
                .to_str()
                .unwrap_or("")
                .to_owned()
        }
        else if is_json {
            self.enforce_context(|ctx| Ok(ctx.get_value(alias)?.clone()))?
                .to_string()
        }
        else if is_path {
            self.enforce_context(|ctx| ctx.get_path(alias))?
                .to_str()
                .unwrap_or("")
                .to_owned()
        }
        else if nullable {
            self.optional_context(|ctx| ctx.get_stringlike_opt(alias))?
                .unwrap_or(String::new())
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
                    IncludeContentMod::Json => value,
                    IncludeContentMod::Filename => {
                        let p = PathBuf::from(value);
                        p.file_stem().and_then(|f| f.to_str())
                            .map(|f| f.to_owned())
                            .unwrap_or(String::new())
                    },
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
                            let end_idx = if i == into - 1 {
                                l
                            }
                            else {
                                start_idx + (l / into)
                            };

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

    fn do_trim_start<S>(&mut self, tag: S, coord: Coordinate) -> Result<()>
    where
        S: AsRef<str>
    {
        if !self.src().pos().starts_with(consts::block::TRIM) &&
            !self.src().pos().starts_with(consts::block::STARTBLOCK)
        {
            self.illegal_character(&tag);
        }

        if self.src().pos().starts_with(consts::block::TRIM) {
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

        let path = self.pathlike(TAG_NAME, bypass)?;
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

        if !self.src().pos().starts_with(consts::block::ENDTAG) {
            return Err(self.illegal_character(TAG_NAME));
        }

        self.src_mut().take(1);

        let has_block = !self.trim_or_end();

        if !has_block {
            let output = self.include_file_parse(path, is_raw, is_md, bypass)?;
            self.output.push_str(&output);
            return Ok(true);
        }

        let output = self.spawn_sealed_internal_parser(|p| {
            while !p.src().eof() && !p.src().pos().starts_with(consts::block::ENDBLOCK) {
                p.parse_next(bypass)?;
            }

            unexpected_eof(p, start)?;
            p.src_mut().take(1);

            let block_output = std::mem::take(&mut p.output);
            p.set_json_value(consts::CONTENT, block_output.into())?;

            let mut file_output = p.include_file_parse(path, is_raw, is_md, bypass)?;
            std::mem::swap(&mut file_output, &mut p.output);
            Ok(())
        })?;

        self.output.push_str(&output);

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
        else if self.src().pos().starts_with(consts::exp::EQ) {
            self.src_mut().take(consts::exp::EQ.len());
            IfCondition::Eq
        }
        else if self.src().pos().starts_with(consts::exp::NE) {
            self.src_mut().take(consts::exp::NE.len());
            IfCondition::Ne
        }
        else if self.src().pos().starts_with(consts::exp::GE) {
            self.src_mut().take(consts::exp::GE.len());
            IfCondition::Ge
        }
        else if self.src().pos().starts_with(consts::exp::GT) {
            self.src_mut().take(1);
            IfCondition::Gt
        }
        else if self.src().pos().starts_with(consts::exp::LE) {
            self.src_mut().take(consts::exp::LE.len());
            IfCondition::Le
        }
        else if self.src().pos().starts_with(consts::exp::LT) {
            self.src_mut().take(1);
            IfCondition::Lt
        }
        else {
            IfCondition::Truthy
        }
    }

    fn trim_or_end(&mut self) -> bool {
        // if eof or no chain and no startblock, then it is a valid endpoint
        if self.src().eof() ||
            (
                !self.src().pos().starts_with(consts::block::TRIM) &&
                !self.src().pos().starts_with(consts::block::STARTBLOCK)
            )
        {
            return true;
        }

        if self.src().pos().starts_with(consts::block::TRIM) {
            // take chain
            self.src_mut().take(1);
            self.src_mut().trim_start_multiline();

            if !self.src().pos().starts_with(consts::block::STARTBLOCK) {
                return true;
            }
        }

        // must be startblock
        self.src_mut().take(1);

        false
    }

    fn if_is_true<A, O>(
        &mut self, negate: bool, condition: IfCondition, alias: A,
        other_alias: Option<O>
    ) -> Result<bool>
    where
        A: Into<Alias>,
        O: Into<Alias>,
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
            IfCondition::Truthy => {
                match self.optional_context(|ctx| Ok(Some(ctx.truthy(alias)?)))? {
                    Some(truthy) => if !negate { Ok(truthy) } else { Ok(!truthy) },
                    None => if !negate { Ok(false) } else { Ok(true) },
                }
            },
            IfCondition::Eq => if !negate {
                Ok(self.enforce_context(|ctx| ctx.eq(alias, other_alias.unwrap()))?)
            }
            else {
                Ok(!self.enforce_context(|ctx| ctx.eq(alias, other_alias.unwrap()))?)
            },
            IfCondition::Ne => if !negate {
                Ok(self.enforce_context(|ctx| ctx.ne(alias, other_alias.unwrap()))?)
            }
            else {
                Ok(!self.enforce_context(|ctx| ctx.ne(alias, other_alias.unwrap()))?)
            },
            IfCondition::Gt => if !negate {
                Ok(self.enforce_context(|ctx| ctx.gt(alias, other_alias.unwrap()))?)
            }
            else {
                Ok(!self.enforce_context(|ctx| ctx.gt(alias, other_alias.unwrap()))?)
            },
            IfCondition::Ge => if !negate {
                Ok(self.enforce_context(|ctx| ctx.ge(alias, other_alias.unwrap()))?)
            }
            else {
                Ok(!self.enforce_context(|ctx| ctx.ge(alias, other_alias.unwrap()))?)
            },
            IfCondition::Lt => if !negate {
                Ok(self.enforce_context(|ctx| ctx.lt(alias, other_alias.unwrap()))?)
            }
            else {
                Ok(!self.enforce_context(|ctx| ctx.lt(alias, other_alias.unwrap()))?)
            },
            IfCondition::Le => if !negate {
                Ok(self.enforce_context(|ctx| ctx.le(alias, other_alias.unwrap()))?)
            }
            else {
                Ok(!self.enforce_context(|ctx| ctx.le(alias, other_alias.unwrap()))?)
            },
        }
    }

    fn if_tag(&mut self, bypass: bool) -> Result<bool> {
        if !self.src().pos().starts_with(consts::block::IF) {
            return Ok(false);
        }

        let start = self.src().coord();
        const TAG_NAME: &str = "if";

        fn unexpected_eof_if(p: &mut Parser, coords: Coordinate) -> Result<()> {
            p.unexpected_eof(|| Error::UnterminatedTag(
                TAG_NAME.to_owned(),
                coords,
                p.src().file().to_owned(),
            ))
        }

        // take if block
        self.src_mut().take(consts::block::IF.len());

        let mut is_true = false;
        let mut bypass_rest_of_if = false;
        loop {
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

            let alias = self.alias(TAG_NAME)?;

            self.src_mut().trim_start();
            unexpected_eof_if(self, start)?;

            let condition = self.if_condition();
            self.src_mut().trim_start();
            unexpected_eof_if(self, start)?;

            let other_alias = match condition {
                IfCondition::Eq|IfCondition::Ne|IfCondition::Gt|IfCondition::Ge|
                IfCondition::Lt|IfCondition::Le => Some(self.alias(TAG_NAME)?),
                IfCondition::Empty|IfCondition::Exists|IfCondition::Truthy => None
            };

            if !bypass_rest_of_if {
                is_true = self.if_is_true(negate, condition, alias, other_alias)?;
            }

            if self.src().pos().starts_with(consts::exp::AND) {
                self.src_mut().take(consts::exp::AND.len());
                if !bypass_rest_of_if && !is_true {
                    bypass_rest_of_if = true;
                }
            }
            else if self.src().pos().starts_with(consts::exp::OR) {
                self.src_mut().take(consts::exp::OR.len());
                if !bypass_rest_of_if && is_true {
                    bypass_rest_of_if = true;
                }
            }
            else {
                break;
            }
        }

        if !self.src().pos().starts_with(consts::block::ENDTAG) {
            return Err(self.illegal_character("if"));
        }

        self.src_mut().take(1);

        self.do_trim_start("if", start)?;

        // parse if contents
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

        fn unexpected_eof_else(p: &mut Parser, coords: Coordinate) -> Result<()> {
            p.unexpected_eof(|| Error::UnterminatedTag(
                "else".to_owned(),
                coords,
                p.src().file().to_owned(),
            ))
        }

        // if eof or no chain and no startblock, then it is a valid endpoint
        if self.trim_or_end() {
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

    fn for_file_mods(&mut self, start: Coordinate, bypass: bool) -> Result<Option<Vec<ForFileMod>>> {
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
                let path = self.path(bypass)?;

                mods.push(ForFileMod::Ext(path));
            }
            else if self.src().pos().starts_with(consts::modif::REVERSE) {
                self.src_mut().take(consts::modif::REVERSE.len());
                mods.push(ForFileMod::Reverse);
            }
            else if self.src().pos().starts_with(consts::modif::FILES) {
                self.src_mut().take(consts::modif::FILES.len());
                mods.push(ForFileMod::Files);
            }
            else if self.src().pos().starts_with(consts::modif::DIRS) {
                self.src_mut().take(consts::modif::DIRS.len());
                mods.push(ForFileMod::Dirs);
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

    fn set_json_value<A>(&mut self, alias: A, val: JsonValue) -> Result<()>
    where
        A: Into<Alias>
    {
        let mut value_dir = self.src().file().to_owned();
        value_dir.pop();

        if let Some(ctx) = self.ctx_mut() {
            ctx.set_value(alias, value_dir, val)?;
        }
        else {
            let mut new_ctx = JsonContext::faux_context(&self.path)?;
            new_ctx.set_value(alias, value_dir, val)?;
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
        self.set_json_value("$loop.index", idx.into())?;
        self.set_json_value("$loop.position", (idx + 1).into())?;
        self.set_json_value("$loop.length", len.into())?;
        self.set_json_value("$loop.max", (len - 1).into())?;
        self.set_json_value("$loop.first", (idx == 0).into())?;
        self.set_json_value("$loop.last", (idx == len - 1).into())?;
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

        let path = self.pathlike(TAG_NAME, bypass)?;
        let path = self.normalize_path(path);

        self.src_mut().trim_start();
        let mods = self.for_file_mods(start, bypass)?;
        self.src_mut().trim_start();
        unexpected_eof_for(self, start)?;

        if !self.src().pos().starts_with(consts::block::ENDTAG) {
            return Err(self.illegal_character(TAG_NAME));
        }

        // take endblock
        self.src_mut().take(1);

        // handle chain and startblock
        self.do_trim_start("for-file", start)?;

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

        let reverse = mods.as_ref()
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

        let files_only = mods.as_ref()
            .map(|m| m.iter().any(|m| matches!(m, ForFileMod::Files)))
            .unwrap_or(false);

        let dirs_only = if !files_only {
            mods
                .map(|m| m.iter().any(|m| matches!(m, ForFileMod::Dirs)))
                .unwrap_or(false)
        }
        else {
            false
        };

        let mut items = if bypass {
            vec![]
        }
        else {
            let p = path.clone();
            path.read_dir().map_err(|e| Error::IO(e, p.clone()))?
                .map(|entry_res| {
                    let entry = entry_res.map_err(|e| Error::IO(e, p.clone()))?;
                    let path = entry.path();
                    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_owned());
                    let stem = path.file_stem().and_then(|f| f.to_str())
                        .map(|f| f.to_owned());
                    let name = path.file_name().and_then(|f| f.to_str())
                        .map(|f| f.to_owned());

                    if (files_only && !path.is_file()) ||
                        (dirs_only && !path.is_dir()) ||
                        (!extensions.is_empty() && (
                            ext.is_none() ||
                            (ext.is_some() && !extensions.contains(ext.as_ref().unwrap()))
                        ))
                    {
                        return Ok(None);
                    }

                    Ok(Some(LoopFile {
                        ext,
                        stem,
                        name,
                        is_file: path.is_file(),
                        is_dir: path.is_dir(),
                        path,
                    }))
                })
                .collect::<Result<Vec<Option<LoopFile>>>>()?
                .into_iter()
                .flatten()
                .collect::<Vec<LoopFile>>()
        };

        items.sort_unstable_by(|f1, f2| f1.path.cmp(&f2.path));

        if reverse {
            items.reverse();
        }

        let len = items.len();

        let has_items = if items.is_empty() {
            items = vec![ LoopFile::default(), ];
            false
        }
        else {
            true
        };

        let alias_cl = alias.clone();
        for (idx, item) in items.into_iter().enumerate() {
            // revert back to start of loop
            self.src_mut().set_coord(for_start);

            let item_str = if let Some(item_str) = item.path.to_str() {
                item_str.to_owned()
            }
            else {
                "".to_owned()
            };

            let for_output = self.spawn_sealed_internal_parser(|p| {
                // place value into map
                p.set_json_value(alias_cl.clone(), item_str.clone().into())?;

                // setup loop context
                if !bypass && has_items {
                    p.loop_context(idx, len)?;

                    p.set_json_value("$loop.entry.path".to_owned(), item_str.clone().into())?;
                    p.set_json_value("$loop.entry.ext".to_owned(), item.ext.clone().into())?;
                    p.set_json_value("$loop.entry.stem".to_owned(), item.stem.clone().into())?;
                    p.set_json_value("$loop.entry.name".to_owned(), item.name.clone().into())?;
                    p.set_json_value("$loop.entry.is_file".to_owned(), item.is_file.into())?;
                    p.set_json_value("$loop.entry.is_dir".to_owned(), item.is_dir.into())?;
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

            if self.trim_or_end() {
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

        self.src_mut().trim_start();
        unexpected_eof_for(self, start)?;

        // take "in"
        self.in_keyword(TAG_NAME)?;
        unexpected_eof_for(self, start)?;

        let in_alias = self.alias(TAG_NAME)?;
        self.src_mut().trim_start();

        let nullable = if self.src().pos().starts_with(consts::exp::NULLABLE) {
            self.src_mut().take(1);
            true
        }
        else {
            false
        };

        self.src_mut().trim_start();
        unexpected_eof_for(self, start)?;

        let mods = self.for_item_mods(start)?;
        self.src_mut().trim_start();
        unexpected_eof_for(self, start)?;

        if !self.src().pos().starts_with(consts::block::ENDTAG) {
            return Err(self.illegal_character(TAG_NAME));
        }

        // take endblock
        self.src_mut().take(1);

        // handle chain and startblock
        self.do_trim_start(TAG_NAME, start)?;

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

            if self.trim_or_end() {
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

    fn set_json(&mut self, bypass: bool) -> Result<bool> {
        const TAG_NAME: &str = "set-json";

        fn unexpected_eof(p: &mut Parser, coord: Coordinate) -> Result<()> {
            p.unexpected_eof(|| Error::UnterminatedTag(
                TAG_NAME.to_owned(),
                coord,
                p.src().file().to_owned(),
            ))
        }

        let start = self.src().coord();

        if !self.src().pos().starts_with(consts::block::STARTBLOCK) {
            return Err(self.illegal_character(TAG_NAME));
        }

        self.src_mut().take(1);
        unexpected_eof(self, start)?;

        let output = self.spawn_sealed_internal_parser(|p| {
            while !p.src().eof() && !p.src().pos().starts_with(consts::block::ENDBLOCK) {
                p.parse_next(bypass)?;
            }

            unexpected_eof(p, start)?;
            p.src_mut().take(1);
            Ok(())
        })?;

        if !bypass {
            let s_path = self.path.clone();
            let new_ctx = JsonContext::read_from_string(&s_path, output, Some(consts::ROOT))?;

            if let Some(ctx) = self.ctx_mut() {
                ctx.merge(s_path, new_ctx)?;
            }
            else {
                self.context = Some(new_ctx);
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

        // no alias, set json as root
        if self.src().pos().starts_with(consts::block::ENDTAG) {
            self.src_mut().take(1);
            self.set_json(bypass)?;
            return Ok(true);
        }

        let alias = self.alias(TAG_NAME)?;

        self.src_mut().trim_start();
        unexpected_eof(self, start)?;

        if !self.src().pos().starts_with(consts::block::ENDTAG) {
            return Err(self.illegal_character(TAG_NAME));
        }

        self.src_mut().take(1);

        unexpected_eof(self, start)?;

        self.do_trim_start(TAG_NAME, start)?;

        let output = self.spawn_sealed_internal_parser(|p| {
            while !p.src().eof() && !p.src().pos().starts_with(consts::block::ENDBLOCK) {
                p.parse_next(bypass)?;
            }

            unexpected_eof(p, start)?;
            p.src_mut().take(1);
            Ok(())
        })?;

        if bypass {
            return Ok(true);
        }

        let s_path = self.path.clone();

        if self.ctx().is_none() {
            self.context = Some(JsonContext::faux_context(&self.path)?);
        }

        self.set_json_value(alias, JsonContext::parse_json(s_path, output)?)?;

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

        if !self.src().pos().starts_with(consts::block::ENDTAG) {
            return Err(self.illegal_character(TAG_NAME));
        }

        self.src_mut().take(1);

        if alias == consts::CONTENT {
            self.output = String::new();
            return Ok(true);
        }

        self.remove_value(alias);

        Ok(true)
    }

    fn trim_start_tag(&mut self) -> Result<bool> {
        // if doesn't start with trim character or trim character is not the
        // final character on the line
        if !self.src().pos().starts_with(consts::block::TRIM_LF) && (
            !self.src().pos().starts_with(consts::block::TRIM) ||
            self.src().pos().len() > 1
        ) {
            return Ok(false);
        }

        self.src_mut().take(1);
        self.src_mut().trim_start_multiline();

        Ok(true)
    }

    fn delete_path(&mut self, bypass: bool) -> Result<bool> {
        //-{ "some/path/here.txt" }

        if !self.src().pos().starts_with(consts::block::DELETE_PATH) {
            return Ok(false);
        }

        const TAG_NAME: &str = "delete-path";

        fn unexpected_eof(p: &mut Parser, coord: Coordinate) -> Result<()> {
            p.unexpected_eof(|| Error::UnterminatedTag(
                TAG_NAME.to_owned(),
                coord,
                p.src().file().to_owned()
            ))
        }

        let start = self.src().coord();
        self.src_mut().take(consts::block::DELETE_PATH.len());
        // "some/path/here.txt" }
        self.src_mut().trim_start();
        //"some/path/here.txt" }

        unexpected_eof(self, start)?;

        let path = self.pathlike(TAG_NAME, bypass)?;
        // }

        self.src_mut().trim_start();
        //}

        if !self.src().pos().starts_with(consts::block::ENDTAG) {
            return Err(self.illegal_character(TAG_NAME));
        }

        self.src_mut().take(1);

        let mut srcdir = self.src().file().to_owned();
        srcdir.pop();
        let path = JsonContext::normalize_path(srcdir, path);

        let is_file = path.is_file();
        if bypass || !is_file {
            return Ok(true);
        }

        std::fs::remove_file(path).map_err(|e| Error::IO(e, self.src().file().to_owned()))?;

        Ok(true)
    }

    fn copy_path(&mut self, bypass: bool) -> Result<bool> {
        //tag from            to
        //~{  "this/path.txt" "that/path.txt"  }

        if !self.src().pos().starts_with(consts::block::COPY_PATH) {
            return Ok(false);
        }

        const TAG_NAME: &str = "copy-path";

        fn unexpected_eof(p: &mut Parser, coord: Coordinate) -> Result<()> {
            p.unexpected_eof(|| Error::UnterminatedTag(
                TAG_NAME.to_owned(),
                coord,
                p.src().file().to_owned()
            ))
        }

        let start = self.src().coord();
        self.src_mut().take(consts::block::COPY_PATH.len());
        //  "this/path.txt" "that/path.txt"  }
        self.src_mut().trim_start();
        //"this/path.txt" "that/path.txt"  }

        unexpected_eof(self, start)?;

        let from = self.pathlike(TAG_NAME, bypass)?;
        // "that/path.txt"  }

        self.src_mut().trim_start();
        //"that/path.txt"  }

        let to = self.pathlike(TAG_NAME, bypass)?;
        //  }

        self.src_mut().trim_start();
        //}

        if !self.src().pos().starts_with(consts::block::ENDTAG) {
            return Err(self.illegal_character(TAG_NAME));
        }

        self.src_mut().take(1);

        let mut srcdir = self.src().file().to_owned();
        srcdir.pop();
        let from = JsonContext::normalize_path(srcdir.to_owned(), from);
        let to = JsonContext::normalize_path(srcdir, to);

        if bypass || !from.is_file() {
            return Ok(true);
        }

        let mut to_dir = to.clone();
        to_dir.pop();

        if !to_dir.is_dir() {
            std::fs::create_dir_all(to_dir)
                .map_err(|e| Error::IO(e, self.src().file().to_owned()))?;
        }

        std::fs::copy(from, to).map_err(|e| Error::IO(e, self.src().file().to_owned()))?;

        Ok(true)
    }

    fn write_content(&mut self, bypass: bool) -> Result<bool> {
        //tag to                      content
        //^{  "some/path/here.txt"  }(&{"this/file.arcana"})

        if !self.src().pos().starts_with(consts::block::WRITE_CONTENT) {
            return Ok(false);
        }

        const TAG_NAME: &str = "write-content";

        fn unexpected_eof(p: &mut Parser, coord: Coordinate) -> Result<()> {
            p.unexpected_eof(|| Error::UnterminatedTag(
                TAG_NAME.to_owned(),
                coord,
                p.src().file().to_owned()
            ))
        }

        let start = self.src().coord();
        self.src_mut().take(consts::block::WRITE_CONTENT.len());
        //  "some/path/here.txt"  }(&{"this/file.arcana"})
        self.src_mut().trim_start();
        //"some/path/here.txt"  }(&{"this/file.arcana"})

        unexpected_eof(self, start)?;

        let to = self.pathlike(TAG_NAME, bypass)?;
        //  }(&{"this/file.arcana"})

        self.src_mut().trim_start();
        //}(&{"this/file.arcana"})

        if !self.src().pos().starts_with(consts::block::ENDTAG) {
            return Err(self.illegal_character(TAG_NAME));
        }

        self.src_mut().take(1);
        //(&{"this/file.arcana"})

        self.do_trim_start(TAG_NAME, start)?;
        //(&{"this/file.arcana"})

        let content = self.spawn_sealed_internal_parser(|p| {
            let start = p.src().coord();

            while !p.src().eof() && !p.src().pos().starts_with(consts::block::ENDBLOCK) {
                p.parse_next(bypass)?;
            }

            unexpected_eof(p, start)?;
            p.src_mut().take(1);

            Ok(())
        })?;
        // ""

        if bypass {
            return Ok(true);
        }

        let mut srcdir = self.src().file().to_owned();
        srcdir.pop();

        let to = JsonContext::normalize_path(srcdir, to);

        let mut to_dir = to.clone();
        to_dir.pop();

        if !to_dir.is_dir() {
            std::fs::create_dir_all(to_dir)
                .map_err(|e| Error::IO(e, self.src().file().to_owned()))?;
        }

        std::fs::write(to, content.as_bytes()).map_err(|e| Error::IO(e, self.src().file().to_owned()))?;

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
        // escaped trim_lf: \\<newline>
        if self.src().pos().starts_with(consts::block::esc::TRIM_LF) {
            self.src_mut().take(1);
            let taken = self.src_mut().take(2).unwrap();
            self.output.push_str(&taken);
        }
        // escaped backslash: \\
        if self.src().pos().starts_with(consts::block::esc::TRIM) {
            self.src_mut().take(1);
            let taken = self.src_mut().take(1).unwrap();
            self.output.push_str(&taken);
        }
        // trim character overlaps with escapes, but MUST be the final character
        // on the line.
        else if self.trim_start_tag()? {
            // do nothing
        }
        // is escaped (2 char pattern)
        else if self.src().pos().starts_with(consts::block::esc::MODIFIER) ||
            self.src().pos().starts_with(consts::block::esc::COMMENT) ||
            self.src().pos().starts_with(consts::block::esc::EXTENDS) ||
            self.src().pos().starts_with(consts::block::esc::SOURCE) ||
            self.src().pos().starts_with(consts::block::esc::INCLUDE_FILE) ||
            self.src().pos().starts_with(consts::block::esc::INCLUDE_CONTENT) ||
            self.src().pos().starts_with(consts::block::esc::EXPRESSION) ||
            self.src().pos().starts_with(consts::block::esc::SET_ITEM) ||
            self.src().pos().starts_with(consts::block::esc::UNSET_ITEM) ||
            self.src().pos().starts_with(consts::block::esc::DELETE_PATH) ||
            self.src().pos().starts_with(consts::block::esc::COPY_PATH) ||
            self.src().pos().starts_with(consts::block::esc::WRITE_CONTENT)
        {
            self.src_mut().take(1);
            let taken = self.src_mut().take(2).unwrap();
            self.output.push_str(&taken);
        }
        // is escaped (1 char pattern)
        else if self.src().pos().starts_with(consts::block::esc::BLOCK) ||
            self.src().pos().starts_with(consts::block::esc::ENDBLOCK) ||
            self.src().pos().starts_with(consts::block::esc::TAG) ||
            self.src().pos().starts_with(consts::block::esc::ENDTAG)
        {
            self.esc_endblock();
        }
        // is a comment
        else if self.comment()? ||
            // is extending
            self.extends(bypass)? ||
            // is sourcing
            self.source(bypass)? ||
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
            self.unset_item()? ||
            // is delete-path
            self.delete_path(bypass)? ||
            // is copy-path
            self.copy_path(bypass)? ||
            // is write-content
            self.write_content(bypass)?
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
                self.set_json_value(consts::CONTENT, orig_output.into())?;
            }
            let output = self.spawn_parser(extends, |p| p.parse())?;
            self.output.push_str(&output);
        }

        Ok(())
    }
}
