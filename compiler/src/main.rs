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
    std::{
        env::Args,
        io::{
            BufRead,
            stdin,
            stdout,
            Write,
        },
        path::PathBuf,
        process::exit as pexit,
    },
    arcana_core::{ Error, Parser, Result, },
};

const HELP: &str = include_str!("../resources/help.txt");
const NOTICE: &str = include_str!("../NOTICE.txt");
const LICENSE: &str = include_str!("../../LICENSE.md");

#[derive(Default)]
struct Options {
    interactive: bool,
    from_string: Option<String>,
    path: Option<PathBuf>,
    quiet: bool,
}

impl Options {
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

    fn version(&self) -> ! {
        println!("arcc v{}", env!("CARGO_PKG_VERSION"));
        pexit(0);
    }

    fn err<S>(&self, msg: S) -> !
    where
        S: AsRef<str>
    {
        let msg = msg.as_ref();
        eprintln!("arcc: {msg}");
        pexit(1);
    }

    fn interactive(&mut self) {
        if self.interactive {
            self.err("interactive specified more than once.");
        }
        else if self.path.is_some() {
            self.err("interactive cannot be specified alongside path.");
        }
        else if self.from_string.is_some() {
            self.err("interactive cannot be specified alongside from-string.");
        }

        self.interactive = true;
    }

    fn quiet(&mut self) {
        if self.quiet {
            self.err("quiet specified more than once.");
        }

        self.quiet = true;
    }

    fn handle_from_string(&mut self, args: &mut Args) {
        if self.from_string.is_some() {
            self.err("from-string specified more than once.");
        }
        else if self.interactive {
            self.err("from-string cannot be specified alongside interactive.");
        }
        else if self.path.is_some() {
            self.err("from-string cannot be specified alongside path.");
        }

        let input = args.next();
        if input.is_none() {
            self.err("from-string requires a value.");
        }

        self.from_string = Some(input.unwrap());
    }

    fn path(&mut self, path: String) {
        if self.path.is_some() {
            self.err("path specified more than once.");
        }
        else if self.interactive {
            self.err("path cannot be specified alongside interactive.");
        }
        else if self.from_string.is_some() {
            self.err("path cannot be specified alongside from-string.");
        }

        self.path = Some(path.into());
    }

    fn unknown(&mut self, arg: String) -> ! {
        self.err(format!("unknown argument \"{arg}\""));
    }
}

fn interactive() -> Result<Parser> {
    let pwd = std::env::current_dir().map_err(|e| Error::IO(e, PathBuf::new()))?;

    let mut lines = Vec::new();

    eprintln!("<<EOF");
    stdout().flush().map_err(|e| Error::IO(e, pwd.to_owned()))?;

    for line in stdin().lock().lines() {
        let line = line.map_err(|e| Error::IO(e, pwd.to_owned()))?;
        lines.push(line);
    }
    eprintln!("EOF");

    let input = lines.join("\n");

    let mut faux_path = pwd.clone();
    faux_path.push("interactive.txt");

    Parser::from_string_and_path(faux_path, input)
}

fn from_string(input: String) -> Result<Parser> {
    let pwd = std::env::current_dir().map_err(|e| Error::IO(e, PathBuf::new()))?;

    let input = input.lines().map(|s| s.to_owned()).collect::<Vec<String>>().join("\n");

    let mut faux_path = pwd.clone();
    faux_path.push("interactive.txt");

    Parser::from_string_and_path(faux_path, input)
}

fn print_or_quiet(quiet: bool, p: Parser) {
    if quiet {
        return;
    }

    println!("{}", p.as_output());
}

fn main() -> Result<()> {
    let mut opts = Options::default();

    let mut args = std::env::args();
    args.next(); // burn program name

    while let Some(arg) = args.next() {
        if arg.starts_with("--") {
            match arg.as_str() {
                "--help" => opts.help(),
                "--interactive" => opts.interactive(),
                "--license-notice" => opts.license_notice(),
                "--license" => opts.license(),
                "--from-string" => opts.handle_from_string(&mut args),
                "--version" => opts.version(),
                "--quiet" => opts.quiet(),
                _ => opts.unknown(arg),
            }
        }
        else if arg.starts_with('-') {
            let mut chars = arg.chars();
            chars.next(); // burn '-'

            for c in chars {
                let arg = format!("-{c}");
                match c {
                    'h' => opts.help(),
                    'i' => opts.interactive(),
                    'l' => opts.license_notice(),
                    'L' => opts.license(),
                    'q' => opts.quiet(),
                    's' => opts.handle_from_string(&mut args),
                    'V' => opts.version(),
                    _ => opts.unknown(arg),
                }
            }
        }
        else {
            opts.path(arg);
        }
    }

    let mut p = if opts.interactive {
        interactive()?
    }
    else if opts.from_string.is_some() {
        from_string(opts.from_string.unwrap())?
    }
    else if opts.path.is_none() {
        opts.err("path must be specified when not in interactive or from-string mode.");
    }
    else {
        Parser::new(opts.path.unwrap())?
    };

    match p.parse() {
        Ok(_) => print_or_quiet(opts.quiet, p),
        Err(e) => {
            print_or_quiet(opts.quiet, p);
            Result::<()>::Err(e)?;
        },
    }

    Ok(())
}
