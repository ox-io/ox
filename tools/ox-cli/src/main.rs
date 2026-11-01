use ox_lexer::{lex, CANONICAL_KEYWORDS, RESERVED_KEYWORDS};
use ox_parser::parse;
use ox_types::check_entrypoint;
use std::env;
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(output) => {
            if !output.is_empty() {
                println!("{output}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<String, String> {
    match args.as_slice() {
        [command] if command == "version" || command == "--version" => {
            Ok(format!("ox {}", env!("CARGO_PKG_VERSION")))
        }
        [command] if command == "keywords" => Ok(format!(
            "Implemented ({}): {}\nReserved ({}): {}",
            CANONICAL_KEYWORDS.len(),
            CANONICAL_KEYWORDS.join(" "),
            RESERVED_KEYWORDS.len(),
            RESERVED_KEYWORDS.join(" ")
        )),
        [command, path] if command == "check" => {
            let source = fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
            let tokens = lex(&source).map_err(|error| format!("{path}: {error}"))?;
            let program = parse(&tokens).map_err(|error| format!("{path}: {error}"))?;
            check_entrypoint(&program).map_err(|error| format!("{path}: {error}"))?;
            Ok(format!("Checked {path}"))
        }
        _ => Err("usage: ox <check FILE|keywords|version>".into()),
    }
}
