//! The deployer for the Arcana Templating Engine.
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
    args::{
        Arguments,
        OptionType,
    },
    serde::Deserialize,
    serde_json::from_str as from_json_str,
    std::{
        fs::{
            create_dir_all,
            copy,
            read_to_string,
            write,
        },
        path::PathBuf,
        process::exit as pexit,
    },
    arcana_core::{ Error, Parser, Result, },
};

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
#[serde(rename_all = "kebab-case")]
enum Action {
    CompileFile(CompileFile),
    CompileDirectory(CompileDirectory),
    CopyFile(CopyFile),
    CopyDirectory(CopyDirectory),
}

#[derive(Deserialize)]
struct Deployment {
    actions: Vec<Action>,
}

#[derive(Default)]
struct Options {
    deployment: Option<PathBuf>,
    verbose: bool,
}

const HELP: &str = include_str!("../resources/help.txt");
const NOTICE: &str = include_str!("../NOTICE.txt");
const LICENSE: &str = include_str!("../../LICENSE.md");

fn main() -> Result<()> {
    let mut opts = Options::default();

    Arguments::with_args(&mut opts, |_, opts, arg| {
        match arg.option_type() {
            OptionType::Argument(_) => match arg.qualifier() {
                "h"|"help" => {
                    println!("{HELP}");
                    pexit(0);
                },
                "l"|"license-notice" => {
                    println!("{NOTICE}");
                    pexit(0);
                },
                "L"|"license" => {
                    println!("{LICENSE}");
                    pexit(0);
                },
                "s"|"schema" => {
                    println!("{SCHEMA}");
                    pexit(0);
                },
                "v"|"verbose" => {
                    opts.verbose = true;
                },
                "V"|"version" => {
                    println!("arcd v{}", env!("CARGO_PKG_VERSION"));
                    pexit(0);
                },
                _ => {
                    eprintln!("Unknown argument \"{}\".", arg.qualifier());
                    pexit(1);
                },
            },
            OptionType::Value(p) => opts.deployment = Some(p.into()),
        }

        Ok(())
    }).map_err(|e| Error::IO(e, PathBuf::new()))?;

    if opts.deployment.is_none() {
        eprintln!("Deployment must be defined.");
        pexit(1);
    }

    let dpath = opts.deployment.clone().unwrap();

    let deployment = from_json_str::<Deployment>(
        &read_to_string(opts.deployment.unwrap()).map_err(|e| Error::IO(e, dpath.clone()))?
    ).map_err(|e| Error::JsonParse(e, dpath.clone()))?;

    let verbose = opts.verbose;

    macro_rules! vprint {
        ($msg:expr$(, $fmt:expr)*) => {
            if verbose {
                println!($msg$(, $fmt)*);
            }
        }
    }

    for action in deployment.actions.into_iter() {
        match action {
            Action::CompileFile(cfile) => {
                vprint!("Compiling single file {:?}", cfile.source);

                let mut parser = Parser::new(&cfile.source)?;
                parser.parse()?;

                let mut dir = cfile.destination.clone();
                dir.pop();
                create_dir_all(dir).map_err(|e| Error::IO(e, dpath.clone()))?;

                write(&cfile.destination, &parser.as_output()).map_err(|e| Error::IO(e, dpath.clone()))?;
            },
            Action::CompileDirectory(cdir) => {
                vprint!("Compiling directory {:?}", cdir.source.directory);

                for e_res in cdir.source.directory.read_dir().map_err(|e| Error::IO(e, dpath.clone()))? {
                    let entry = e_res.map_err(|e| Error::IO(e, dpath.clone()))?;
                    let path = entry.path();

                    if !path.is_file() {
                        continue;
                    }

                    let filename = path.file_stem()
                        .map(|v| v.to_str().unwrap_or(""))
                        .or(Some(""))
                        .map(|v| v.to_owned())
                        .unwrap();
                    let ext = path.extension()
                        .map(|v| v.to_str().unwrap_or(""))
                        .or(Some(""))
                        .map(|v| v.to_owned())
                        .unwrap();

                    if let Some(exts) = &cdir.source.extensions {
                        if !exts.is_empty() && !exts.contains(&ext) {
                            continue;
                        }
                    }

                    vprint!("  Compiling file {path:?}");

                    let mut parser = Parser::new(&path)?;
                    parser.parse()?;

                    let mut dest = cdir.destination.directory.clone();
                    if let Some(ext) = &cdir.destination.extension {
                        dest.push(format!("{filename}.{ext}"));
                    }
                    else {
                        dest.push(format!("{filename}.{ext}"));
                    }

                    vprint!("  Writing to {dest:?}");

                    let mut dir = dest.clone();
                    dir.pop();
                    create_dir_all(dir).map_err(|e| Error::IO(e, dpath.clone()))?;

                    write(dest, parser.as_output()).map_err(|e| Error::IO(e, dpath.clone()))?;
                }
            },
            Action::CopyFile(cfile) => {
                vprint!("Copying file {:?}", cfile.source);
                vprint!("  To {:?}", cfile.destination);

                let mut dir = cfile.destination.clone();
                dir.pop();
                create_dir_all(dir).map_err(|e| Error::IO(e, dpath.clone()))?;

                copy(&cfile.source, &cfile.destination).map_err(|e| Error::IO(e, dpath.clone()))?;
            },
            Action::CopyDirectory(cdir) => {
                vprint!("Copying directory {:?}", cdir.source);

                for e_res in cdir.source.read_dir().map_err(|e| Error::IO(e, dpath.clone()))? {
                    let entry = e_res.map_err(|e| Error::IO(e, dpath.clone()))?;
                    let path = entry.path();

                    if !path.is_file() {
                        continue;
                    }

                    vprint!("  Copying file {path:?}");

                    let filename = path.file_stem()
                        .map(|v| v.to_str().unwrap_or(""))
                        .or(Some(""))
                        .map(|v| v.to_owned())
                        .unwrap();
                    let ext = path.extension()
                        .map(|v| v.to_str().unwrap_or(""))
                        .or(Some(""))
                        .map(|v| v.to_owned())
                        .unwrap();

                    if let Some(exts) = &cdir.extensions {
                        if !exts.is_empty() && !exts.contains(&ext) {
                            continue;
                        }
                    }

                    let mut dest = cdir.destination.to_owned();
                    create_dir_all(&dest).map_err(|e| Error::IO(e, dpath.clone()))?;

                    dest.push(format!("{filename}.{ext}"));

                    vprint!("  To {dest:?}");

                    copy(&path, dest).map_err(|e| Error::IO(e, dpath.clone()))?;
                }
            },
        }
    }

    Ok(())
}
