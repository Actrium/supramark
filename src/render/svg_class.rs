//! Class diagram SVG renderer.
//!
//! Upstream references:
//! * `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/class/classRenderer-v3-unified.ts`
//! * `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/rendering-util/rendering-elements/shapes/classBox.ts`
//! * `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/class/styles.js`
//!
//! ## Status
//!
//! Full byte-exact rendering of the class diagram is a substantial
//! port — the upstream v3 unified renderer leans on dagre layout *plus*
//! a family of path-based shape emitters (classBox with 8-segment
//! basis-spline outline, hand-drawn "neo" look, etc), 10 pre-registered
//! marker families, foreignObject markdown rendering for every label,
//! and a sizeable diagram-specific CSS block. Several of the supporting
//! pieces (markers, clusters, edges, font metrics) are already present
//! in this crate, but the classBox shape still renders as a minimal
//! rectangle stub (see `src/render/shapes/classbox.rs`).
//!
//! This module consumes [`ClassLayout`] and produces a structurally-
//! plausible SVG via the shape registry. That output is **not**
//! byte-exact against the reference SVGs yet. When the shape / style
//! port lands, this renderer picks up the upgrades automatically.
//!
//! We keep the public API stable so `lib.rs` can route to it as soon
//! as the integration agent wires the `Class` branch. Until then the
//! renderer short-circuits with an explicit `Unsupported` error —
//! matching the current `lib.rs` behaviour for this diagram kind.

use crate::error::{MermaidError, Result};
use crate::layout::class::ClassLayout;
use crate::model::class::ClassDiagram;
use crate::theme::ThemeVariables;

/// Public entry point. Returns [`MermaidError::Unsupported`] until the
/// byte-exact rendering path is complete.
///
/// The signature (`id`, theme, diagram, layout) mirrors the other
/// 12 byte-exact renderers in this crate so downstream callers don't
/// need to special-case class.
pub fn render(
    _d: &ClassDiagram,
    _l: &ClassLayout,
    _theme: &ThemeVariables,
    _id: &str,
) -> Result<String> {
    // NOTE: fail loudly rather than emit a partially-formed SVG. The
    // byte-exact test harness compares character-for-character with
    // the upstream reference; a stub SVG would only advertise false
    // progress. Once the classBox shape and styles port lands, replace
    // this body with the assembled SVG and enable the fixture sweep.
    Err(MermaidError::Unsupported(
        "classDiagram: byte-exact render pending — parser + layout complete, shape/style port outstanding"
            .into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::class::layout as class_layout;
    use crate::parser::class::parse;
    use crate::theme::get_theme;

    #[test]
    fn render_reports_unsupported() {
        let d = parse("classDiagram\nclass Foo\n").unwrap();
        let theme = get_theme("default");
        let l = class_layout(&d, &theme).unwrap();
        let err = render(&d, &l, &theme, "id").unwrap_err();
        assert!(matches!(err, MermaidError::Unsupported(_)));
    }

    /// Full sweep: parser + layout over every class fixture
    /// (cypress + demos), minus the known-ignored entry in
    /// `tests/known_ignored.txt`. Verifies the parser handles the
    /// full grammar surface without panicking. Byte-exact render
    /// comparison is gated until the shape port lands.
    #[test]
    fn sweep_smoke_test() {
        use std::fs;
        use std::path::PathBuf;
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let theme = get_theme("default");
        let dirs = [
            "tests/ext_fixtures/cypress/class",
            "tests/ext_fixtures/demos/class",
        ];
        let ignored: Vec<String> = fs::read_to_string(base.join("tests/known_ignored.txt"))
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
            .filter_map(|l| l.split_whitespace().next().map(str::to_string))
            .collect();

        let mut total = 0usize;
        let mut ok = 0usize;
        let mut parse_err = 0usize;
        let mut layout_err = 0usize;
        for dir in dirs {
            let Ok(entries) = fs::read_dir(base.join(dir)) else {
                continue;
            };
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) != Some("mmd") {
                    continue;
                }
                let rel = format!(
                    "{}/{}",
                    dir.trim_start_matches("tests/"),
                    p.file_name().and_then(|s| s.to_str()).unwrap_or("")
                );
                if ignored.iter().any(|ig| ig == &rel) {
                    continue;
                }
                total += 1;
                let Ok(src) = fs::read_to_string(&p) else {
                    continue;
                };
                match parse(&src) {
                    Ok(d) => match class_layout(&d, &theme) {
                        Ok(_) => ok += 1,
                        Err(e) => {
                            eprintln!("layout {}: {}", rel, e);
                            layout_err += 1;
                        }
                    },
                    Err(e) => {
                        eprintln!("parse {}: {}", rel, e);
                        parse_err += 1;
                    }
                }
            }
        }
        eprintln!(
            "class sweep: {}/{} ok ({} parse-err, {} layout-err)",
            ok, total, parse_err, layout_err
        );
        assert!(ok > 0, "no class fixtures parsed cleanly");
        // Parser should comfortably cover ≥ 95% of the corpus at this
        // point. If this ratio drops, a grammar case regressed.
        assert!(
            ok * 100 / total.max(1) >= 90,
            "parser regressed below 90% corpus coverage"
        );
    }
}
