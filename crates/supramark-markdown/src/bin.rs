use std::io::{Read, Write};

#[cfg(not(tarpaulin_include))]
fn main() {
    let mut input = "-".to_owned();
    let mut output = "-".to_owned();
    let mut options_json: Option<String> = None;
    let mut no_gfm_autolink = false;

    {
        let mut cli = argparse::ArgumentParser::new();

        cli.add_option(
            &["-v", "--version"],
            argparse::Print(env!("CARGO_PKG_VERSION").to_owned()),
            "Show version",
        );

        cli.refer(&mut output)
            .add_option(&["-o", "--output"], argparse::Store, "File to write");

        // Per-case parse options as JSON (see `ParseOptions`).
        // `{ "disable": ["codeIndented"], "allowDangerousHtml": true, ... }`.
        cli.refer(&mut options_json).add_option(
            &["--options"],
            argparse::StoreOption,
            "Parse options as JSON (default: supramark defaults)",
        );

        cli.refer(&mut input)
            .add_argument("file", argparse::Store, "File to read");

        cli.refer(&mut no_gfm_autolink).add_option(
            &["--no-gfm-autolink"],
            argparse::StoreTrue,
            "Disable the GFM bare-URL/email autolink extension (CommonMark profile)",
        );

        cli.parse_args_or_exit();
    }

    let vec = if input == "-" {
        let mut vec = Vec::new();
        std::io::stdin().read_to_end(&mut vec).unwrap();
        vec
    } else {
        std::fs::read(input).unwrap()
    };

    let source = String::from_utf8_lossy(&vec);
    let mut options = match options_json {
        Some(json) => serde_json::from_str::<supramark_markdown::ParseOptions>(&json)
            .unwrap_or_else(|e| {
                eprintln!("failed to parse --options JSON: {e}");
                std::process::exit(1);
            }),
        None => supramark_markdown::ParseOptions::default(),
    };
    if no_gfm_autolink {
        options.gfm_autolink = false;
    }
    let ast = supramark_markdown::parse_with_options(&source, options);
    let result = serde_json::to_string_pretty(&ast).unwrap();

    if output == "-" {
        std::io::stdout().write_all(result.as_bytes()).unwrap();
        std::io::stdout().write_all(b"\n").unwrap();
    } else {
        std::fs::write(output, result).unwrap();
    }
}
