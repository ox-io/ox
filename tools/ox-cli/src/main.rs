use ox_lexer::{lex, CANONICAL_KEYWORDS, RESERVED_KEYWORDS};
use ox_parser::parse;
use ox_types::check;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::process::ExitCode;

mod package;

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
        [command, path] if command == "check" || command == "test" => {
            let source = load_modules(
                std::path::Path::new(path),
                &mut Vec::new(),
                &mut HashSet::new(),
            )?;
            let tokens = lex(&source).map_err(|error| format!("{path}: {error}"))?;
            let program = parse(&tokens).map_err(|error| format!("{path}: {error}"))?;
            check(&program).map_err(|error| format!("{path}: {error}"))?;
            Ok(if command == "test" {
                format!("Tested {path}")
            } else {
                format!("Checked {path}")
            })
        }
        [command] if command == "init" => {
            if std::path::Path::new("ox.toml").exists() {
                return Err("ox.toml already exists".into());
            }
            fs::write(
                "ox.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n",
            )
            .map_err(|e| e.to_string())?;
            Ok("Created ox.toml".into())
        }
        [command] if command == "lock" => package::lock(false, false),
        [command, flag] if command == "lock" && flag == "--offline" => package::lock(true, false),
        [command] if command == "update" => package::lock(false, true),
        [command, name, spec] if command == "add" => package::add(name, spec),
        [command, name] if command == "remove" => package::remove(name),
        [command] if command == "vendor" => package::vendor(),
        [command] if command == "audit" => package::audit(),
        _ => Err("usage: ox <check FILE|test FILE|init|lock [--offline]|add NAME SPEC|remove NAME|update|vendor|audit|keywords|version>".into()),
    }
}

fn load_modules(
    path: &std::path::Path,
    visiting: &mut Vec<std::path::PathBuf>,
    loaded: &mut HashSet<std::path::PathBuf>,
) -> Result<String, String> {
    let path = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if visiting.contains(&path) {
        return Err(format!("module import cycle at {}", path.display()));
    }
    if !loaded.insert(path.clone()) {
        return Ok(String::new());
    }
    visiting.push(path.clone());
    let source = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut dependencies = String::new();
    let mut body = Vec::new();
    let mut aliases = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("use \"") {
            let end = rest.find('"').ok_or("invalid module import")?;
            if let Some(alias) = rest[end + 1..].trim().strip_prefix("as ") {
                aliases.push(alias.trim_end_matches(';').trim().to_owned());
            }
            dependencies.push_str(&load_modules(
                &path.parent().unwrap().join(&rest[..end]),
                visiting,
                loaded,
            )?);
            dependencies.push('\n');
        } else {
            body.push(line);
        }
    }
    visiting.pop();
    let mut body = body.join("\n");
    for alias in aliases {
        body = body.replace(&format!("{alias}."), "");
    }
    let body = body
        .replace("pub fx ", "fx ")
        .replace("pub shape ", "shape ")
        .replace("pub choice ", "choice ")
        .replace("pub api ", "api ");
    Ok(format!("{dependencies}{body}"))
}
