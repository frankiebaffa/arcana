//! The compiler for the Arcana Templating Engine.
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
    serde::Deserialize,
    serde_json::from_str as from_json_str,
    std::{
        env::Args,
        fs::{
            canonicalize,
            copy,
            create_dir_all,
            DirEntry,
            read_to_string,
            remove_dir,
            remove_file,
            write,
        },
        path::{ Path, PathBuf, },
        process::exit as pexit,
    },
    arcana_core::{ Error, JsonContext, Parser, Result, },
};

macro_rules! vprint {
    ($verbose:expr, $msg:expr$(, $fmt:expr)*) => {
        if $verbose {
            println!($msg$(, $fmt)*);
        }
    }
}

const SCHEMA: &str = include_str!("../schema/deployment.json");

#[derive(Deserialize)]
struct CompileFile {
    source: PathBuf,
    destination: PathBuf,
}

#[derive(Deserialize)]
struct CompileDirectorySource {
    directory: PathBuf,
    extensions: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct CompileDirectoryDestination {
    directory: PathBuf,
    extension: Option<String>,
}

#[derive(Deserialize)]
struct CompileDirectory {
    source: CompileDirectorySource,
    destination: CompileDirectoryDestination,
}

#[derive(Deserialize)]
struct CompileAgainstDestination {
    directory: PathBuf,
    extension: Option<String>,
}

#[derive(Deserialize)]
struct CompileAgainstDirectory {
    path: PathBuf,
    extensions: Option<Vec<String>>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct CompileAgainstTarget {
    alias: String,
    for_each: bool,
    filename_extractor: Option<String>,
    alias_to: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct CompileAgainst {
    template: PathBuf,
    destination: CompileAgainstDestination,
    context: Option<PathBuf>,
    contexts: Option<Vec<PathBuf>>,
    context_directory: Option<CompileAgainstDirectory>,
    #[serde(default="Vec::new")]
    target: Vec<CompileAgainstTarget>,
}

#[derive(Deserialize)]
struct CopyFile {
    source: PathBuf,
    destination: PathBuf,
}

#[derive(Deserialize)]
struct CopyDirectory {
    source: PathBuf,
    destination: PathBuf,
    extensions: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct DeleteFile {
    file: Option<PathBuf>,
    files: Option<Vec<PathBuf>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Action {
    CompileFile(CompileFile),
    CompileDirectory(CompileDirectory),
    CompileAgainst(CompileAgainst),
    CopyFile(CopyFile),
    CopyDirectory(CopyDirectory),
    DeleteFile(DeleteFile),
}

#[derive(Deserialize)]
struct Deployment {
    actions: Vec<Action>,
}

const HELP: &str = include_str!("../resources/help.txt");

// compile-against
const HELP_COMPILE_AGAINST: &str = include_str!(
    "../resources/compile-against.txt"
);
const HELP_COMPILE_AGAINST_DESTINATION: &str = include_str!(
    "../resources/compile-against-destination.txt"
);
const HELP_COMPILE_AGAINST_CONTEXT_DIRECTORY: &str = include_str!(
    "../resources/compile-against-context-directory.txt"
);
const HELP_COMPILE_AGAINST_TARGET: &str = include_str!(
    "../resources/compile-against-target.txt"
);

// compile-directory
const HELP_COMPILE_DIRECTORY: &str = include_str!(
    "../resources/compile-directory.txt"
);
const HELP_COMPILE_DIRECTORY_DESTINATION: &str = include_str!(
    "../resources/compile-directory-destination.txt"
);
const HELP_COMPILE_DIRECTORY_SOURCE: &str = include_str!(
    "../resources/compile-directory-source.txt"
);

// compile-file
const HELP_COMPILE_FILE: &str = include_str!(
    "../resources/compile-file.txt"
);

// copy-directory
const HELP_COPY_DIRECTORY: &str = include_str!(
    "../resources/copy-directory.txt"
);

// copy-file
const HELP_COPY_FILE: &str = include_str!(
    "../resources/copy-file.txt"
);

// delete-file
const HELP_DELETE_FILE: &str = include_str!(
    "../resources/delete-file.txt"
);

const NOTICE: &str = include_str!("../NOTICE.txt");
const LICENSE: &str = include_str!("../../LICENSE.md");

fn get_files_from_dir<E, D>(verbose: bool, depl: E, dir: D, exts: &Option<Vec<String>>, rcrsv: bool) -> Result<Vec<PathBuf>>
where
    E: AsRef<Path>,
    D: AsRef<Path>
{
    let depl: PathBuf = depl.as_ref().into();
    let dir: PathBuf = dir.as_ref().into();

    if !dir.exists() {
        return Ok(vec![]);
    }

    if verbose {
        if let Some(exts) = exts {
            let exts = exts.join(", ");
            vprint!(verbose, "Retrieving files from {dir:?} with extensions \"{exts}\"");
        }
        else {
            vprint!(verbose, "Retrieving files from {dir:?}");
        }
    }

    Ok(dir.read_dir().map_err(|e| Error::IO(e, dir.clone()))?
        .map(|v| v.map_err(|e| Error::IO(e, dir.clone())))
        .collect::<Result<Vec<DirEntry>>>()?
        .into_iter()
        // keep only files if not recursive
        .filter(|f| rcrsv || f.path().is_file())
        .map(|f| if f.path().is_dir() {
            get_files_from_dir(verbose, &depl, f.path(), exts, rcrsv)
        } else {
            Ok(vec![ f.path(), ])
        })
        .collect::<Result<Vec<Vec<PathBuf>>>>()?
        .into_iter()
        .flatten()
        // keep files where extension is in requested extensions
        .filter(|f| exts.is_none() || exts.as_ref().unwrap().iter()
            .any(|e| f.extension().map(|e| e.to_str().unwrap_or("")).unwrap_or("").to_owned().eq(e))
        )
        .collect::<Vec<PathBuf>>()
    )
}

fn copy_dir_all<E, S, D>(verbose: bool, depl: E, exts: &Option<Vec<String>>, src: S, dst: D) -> Result<()>
where
    E: AsRef<Path>,
    S: AsRef<Path>,
    D: AsRef<Path>,
{
    let depl: PathBuf = depl.as_ref().into();
    let src: PathBuf = src.as_ref().into();
    let dst: PathBuf = dst.as_ref().into();

    let files = get_files_from_dir(verbose, depl, &src, exts, true)?;

    for file in files {
        let mut src_wo_dir = src.clone();
        src_wo_dir.pop();

        let dst_f_end = file.strip_prefix(src_wo_dir).expect("Not a prefix");
        let dst_file = dst.join(dst_f_end);

        let mut dst_dir = dst_file.clone();
        dst_dir.pop();

        if !dst_dir.is_dir() {
            vprint!(verbose, "Creating dir(s) {dst_dir:?}");
            create_dir_all(&dst_dir).map_err(|e| Error::IO(e, dst_dir.clone()))?;
        }

        vprint!(verbose, "Copying file {file:?} to {dst_file:?}");
        copy(&file, dst_file).map_err(|e| Error::IO(e, file.clone()))?;
    }

    Ok(())
}

fn output(p: Parser) -> String {
    let mut output = p.as_output();

    if !output.ends_with('\n') {
        output.push('\n');
    }

    output
}

fn get_fex_and_contexts_from_target_iter<'a>(
    mut ctxs: Vec<JsonContext>, target_iter: &mut std::slice::Iter<'a, CompileAgainstTarget>
) -> Result<(Option<String>, Vec<JsonContext>)> {
    let mut fex = None;

    while let Some(t) = target_iter.next() {
        fex = t.filename_extractor.clone();

        if t.for_each {
            ctxs = ctxs.into_iter()
                .map(|ctx| ctx.get_each_as_context(&t.alias, t.alias_to.clone()))
                .collect::<Result<Vec<Vec<JsonContext>>>>()?
                .into_iter()
                .flatten()
                .collect::<Vec<JsonContext>>();
        }
        else {
            ctxs = ctxs.into_iter()
                .map(|ctx| ctx.get_as_context(&t.alias, t.alias_to.clone()))
                .collect::<Result<Vec<JsonContext>>>()?;
        }
    }

    Ok((fex, ctxs))
}

fn get_fex_and_contexts_from_target<P>(
    ctx_path: P, target: &Vec<CompileAgainstTarget>
) -> Result<(Option<String>, Vec<JsonContext>)>
where
    P: AsRef<Path>
{
    let ctx: PathBuf = ctx_path.as_ref().into();

    let context_path = canonicalize(&ctx)
        .map_err(|e| Error::IO(e, ctx.clone()))?;
    let context = JsonContext::read(context_path)?;

    let mut target_iter = target.iter();

    Ok(get_fex_and_contexts_from_target_iter(vec![ context ], &mut target_iter)?)
}

fn compile_against<C, T, D>(
    verbose: bool, ctx: C, tmpl: T, dst: D, dst_ext: &Option<String>,
    target: &Vec<CompileAgainstTarget>
) -> Result<()>
where
    C: AsRef<Path>,
    T: AsRef<Path>,
    D: AsRef<Path>
{
    let ctx: PathBuf = ctx.as_ref().into();
    let tmpl: PathBuf = tmpl.as_ref().into();
    let dst: PathBuf = dst.as_ref().into();

    let mut filename = ctx.file_stem()
        .map(|v| v.to_str().unwrap_or(""))
        .or(Some(""))
        .map(|v| v.to_owned())
        .unwrap();

    let (fex, contexts) = get_fex_and_contexts_from_target(&ctx, target)?;

    for context in contexts {
        let mut p = Parser::new_with_context(&tmpl, context.clone())?;
        p.parse()?;

        let mut dest = dst.clone();

        if let Some(fex) = &fex {
            let mut fex_p = Parser::from_string_and_path_with_context(
                "./tmp.json", fex.to_owned(), context
            )?;
            fex_p.parse()?;
            filename = fex_p.as_output();
        }

        if let Some(ext) = dst_ext {
            dest.push(format!("{filename}.{ext}"));
        }
        else {
            dest.push(&filename);
        }

        vprint!(verbose, "Compiling {tmpl:?} against context {ctx:?} to {dest:?}");

        let mut dir = dest.clone();
        dir.pop();
        create_dir_all(&dir).map_err(|e| Error::IO(e, dir.clone()))?;

        write(&dest, output(p)).map_err(|e| Error::IO(e, dest.clone()))?;
    }

    Ok(())
}

fn compile_file<S, D>(verbose: bool, src: S, dst: D) -> Result<()>
where
    S: AsRef<Path>,
    D: AsRef<Path>,
{
    let src: PathBuf = src.as_ref().into();
    let dst: PathBuf = dst.as_ref().into();

    vprint!(verbose, "Compiling single file {:?}", src);

    let mut parser = Parser::new(&src)?;
    parser.parse()?;

    let mut dir = dst.clone();
    dir.pop();
    create_dir_all(&dir).map_err(|e| Error::IO(e, dir.clone()))?;

    write(&dst, output(parser)).map_err(|e| Error::IO(e, dst.clone()))?;

    Ok(())
}

fn as_output_path<S, D>(src_file: S, dst_dir: D, dst_ext: &Option<String>) -> Result<PathBuf>
where
    S: AsRef<Path>,
    D: AsRef<Path>,
{
    let src_file: PathBuf = src_file.as_ref().into();
    let mut dst_dir: PathBuf = dst_dir.as_ref().into();

    let stem = src_file.file_stem()
        .map(|v| v.to_str().unwrap_or(""))
        .or(Some(""))
        .map(|v| v.to_owned())
        .unwrap();

    let ext = src_file.extension()
        .map(|v| v.to_str().unwrap_or(""))
        .or(Some(""))
        .map(|v| v.to_owned())
        .unwrap();

    if let Some(ext) = &dst_ext {
        dst_dir.push(format!("{stem}.{ext}"));
    }
    else {
        dst_dir.push(format!("{stem}.{ext}"));
    }

    Ok(dst_dir)
}

fn dir_is_empty<D>(dir: D) -> Result<bool>
where
    D: AsRef<Path>,
{
    let dir: PathBuf = dir.as_ref().into();

    Ok(!dir.read_dir().map_err(|e| Error::IO(e, dir.clone()))?.any(|_| true))
}

fn cnd_remove_dir_all<E, D>(verbose: bool, depl: E, dir: D) -> Result<()>
where
    E: AsRef<Path>,
    D: AsRef<Path>,
{
    let depl: PathBuf = depl.as_ref().into();
    let dir: PathBuf = dir.as_ref().into();

    if !dir.exists() {
        return Ok(());
    }

    let dirs = dir.read_dir().map_err(|e| Error::IO(e, dir.clone()))?
        .map(|r| r.map_err(|e| Error::IO(e, dir.clone())))
        .collect::<Result<Vec<DirEntry>>>()?
        .into_iter()
        .filter(|de| de.path().is_dir())
        .map(|de| de.path())
        .collect::<Vec<PathBuf>>();

    for d in dirs {
        cnd_remove_dir_all(verbose, &depl, d)?;
    }

    if dir_is_empty(&dir)? {
        vprint!(verbose, "Removing empty directory {dir:?}");
        remove_dir(&dir).map_err(|e| Error::IO(e, dir.clone()))?;
    }

    Ok(())
}

fn cnd_remove_file<E, P>(verbose: bool, depl: E, path: P) -> Result<()>
where
    E: AsRef<Path>,
    P: AsRef<Path>,
{
    let depl: PathBuf = depl.as_ref().into();
    let path: PathBuf = path.as_ref().into();

    if path.is_file() {
        vprint!(verbose, "Deleting file {path:?}");
        remove_file(&path).map_err(|e| Error::IO(e, path.clone()))?;
    }

    let mut dir = path.clone();
    dir.pop();

    cnd_remove_dir_all(verbose, depl, dir)?;

    Ok(())
}

fn do_clean<E>(verbose: bool, depl: E, deployment: Deployment) -> Result<()>
where
    E: AsRef<Path>,
{
    for action in deployment.actions {
        match action {
            Action::CompileFile(cf) => {
                cnd_remove_file(verbose, &depl, cf.destination)?;
            },
            Action::CompileDirectory(cd) => {
                let files = get_files_from_dir(
                    verbose,
                    &depl,
                    &cd.source.directory,
                    &cd.source.extensions,
                    false
                )?;

                for file in files {
                    let dst = as_output_path(
                        &file,
                        &cd.destination.directory,
                        &cd.destination.extension
                    )?;

                    cnd_remove_file(verbose, &depl, &dst)?;
                }
            },
            // TODO: Handle clean when 'target' is included.
            Action::CompileAgainst(ca) => if let Some(ctx) = ca.context {
                let dst = as_output_path(
                    ctx,
                    ca.destination.directory,
                    &ca.destination.extension,
                )?;

                cnd_remove_file(verbose, &depl, dst)?;
            }
            else if let Some(ctxs) = ca.contexts {
                for ctx in ctxs {
                    let dst = as_output_path(
                        ctx,
                        &ca.destination.directory,
                        &ca.destination.extension
                    )?;

                    cnd_remove_file(verbose, &depl, dst)?;
                }
            }
            else if let Some(ctx_dir) = ca.context_directory {
                let files = get_files_from_dir(
                    verbose,
                    &depl,
                    ctx_dir.path,
                    &ctx_dir.extensions,
                    false
                )?;

                for file in files {
                    let dst = as_output_path(
                        file,
                        &ca.destination.directory,
                        &ca.destination.extension,
                    )?;

                    cnd_remove_file(verbose, &depl, dst)?;
                }
            },
            Action::CopyFile(cf) => {
                cnd_remove_file(verbose, &depl, cf.destination)?;
            },
            Action::CopyDirectory(cd) => {
                let files = get_files_from_dir(
                    verbose,
                    &depl,
                    &cd.source,
                    &cd.extensions,
                    true
                )?;

                for file in files {
                    let mut src_wo_dir = cd.source.clone();
                    src_wo_dir.pop();

                    let dst_f_end = file.strip_prefix(src_wo_dir).expect("Not a prefix");
                    let dst_file = cd.destination.join(dst_f_end);

                    cnd_remove_file(verbose, &depl, &dst_file)?;
                }
            },
            // just make sure they're deleted
            Action::DeleteFile(df) => if let Some(file) = df.file {
                cnd_remove_file(verbose, &depl, file)?;
            }
            else if let Some(files) = df.files {
                for file in files {
                    cnd_remove_file(verbose, &depl, file)?;
                }
            },
        }
    }

    Ok(())
}

#[derive(Default)]
struct Options {
    deployment: Option<PathBuf>,
    verbose: bool,
    clean: bool,
    actions: Vec<Action>,
}

impl Options {
    fn clean(&mut self) {
        if self.clean {
            eprintln!("Clean was specified multiple times");
            pexit(1);
        }

        self.clean = true;
    }

    fn compile_against_template(&mut self, args: &mut Args, outer: [String; 2], tmp: &mut Option<PathBuf>) {
        let [ a, b, ] = outer;

        if tmp.is_some() {
            self.mtonce_sub(b, Some(vec![ a, ]));
        }
        else {
            std::mem::swap(tmp, &mut args.next().map(String::into));
        }
    }

    fn compile_against_destination_directory(&mut self, args: &mut Args, outer: [String; 3], dir: &mut Option<PathBuf>) {
        let [ a, b, c, ] = outer;

        if dir.is_some() {
            self.mtonce_sub(c, Some(vec![ a, b, ]));
        }
        else {
            std::mem::swap(dir, &mut args.next().map(String::into));
        }
    }

    fn compile_against_destination_extension(&mut self, args: &mut Args, outer: [String; 3], ext: &mut Option<String>) {
        let [ a, b, c, ] = outer;

        if ext.is_some() {
            self.mtonce_sub(c, Some(vec![ a, b, ]));
        }
        else {
            std::mem::swap(ext, &mut args.next());
        }
    }

    fn compile_against_destination(
        &mut self, args: &mut Args, outer: [String; 2], dst: &mut Option<CompileAgainstDestination>
    ) {
        let [ a, b, ] = outer;

        if dst.is_some() {
            self.mtonce_sub(b, Some(vec![ a, ]));
        }
        else {
            let mut dir = None;
            let mut ext = None;

            while let Some(arg) = args.next() {
                if arg.starts_with("--") {
                    match arg.as_str() {
                        "--help" => {
                            println!("{HELP_COMPILE_AGAINST_DESTINATION}");
                            pexit(0);
                        },
                        "--directory" => self.compile_against_destination_directory(
                            args, [a.clone(), b.clone(), arg], &mut dir
                        ),
                        "--extension" => self.compile_against_destination_extension(
                            args, [a.clone(), b.clone(), arg], &mut ext
                        ),
                        "--" => break,
                        _ => self.unknown_sub(arg, Some(vec![ a, b, ])),
                    }
                }
                else if arg.starts_with('-') {
                    let mut chars = arg.chars();
                    chars.next();

                    for c in chars {
                        let arg = format!("-{c}");
                        match c {
                            'd' => self.compile_against_destination_directory(args, [a.clone(), b.clone(), arg], &mut dir),
                            'e' => self.compile_against_destination_extension(args, [a.clone(), b.clone(), arg], &mut ext),
                            'h'=> {
                                println!("{HELP_COMPILE_AGAINST_DESTINATION}");
                                pexit(0);
                            },
                            _ => self.unknown_sub(arg, Some(vec![ a, b, ])),
                        }
                    }
                }
                else {
                    self.unknown_sub(arg, Some(vec![ a, b, ]));
                }
            }

            if dir.is_none() {
                self.missing_sub("--directory", b, Some(vec![ a, ]));
            }

            std::mem::swap(dst, &mut Some(CompileAgainstDestination {
                directory: dir.unwrap(),
                extension: ext,
            }));
        }
    }

    fn compile_against_target_alias(
        &mut self, args: &mut Args, outer: [String; 3], alias: &mut Option<String>
    ) {
        let [ a, b, c, ] = outer;

        if alias.is_some() {
            self.mtonce_sub(c, Some(vec![ a, b, ]));
        }

        if let Some(a) = args.next() {
            std::mem::swap(alias, &mut Some(a));
        }
        else {
            self.missing_sub("<als>", c, Some(vec![ a, b, ]));
        }
    }

    fn compile_against_target_for_each(
        &mut self, outer: [String; 3], for_each: &mut bool
    ) {
        let [ a, b, c, ] = outer;

        if *for_each {
            self.mtonce_sub(c, Some(vec![ a, b, ]));
        }

        std::mem::swap(for_each, &mut true);
    }

    fn compile_against_target_file_extractor(
        &mut self, args: &mut Args, outer: [String; 3], fex: &mut Option<String>,
    ) {
        let [ a, b, c, ] = outer;

        if fex.is_some() {
            self.mtonce_sub(c, Some(vec![ a, b, ]));
        }

        if let Some(f) = args.next() {
            std::mem::swap(fex, &mut Some(f));
        }
        else {
            self.missing_sub("<arc>", c, Some(vec![ a, b, ]));
        }
    }

    fn compile_against_target_alias_to(
        &mut self, args: &mut Args, outer: [String; 3], ato: &mut Option<String>,
    ) {
        let [ a, b, c, ] = outer;

        if ato.is_some() {
            self.mtonce_sub(c, Some(vec![ a, b, ]));
        }

        if let Some(f) = args.next() {
            std::mem::swap(ato, &mut Some(f));
        }
        else {
            self.missing_sub("<als>", c, Some(vec![ a, b, ]));
        }
    }

    fn compile_against_target(
        &mut self, args: &mut Args, outer: [String; 2], target: &mut Vec<CompileAgainstTarget>
    ) {
        let [ a, b, ] = outer;

        let mut alias = None;
        let mut for_each = false;
        let mut fex = None;
        let mut ato = None;

        while let Some(arg) = args.next() {
            if arg.starts_with("--") {
                match arg.as_str() {
                    "--help" => {
                        println!("{HELP_COMPILE_AGAINST_TARGET}");
                        pexit(0);
                    },
                    "--alias" => self.compile_against_target_alias(
                        args, [a.clone(), b.clone(), arg], &mut alias
                    ),
                    "--for-each" => self.compile_against_target_for_each(
                        [a.clone(), b.clone(), arg], &mut for_each
                    ),
                    "--filename-extractor" => self.compile_against_target_file_extractor(
                        args, [a.clone(), b.clone(), arg], &mut fex
                    ),
                    "--alias-to" => self.compile_against_target_alias_to(
                        args, [a.clone(), b.clone(), arg], &mut ato
                    ),
                    "--" => break,
                    _ => self.unknown_sub(arg, Some(vec![ a, b, ])),
                }
            }
            else if arg.starts_with('-') {
                let mut chars = arg.chars();
                chars.next();

                for c in chars {
                    let arg = format!("-{c}");
                    match c {
                        'a' => self.compile_against_target_alias(args, [a.clone(), b.clone(), arg], &mut alias),
                        'f' => self.compile_against_target_for_each([a.clone(), b.clone(), arg], &mut for_each),
                        'h'=> {
                            println!("{HELP_COMPILE_AGAINST_TARGET}");
                            pexit(0);
                        },
                        't' => self.compile_against_target_alias_to(
                            args, [a.clone(), b.clone(), arg], &mut ato
                        ),
                        'x' => self.compile_against_target_file_extractor(args, [a.clone(), b.clone(), arg], &mut fex),
                        _ => self.unknown_sub(arg, Some(vec![ a, b, ])),
                    }
                }
            }
            else {
                self.unknown_sub(arg, Some(vec![ a, b, ]));
            }
        }

        if alias.is_none() {
            self.missing_sub("--alias", b, Some(vec![ a, ]));
        }

        target.push(CompileAgainstTarget {
            alias: alias.unwrap(),
            for_each,
            filename_extractor: fex,
            alias_to: ato,
        });
    }

    fn compile_against_context(&mut self, args: &mut Args, outer: [String; 2], ctxs: &mut Vec<PathBuf>) {
        let [ a, b, ] = outer;

        if let Some(ctx) = args.next() {
            ctxs.push(ctx.into());
        }
        else {
            self.missing_sub("<ctx>", b, Some(vec![ a, ]));
        }
    }

    fn compile_against_context_directory_path(&mut self, args: &mut Args, outer: [String; 3], path: &mut Option<PathBuf>) {
        let [ a, b, c, ] = outer;

        if path.is_some() {
            self.mtonce_sub(c, Some(vec![ a, b, ]));
        }
        else {
            std::mem::swap(path, &mut args.next().map(String::into));
        }
    }

    fn compile_against_context_directory_extensions(&mut self, args: &mut Args, outer: [String; 3], exts: &mut Vec<String>) {
        let [ a, b, c, ] = outer;

        if let Some(arg) = args.next() {
            exts.push(arg);
        }
        else {
            self.missing_sub("<ext>", c, Some(vec![ a, b, ]));
        }
    }

    fn compile_against_context_directory(
        &mut self, args: &mut Args, outer: [String; 2],
        ctx_dir: &mut Option<CompileAgainstDirectory>
    ) {
        let [ a, b, ] = outer;

        if ctx_dir.is_some() {
            self.mtonce_sub(b, Some(vec![ a, ]));
        }
        else {
            let mut path = None;
            let mut exts = Vec::new();

            while let Some(arg) = args.next() {
                if arg.starts_with("--") {
                    match arg.as_str() {
                        "--help" => {
                            println!("{HELP_COMPILE_AGAINST_CONTEXT_DIRECTORY}");
                            pexit(0);
                        },
                        "--path" => self.compile_against_context_directory_path(
                            args, [a.clone(), b.clone(), arg], &mut path
                        ),
                        "--extension" => self.compile_against_context_directory_extensions(
                            args, [a.clone(), b.clone(), arg], &mut exts
                        ),
                        "--" => break,
                        _ => self.unknown_sub(arg, Some(vec![ a, b, ])),
                    }
                }
                else if arg.starts_with('-') {
                    let mut chars = arg.chars();
                    chars.next();

                    for c in chars {
                        let arg = format!("-{c}");
                        match c {
                            'h' => {
                                println!("{HELP_COMPILE_AGAINST_CONTEXT_DIRECTORY}");
                                pexit(0);
                            },
                            'p' => self.compile_against_context_directory_path(
                                args, [a.clone(), b.clone(), arg], &mut path
                            ),
                            'e' => self.compile_against_context_directory_extensions(
                                args, [a.clone(), b.clone(), arg], &mut exts
                            ),
                            _ => self.unknown_sub(arg, Some(vec![ a, b, ])),
                        }
                    }
                }
                else {
                    self.unknown_sub(arg, Some(vec![ a, b, ]));
                }
            }

            std::mem::swap(ctx_dir, &mut Some(CompileAgainstDirectory {
                path: path.unwrap(),
                extensions: if exts.is_empty() { None } else { Some(exts) },
            }));
        }
    }

    fn compile_against(&mut self, args: &mut Args, outer: String) {
        let mut tmp = None;
        let mut dst = None;
        let mut ctxs = Vec::new();
        let mut ctx_dir = None;
        let mut target = Vec::new();

        while let Some(arg) = args.next() {
            if arg.starts_with("--") {
                match arg.as_str() {
                    "--help" => {
                        println!("{HELP_COMPILE_AGAINST}");
                        pexit(0);
                    },
                    "--template" => self.compile_against_template(args, [outer.clone(), arg], &mut tmp),
                    "--destination" => self.compile_against_destination(args, [outer.clone(), arg], &mut dst),
                    "--context" => self.compile_against_context(args, [outer.clone(), arg], &mut ctxs),
                    "--context-directory" => self.compile_against_context_directory(args, [outer.clone(), arg], &mut ctx_dir),
                    "--target" => self.compile_against_target(args, [outer.clone(), arg], &mut target),
                    "--" => break,
                    _ => self.unknown_sub(arg, Some(vec![ outer, ])),
                }
            }
            else if arg.starts_with('-') {
                let mut chars = arg.chars();
                chars.next();

                for c in chars {
                    let arg = format!("-{c}");
                    match c {
                        'c' => self.compile_against_context(args, [outer.clone(), arg], &mut ctxs),
                        'd' => self.compile_against_destination(args, [outer.clone(), arg], &mut dst),
                        'h' => {
                            println!("{HELP_COMPILE_AGAINST}");
                            pexit(0);
                        },
                        'r' => self.compile_against_target(args, [outer.clone(), arg], &mut target),
                        't' => self.compile_against_template(args, [outer.clone(), arg], &mut tmp),
                        'x' => self.compile_against_context_directory(args, [outer.clone(), arg], &mut ctx_dir),
                        _ => self.unknown_sub(arg, Some(vec![ outer, ])),
                    }
                }
            }
            else {
                self.unknown_sub(arg, Some(vec![ outer, ]));
            }
        }

        if tmp.is_none() {
            self.missing("--template", outer.as_str());
        }
        else if dst.is_none() {
            self.missing("--destination", outer.as_str());
        }
        else if ctxs.is_empty() && ctx_dir.is_none() {
            self.missing("--context|--context-directory", outer.as_str());
        }

        match ctxs.len() {
            0 => {
                self.actions.push(Action::CompileAgainst(CompileAgainst {
                    context: None,
                    contexts: None,
                    context_directory: ctx_dir,
                    template: tmp.unwrap(),
                    destination: dst.unwrap(),
                    target,
                }));
            },
            1 => {
                self.actions.push(Action::CompileAgainst(CompileAgainst {
                    context: ctxs.into_iter().next(),
                    contexts: None,
                    context_directory: None,
                    template: tmp.unwrap(),
                    destination: dst.unwrap(),
                    target,
                }));
            },
            _ => {
                self.actions.push(Action::CompileAgainst(CompileAgainst {
                    context: None,
                    contexts: Some(ctxs),
                    context_directory: None,
                    template: tmp.unwrap(),
                    destination: dst.unwrap(),
                    target,
                }));
            }
        }
    }

    fn compile_directory_source_directory(&mut self, args: &mut Args, outer: [String; 3], dir: &mut Option<PathBuf>) {
        let [ a, b, c, ] = outer;

        if dir.is_some() {
            self.mtonce_sub(c, Some(vec![ a, b, ]));
        }

        std::mem::swap(dir, &mut args.next().map(String::into));
    }

    fn compile_directory_source_extension(&mut self, args: &mut Args, outer: [String; 3], exts: &mut Vec<String>) {
        let [ a, b, c, ] = outer;

        if let Some(ext) = args.next() {
            exts.push(ext);
        }
        else {
            self.missing_sub("<ext>", c, Some(vec![ a, b, ]));
        }
    }

    fn compile_directory_source(&mut self, args: &mut Args, outer: [String; 2], src: &mut Option<CompileDirectorySource>) {
        let [ a, b, ] = outer;

        if src.is_some() {
            self.mtonce_sub(b, Some(vec![ a, ]));
        }
        else {
            let mut dir = None;
            let mut exts = Vec::new();

            while let Some(arg) = args.next() {
                if arg.starts_with("--") {
                    match arg.as_str() {
                        "--directory" => self.compile_directory_source_directory(
                            args, [a.clone(), b.clone(), arg], &mut dir
                        ),
                        "--extension" => self.compile_directory_source_extension(
                            args, [a.clone(), b.clone(), arg], &mut exts
                        ),
                        "--help" => {
                            println!("{HELP_COMPILE_DIRECTORY_SOURCE}");
                            pexit(0);
                        },
                        "--" => break,
                        _ => self.unknown_sub(arg, Some(vec![ a, b, ])),
                    }
                }
                else if arg.starts_with('-') {
                    let mut chars = arg.chars();
                    chars.next();

                    for c in chars {
                        let arg = format!("-{c}");
                        match c {
                            'd' => self.compile_directory_source_directory(
                                args, [a.clone(), b.clone(), arg], &mut dir
                            ),
                            'e' => self.compile_directory_source_extension(
                                args, [a.clone(), b.clone(), arg], &mut exts
                            ),
                            'h' => {
                                println!("{HELP_COMPILE_DIRECTORY_SOURCE}");
                                pexit(0);
                            },
                            _ => self.unknown_sub(arg, Some(vec![ a, b, ])),
                        }
                    }
                }
                else {
                    self.unknown(arg);
                }
            }

            if dir.is_none() {
                self.missing_sub("--directory <dir>", b, Some(vec![ a, ]));
            }

            let exts = if exts.is_empty() {
                None
            }
            else {
                Some(exts)
            };

            std::mem::swap(src, &mut Some(CompileDirectorySource {
                directory: dir.unwrap(),
                extensions: exts,
            }));
        }
    }

    fn compile_directory_destination_directory(&mut self, args: &mut Args, outer: [String; 3], dir: &mut Option<PathBuf>) {
        let [ a, b, c, ] = outer;

        if dir.is_some() {
            self.mtonce_sub(c, Some(vec![ a, b, ]));
        }

        std::mem::swap(dir, &mut args.next().map(String::into));
    }

    fn compile_directory_destination_extension(&mut self, args: &mut Args, outer: [String; 3], ext: &mut Option<String>) {
        let [ a, b, c, ] = outer;

        if ext.is_some() {
            self.mtonce_sub(c, Some(vec![ a, b, ]));
        }

        std::mem::swap(ext, &mut args.next());
    }

    fn compile_directory_destination(
        &mut self, args: &mut Args, outer: [String; 2], dst: &mut Option<CompileDirectoryDestination>
    ) {
        let [ a, b, ] = outer;

        if dst.is_some() {
            self.mtonce_sub(b, Some(vec![ a, ]));
        }
        else {
            let mut dir = None;
            let mut ext = None;

            while let Some(arg) = args.next() {
                if arg.starts_with("--") {
                    match arg.as_str() {
                        "--directory" => self.compile_directory_destination_directory(
                            args, [a.clone(), b.clone(), arg], &mut dir
                        ),
                        "--extension" => self.compile_directory_destination_extension(
                            args, [a.clone(), b.clone(), arg], &mut ext
                        ),
                        "--help" => {
                            println!("{HELP_COMPILE_DIRECTORY_DESTINATION}");
                            pexit(0);
                        },
                        "--" => break,
                        _ => self.unknown_sub(arg, Some(vec![ a, b, ])),
                    }
                }
                else if arg.starts_with('-') {
                    let mut chars = arg.chars();
                    chars.next();

                    for c in chars {
                        let arg = format!("-{c}");

                        match c {
                            'd' => self.compile_directory_destination_directory(
                                args, [a.clone(), b.clone(), arg], &mut dir
                            ),
                            'e' => self.compile_directory_destination_extension(
                                args, [a.clone(), b.clone(), arg], &mut ext
                            ),
                            'h' => {
                                println!("{HELP_COMPILE_DIRECTORY_DESTINATION}");
                                pexit(0);
                            },
                            _ => self.unknown_sub(arg, Some(vec![ a, b, ])),
                        }
                    }
                }
                else {
                    self.unknown_sub(arg, Some(vec![ a, b, ]));
                }
            }

            if dir.is_none() {
                self.missing_sub("--directory <dir>", b, Some(vec![ a, ]));
            }

            std::mem::swap(dst, &mut Some(CompileDirectoryDestination {
                directory: dir.unwrap(),
                extension: ext,
            }));
        }
    }

    fn compile_directory(&mut self, args: &mut Args, outer: String) {
        let mut src = None;
        let mut dst = None;

        while let Some(arg) = args.next() {
            if arg.starts_with("--") {
                match arg.as_str() {
                    "--help" => {
                        println!("{HELP_COMPILE_DIRECTORY}");
                        pexit(0);
                    },
                    "--source" => self.compile_directory_source(
                        args, [outer.clone(), arg], &mut src
                    ),
                    "--destination" => self.compile_directory_destination(
                        args, [outer.clone(), arg], &mut dst
                    ),
                    "--" => break,
                    _ => self.unknown(arg),
                }
            }
            else if arg.starts_with('-') {
                let mut chars = arg.chars();
                chars.next();

                for c in chars {
                    let arg = format!("-{c}");
                    match c {
                        'h' => {
                            println!("{HELP_COMPILE_DIRECTORY}");
                            pexit(0);
                        },
                        's' => self.compile_directory_source(
                            args, [outer.clone(), arg], &mut src
                        ),
                        'd' => self.compile_directory_destination(
                            args, [outer.clone(), arg], &mut dst
                        ),
                        _ => self.unknown(arg),
                    }
                }
            }
            else {
                self.unknown_sub(arg, Some(vec![ outer, ]));
            }
        }

        if src.is_none() {
            self.missing("--source <src>", outer);
        }
        else if dst.is_none() {
            self.missing("--destination <dst>", outer);
        }

        self.actions.push(Action::CompileDirectory(CompileDirectory {
            source: src.unwrap(),
            destination: dst.unwrap(),
        }));
    }

    fn compile_file_source(&mut self, args: &mut Args, outer: [String; 2], src: &mut Option<PathBuf>) {
        let [ a, b, ] = outer;

        if src.is_some() {
            self.mtonce_sub(b, Some(vec![ a, ]));
        }

        std::mem::swap(src, &mut args.next().map(|v| v.into()));
    }

    fn compile_file_destination(&mut self, args: &mut Args, outer: [String; 2], dst: &mut Option<PathBuf>) {
        let [ a, b, ] = outer;

        if dst.is_some() {
            self.mtonce_sub(b, Some(vec![ a, ]));
        }

        std::mem::swap(dst, &mut args.next().map(|v| v.into()));
    }

    fn compile_file(&mut self, args: &mut Args, outer: String) {
        let mut src = None;
        let mut dst = None;

        while let Some(arg) = args.next() {
            if arg.starts_with("--") {
                match arg.as_str() {
                    "--destination" => self.compile_file_destination(args, [outer.clone(), arg], &mut dst),
                    "--help" => {
                        println!("{HELP_COMPILE_FILE}");
                        pexit(0);
                    },
                    "--source" => self.compile_file_source(args, [outer.clone(), arg], &mut src),
                    "--" => break,
                    _ => self.unknown_sub(arg, Some(vec![ outer])),
                }
            }
            else if arg.starts_with('-') {
                let mut chars = arg.chars();
                chars.next();

                for c in chars {
                    let arg = format!("-{c}");
                    match c {
                        'd' => self.compile_file_destination(args, [outer.clone(), arg], &mut dst),
                        'h' => {
                            println!("{HELP_COMPILE_FILE}");
                            pexit(0);
                        },
                        's' => self.compile_file_source(args, [outer.clone(), arg], &mut src),
                        _ => self.unknown_sub(arg, Some(vec![ outer])),
                    }
                }
            }
            else {
                self.unknown_sub(arg, Some(vec![ outer, ]));
            }
        }

        if src.is_none() {
            self.missing("--source <src>", outer);
        }
        else if dst.is_none() {
            self.missing("--destination <dst>", outer);
        }

        self.actions.push(Action::CompileFile(CompileFile {
            source: src.unwrap(),
            destination: dst.unwrap(),
        }));
    }

    fn copy_directory_source(&mut self, args: &mut Args, outer: [String; 2], src: &mut Option<PathBuf>) {
        let [ a, b, ] = outer;

        if src.is_some() {
            self.mtonce_sub(b, Some(vec![ a, ]));
        }

        std::mem::swap(src, &mut args.next().map(|v| v.into()));
    }

    fn copy_directory_destination(&mut self, args: &mut Args, outer: [String; 2], dst: &mut Option<PathBuf>) {
        let [ a, b, ] = outer;

        if dst.is_some() {
            self.mtonce_sub(b, Some(vec![ a, ]));
        }

        std::mem::swap(dst, &mut args.next().map(|v| v.into()));
    }

    fn copy_directory_extension(&mut self, args: &mut Args, outer: [String; 2], exts: &mut Vec<String>) {
        let [ a, b, ] = outer;

        if let Some(arg) = args.next() {
            exts.push(arg);
        }
        else {
            self.missing_sub("<ext>", b, Some(vec![ a, ]));
        }
    }

    fn copy_directory(&mut self, args: &mut Args, outer: String) {
        let mut src = None;
        let mut dst = None;
        let mut exts = Vec::new();

        while let Some(arg) = args.next() {
            if arg.starts_with("--") {
                match arg.as_str() {
                    "--destination" => self.copy_directory_destination(args, [outer.clone(), arg], &mut dst),
                    "--extension" => self.copy_directory_extension(args, [outer.clone(), arg], &mut exts),
                    "--help" => {
                        println!("{HELP_COPY_DIRECTORY}");
                        pexit(0);
                    },
                    "--source" => self.copy_directory_source(args, [outer.clone(), arg], &mut src),
                    "--" => break,
                    _ => self.unknown_sub(arg, Some(vec![ outer, ])),
                }
            }
            else if arg.starts_with('-') {
                let mut chars = arg.chars();
                chars.next();

                for c in chars {
                    let arg = format!("-{c}");
                    match c {
                        'd' => self.copy_directory_destination(args, [outer.clone(), arg], &mut dst),
                        'e' => self.copy_directory_extension(args, [outer.clone(), arg], &mut exts),
                        'h' => {
                            println!("{HELP_COPY_DIRECTORY}");
                            pexit(0);
                        },
                        's' => self.copy_directory_source(args, [outer.clone(), arg], &mut src),
                        _ => self.unknown_sub(arg, Some(vec![ outer, ])),
                    }
                }
            }
            else {
                self.unknown_sub(arg, Some(vec![ outer, ]));
            }
        }

        if src.is_none() {
            self.missing("--source <src>", outer);
        }
        else if dst.is_none() {
            self.missing("--destination <dst>", outer);
        }

        self.actions.push(Action::CopyDirectory(CopyDirectory {
            source: src.unwrap(),
            destination: dst.unwrap(),
            extensions: if exts.is_empty() { None } else { Some(exts) },
        }));
    }

    fn copy_file_source(&mut self, args: &mut Args, outer: [String; 2], src: &mut Option<PathBuf>) {
        let [ a, b, ] = outer;

        if src.is_some() {
            self.mtonce_sub(b, Some(vec![ a, ]));
        }

        std::mem::swap(src, &mut args.next().map(String::into));
    }

    fn copy_file_destination(&mut self, args: &mut Args, outer: [String; 2], dst: &mut Option<PathBuf>) {
        let [ a, b, ] = outer;

        if dst.is_some() {
            self.mtonce_sub(b, Some(vec![ a, ]));
        }

        std::mem::swap(dst, &mut args.next().map(String::into));
    }

    fn copy_file(&mut self, args: &mut Args, outer: String) {
        let mut src = None;
        let mut dst = None;

        while let Some(arg) = args.next() {
            if arg.starts_with("--") {
                match arg.as_str() {
                    "--destination" => self.copy_file_destination(args, [outer.clone(), arg], &mut dst),
                    "--help" => {
                        println!("{HELP_COPY_FILE}");
                        pexit(0);
                    },
                    "--source" => self.copy_file_source(args, [outer.clone(), arg], &mut src),
                    "--" => break,
                    _ => self.unknown_sub(arg, Some(vec![ outer, ])),
                }
            }
            else if arg.starts_with('-') {
                let mut chars = arg.chars();
                chars.next();

                for c in chars {
                    let arg = format!("-{c}");
                    match c {
                        'd' => self.copy_file_destination(args, [outer.clone(), arg], &mut dst),
                        'h' => {
                            println!("{HELP_COPY_FILE}");
                            pexit(0);
                        },
                        's' => self.copy_file_source(args, [outer.clone(), arg], &mut src),
                        _ => self.unknown_sub(arg, Some(vec![ outer, ])),
                    }
                }
            }
            else {
                self.unknown_sub(arg, Some(vec![ outer, ]));
            }
        }

        if src.is_none() {
            self.missing("--source <src>", outer);
        }
        else if dst.is_none() {
            self.missing("--destination <dst>", outer);
        }

        self.actions.push(Action::CopyFile(CopyFile {
            source: src.unwrap(),
            destination: dst.unwrap(),
        }));
    }

    fn delete_file_file(&mut self, args: &mut Args, outer: [String; 2], files: &mut Vec<PathBuf>) {
        let [ a, b, ] = outer;

        if let Some(file) = args.next() {
            files.push(file.into());
        }
        else {
            self.missing_sub("<pth>", b, Some(vec![ a, ]));
        }
    }

    fn delete_file(&mut self, args: &mut Args, outer: String) {
        let mut files = Vec::new();

        while let Some(arg) = args.next() {
            if arg.starts_with("--") {
                match arg.as_str() {
                    "--file" => self.delete_file_file(args, [outer.clone(), arg], &mut files),
                    "--help" => {
                        println!("{HELP_DELETE_FILE}");
                        pexit(0);
                    },
                    "--" => break,
                    _ => self.unknown_sub(arg, Some(vec![ outer, ])),
                }
            }
            else if arg.starts_with('-') {
                let mut chars = arg.chars();
                chars.next();

                for c in chars {
                    let arg = format!("-{c}");
                    match c {
                        'f' => self.delete_file_file(args, [outer.clone(), arg], &mut files),
                        'h' => {
                            println!("{HELP_DELETE_FILE}");
                            pexit(0);
                        },
                        _ => self.unknown_sub(arg, Some(vec![ outer, ])),
                    }
                }
            }
            else {
                self.unknown_sub(arg, Some(vec![ outer, ]));
            }
        }

        if files.is_empty() {
            self.missing("--file <pth>", outer);
        }

        if files.len() == 1 {
            self.actions.push(Action::DeleteFile(DeleteFile {
                file: files.into_iter().next(),
                files: None,
            }));
        }
        else {
            self.actions.push(Action::DeleteFile(DeleteFile {
                file: None,
                files: Some(files),
            }));
        }
    }

    fn help(&self) -> ! {
        println!("{HELP}");
        pexit(0);
    }

    fn license_notice(&self) -> ! {
        println!("{NOTICE}");
        pexit(0);
    }

    fn license(&self) -> ! {
        println!("{LICENSE}");
        pexit(0);
    }

    fn missing_sub<I, P>(&self, item: I, parent: P, sub: Option<Vec<String>>) -> !
    where
        I: AsRef<str>,
        P: AsRef<str>,
    {
        let item = item.as_ref();
        let parent = parent.as_ref();
        let sub = sub.map(|v| format!("{}: ", v.join(": "))).unwrap_or_default();

        eprintln!("arcc: {sub}{parent}: Missing {item}");
        pexit(1);
    }

    fn missing<I, P>(&self, item: I, parent: P) -> !
    where
        I: AsRef<str>,
        P: AsRef<str>,
    {
        self.missing_sub(item, parent, None);
    }

    fn mtonce_sub<A>(&self, arg: A, sub: Option<Vec<String>>) -> !
    where
        A: AsRef<str>,
    {
        let arg = arg.as_ref();
        let sub = sub.map(|v| v.join(": ")).unwrap_or_default();

        eprintln!("arcc: {sub}: {arg} was defined more than once");
        pexit(1);
    }

    fn schema(&self) -> ! {
        println!("{SCHEMA}");
        pexit(0);
    }

    fn verbose(&mut self) {
        if self.verbose {
            eprintln!("Verbose was specified multiple times");
            pexit(1);
        }

        self.verbose = true;
    }

    fn version(&self) -> ! {
        println!("arcc v{}", env!("CARGO_PKG_VERSION"));
        pexit(0);
    }

    fn unknown_sub(&self, arg: String, parents: Option<Vec<String>>) -> ! {
        let sub = parents.map(|v| v.join(": ")).unwrap_or_default();
        eprintln!("arcc: {sub}:  Unknown argument \"{arg}\"");
        pexit(1);
    }

    fn unknown(&self, arg: String) -> ! {
        self.unknown_sub(arg, None);
    }
}

fn main() -> Result<()> {
    let mut opts = Options::default();

    let mut args = std::env::args();
    args.next(); // burn program name

    while let Some(arg) = args.next() {
        if arg.starts_with("--") {
            match arg.as_str() {
                "--clean" => opts.clean(),
                "--compile-against" => opts.compile_against(&mut args, arg),
                "--compile-directory" => opts.compile_directory(&mut args, arg),
                "--compile-file" => opts.compile_file(&mut args, arg),
                "--copy-directory" => opts.copy_directory(&mut args, arg),
                "--copy-file" => opts.copy_file(&mut args, arg),
                "--delete-file" => opts.delete_file(&mut args, arg),
                "--help" => opts.help(),
                "--license-notice" => opts.license_notice(),
                "--license" => opts.license(),
                "--schema" => opts.schema(),
                "--verbose" => opts.verbose(),
                "--version" => opts.version(),
                _ => opts.unknown(arg),
            }
        }
        else if arg.starts_with('-') {
            let mut chars = arg.chars();
            chars.next(); // burn '-'

            for c in chars {
                let arg = format!("-{c}");
                match c {
                    'a' => opts.compile_against(&mut args, arg),
                    'c' => opts.clean(),
                    'd' => opts.compile_directory(&mut args, arg),
                    'e' => opts.delete_file(&mut args, arg),
                    'f' => opts.compile_file(&mut args, arg),
                    'h' => opts.help(),
                    'i' => opts.copy_file(&mut args, arg),
                    'l' => opts.license_notice(),
                    'L' => opts.license(),
                    'r' => opts.copy_directory(&mut args, arg),
                    's' => opts.schema(),
                    'v' => opts.verbose(),
                    'V' => opts.version(),
                    _ => opts.unknown(c.to_string()),
                }
            }
        }
        else if opts.deployment.is_none() {
            opts.deployment = Some(arg.into());
        }
        else {
            eprintln!("Only one deployments may be specified (extra: {arg})");
            pexit(1);
        }
    }

    const DFLT_DEPL: &str = "arcana.json";

    let mut depl_dflted = false;

    if opts.actions.is_empty() && opts.deployment.is_none() {
        let mut depl_chk = std::env::current_dir().map_err(|e| Error::IO(e, PathBuf::new()))?;
        let mut depl = None;
        loop {
            if depl_chk.eq(&PathBuf::from("/")) || !depl_chk.is_dir() {
                break;
            }

            depl_chk.push(DFLT_DEPL);

            if !depl_chk.is_file() {
                depl_chk.pop(); // pop filename
                depl_chk.pop(); // pop directory
                continue;
            }

            let mut cd = depl_chk.clone();
            cd.pop();
            std::env::set_current_dir(&cd).map_err(|e| Error::IO(e, cd.clone()))?;

            depl = Some(depl_chk);
            break;
        }

        if depl.is_none() {
            eprintln!("Deployment or actions must be defined (no default \"{DFLT_DEPL}\" found).");
            pexit(1);
        }
        else {
            depl_dflted = true;
        }

        opts.deployment = depl;
    }

    let verbose = opts.verbose;
    if depl_dflted {
        let d = opts.deployment.as_ref().unwrap();
        vprint!(verbose, "Deployment and actions not defined, defaulted to {d:?}");
    }

    let dpath;
    let deployment = if opts.actions.is_empty() {
        dpath = opts.deployment.clone().unwrap();

        from_json_str::<Deployment>(
            &read_to_string(opts.deployment.as_ref().unwrap()).map_err(|e| Error::IO(e, opts.deployment.clone().unwrap()))?
        ).map_err(|e| Error::JsonParse(e, dpath.clone()))?
    }
    else {
        dpath = PathBuf::new();

        Deployment {
            actions: opts.actions,
        }
    };

    if opts.clean {
        do_clean(verbose, dpath, deployment)?;
        return Ok(());
    }

    for action in deployment.actions.into_iter() {
        match action {
            Action::CompileFile(cfile) => {
                compile_file(verbose, cfile.source, cfile.destination)?;
            },
            Action::CompileDirectory(cdir) => {
                let files = get_files_from_dir(
                    verbose,
                    &dpath,
                    &cdir.source.directory,
                    &cdir.source.extensions,
                    false,
                )?;

                for file in files {
                    let dst = as_output_path(
                        &file,
                        &cdir.destination.directory,
                        &cdir.destination.extension
                    )?;

                    compile_file(verbose, file, dst)?;
                }
            },
            Action::CompileAgainst(opts) => {
                if let Some(context) = opts.context {
                    compile_against(
                        verbose,
                        context,
                        opts.template,
                        opts.destination.directory,
                        &opts.destination.extension,
                        &opts.target
                    )?;
                }
                else if let Some(contexts) = opts.contexts {
                    for context in contexts {
                        compile_against(
                            verbose,
                            context,
                            &opts.template,
                            &opts.destination.directory,
                            &opts.destination.extension,
                            &opts.target
                        )?;
                    }
                }
                else if let Some(directory) = opts.context_directory {
                    let contexts = get_files_from_dir(
                        verbose,
                        &dpath,
                        &directory.path,
                        &directory.extensions,
                        false
                    )?;
                    for context in contexts {
                        compile_against(
                            verbose,
                            context,
                            &opts.template,
                            &opts.destination.directory,
                            &opts.destination.extension,
                            &opts.target
                        )?;
                    }
                }
                else {
                    eprintln!(concat!(
                        "Action \"compile-against\" requires at least one of [ ",
                        "\"context\", ",
                        "\"contexts\", ",
                        "\"context-directory\" ]."
                    ));

                    pexit(1);
                }
            },
            Action::CopyFile(cfile) => {
                vprint!(verbose, "Copying file {:?} to {:?}", cfile.source, cfile.destination);

                let mut dir = cfile.destination.clone();
                dir.pop();
                create_dir_all(&dir).map_err(|e| Error::IO(e, dir.clone()))?;

                let dest;
                if cfile.destination.is_dir() {
                    dest = as_output_path(&cfile.source, cfile.destination, &None)?;
                }
                else {
                    dest = cfile.destination;
                }

                copy(&cfile.source, &dest).map_err(|e| Error::IO(e, cfile.source.clone()))?;
            },
            Action::CopyDirectory(cdir) => {
                copy_dir_all(
                    verbose,
                    &dpath,
                    &cdir.extensions,
                    cdir.source,
                    cdir.destination
                )?;
            },
            Action::DeleteFile(delete) => {
                if let Some(file) = delete.file {
                    cnd_remove_file(verbose, &dpath, &file)?;
                }
                else if let Some(files) = delete.files {
                    for file in files {
                        cnd_remove_file(verbose, &dpath, &file)?;
                    }
                }
                else {
                    eprintln!(concat!(
                        "Action \"delete-file\" requires at least one of [ ",
                        "\"file\", ",
                        "\"files\" ]."
                    ));

                    pexit(1);
                }
            },
        }
    }

    Ok(())
}
