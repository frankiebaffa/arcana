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
    args::{
        Arguments,
        OptionType,
    },
    std::{
        path::PathBuf,
        process::exit as pexit,
    },
    arcana_core::{
        Error,
        Parser,
        Result
    },
};

#[derive(Default)]
struct Options {
    template: Option<PathBuf>,
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
                "V"|"version" => {
                    println!("arcc v{}", env!("CARGO_PKG_VERSION"));
                    pexit(0);
                },
                _ => {
                    eprintln!("Unknown argument \"{}\".", arg.qualifier());
                    pexit(1);
                },
            },
            OptionType::Value(p) => opts.template = Some(p.into()),
        }

        Ok(())
    }).map_err(|e| Error::IO(e, PathBuf::new()))?;

    if opts.template.is_none() {
        eprintln!("Template must be defined.");
        pexit(1);
    }

    let mut parser = Parser::new(opts.template.as_ref().unwrap())?;
    parser.parse()?;
    let output = parser.as_output();

    println!("{output}");

    Ok(())
}
