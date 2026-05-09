use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    env_logger::init();

    let mut args = std::env::args().skip(1);
    let mut input_path: Option<PathBuf> = None;
    let mut output_path: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-o" | "--output" => {
                let Some(v) = args.next() else {
                    eprintln!("error: -o requires a path");
                    return ExitCode::from(2);
                };
                output_path = Some(PathBuf::from(v));
            }
            "-h" | "--help" => {
                println!("usage: mermaid-little [input.mmd] [-o output.svg]");
                return ExitCode::SUCCESS;
            }
            other if !other.starts_with('-') => {
                input_path = Some(PathBuf::from(other));
            }
            other => {
                eprintln!("error: unknown flag: {other}");
                return ExitCode::from(2);
            }
        }
    }

    let source = match input_path {
        Some(p) => match std::fs::read_to_string(&p) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: reading {}: {e}", p.display());
                return ExitCode::FAILURE;
            }
        },
        None => {
            let mut s = String::new();
            if let Err(e) = io::stdin().read_to_string(&mut s) {
                eprintln!("error: reading stdin: {e}");
                return ExitCode::FAILURE;
            }
            s
        }
    };

    let svg = match mermaid_little::convert(&source) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    match output_path {
        Some(p) => {
            if let Err(e) = std::fs::write(&p, svg) {
                eprintln!("error: writing {}: {e}", p.display());
                return ExitCode::FAILURE;
            }
        }
        None => {
            if let Err(e) = io::stdout().write_all(svg.as_bytes()) {
                eprintln!("error: writing stdout: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    ExitCode::SUCCESS
}
