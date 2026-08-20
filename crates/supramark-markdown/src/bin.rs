use std::io::{Read, Write};

#[cfg(not(tarpaulin_include))]
fn main() {
    let mut input = "-".to_owned();
    let mut output = "-".to_owned();
    let mut no_gfm_autolink = false;
    let mut wikilink = false;

    {
        let mut cli = argparse::ArgumentParser::new();

        cli.add_option(
            &["-v", "--version"],
            argparse::Print(env!("CARGO_PKG_VERSION").to_owned()),
            "Show version",
        );

        cli.refer(&mut output)
            .add_option(&["-o", "--output"], argparse::Store, "File to write");

        cli.refer(&mut input)
            .add_argument("file", argparse::Store, "File to read");

        cli.refer(&mut no_gfm_autolink).add_option(
            &["--no-gfm-autolink"],
            argparse::StoreTrue,
            "Disable the GFM bare-URL/email autolink extension (CommonMark profile)",
        );

        cli.refer(&mut wikilink).add_option(
            &["--wikilink"],
            argparse::StoreTrue,
            "Enable the WikiLink extension ([[target]], [[target|label]], [[target#section]])",
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
    let mut options = supramark_markdown::ParseOptions::default();
    if no_gfm_autolink {
        options.gfm_autolink = false;
    }
    if wikilink {
        options.wikilink = true;
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
