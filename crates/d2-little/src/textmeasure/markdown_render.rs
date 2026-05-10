//! Markdown -> sanitised HTML for d2 label rich-text.
//!
//! Pure Rust, no font work. Lives separately from `d2_go_emulation`
//! (which is native-only) so that the `crate::textmeasure::render_markdown`
//! entry point is available on wasm too — the SVG renderer needs it for
//! tooltips / connection labels / shape labels regardless of which
//! `D2Metrics` backend is active.

use std::sync::LazyLock;

use markdown::{CompileOptions, Constructs, Options, ParseOptions};
use regex::Regex;

static HREF_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"href="([^"]*)""#).expect("href regex"));

fn markdown_options() -> Options {
    Options {
        parse: ParseOptions {
            constructs: Constructs {
                gfm_strikethrough: true,
                gfm_table: true,
                ..Constructs::default()
            },
            ..ParseOptions::default()
        },
        compile: CompileOptions {
            allow_dangerous_html: true,
            allow_dangerous_protocol: true,
            ..CompileOptions::default()
        },
    }
}

fn sanitize_links(input: &str) -> String {
    HREF_RE
        .replace_all(input, |caps: &regex::Captures<'_>| {
            let value = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            let value = value.replace("&amp;", "TEMP_AMP");
            let value = value.replace('&', "&amp;");
            let value = value.replace("TEMP_AMP", "&amp;");
            format!(r#"href="{}""#, value)
        })
        .into_owned()
}

pub fn render_markdown(input: &str) -> Result<String, String> {
    let rendered = markdown::to_html_with_options(input, &markdown_options())
        .map_err(|e| format!("markdown render failed: {e}"))?;
    let mut rendered = sanitize_links(&rendered);
    if !rendered.is_empty() && !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    Ok(rendered)
}
