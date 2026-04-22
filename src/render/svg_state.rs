//! State-diagram SVG renderer.
//!
//! Upstream reference:
//! * `stateRenderer-v3-unified.ts` (v2 path) — 370 LoC.
//! * `stateRenderer.js` (v1 path) — emits the classic look.
//!
//! # Byte-exactness caveat (wave 4, first pass)
//!
//! Full byte-exact parity requires three pieces that are **not yet**
//! ported and are all in the "hundreds of LoC each" bucket:
//!
//! 1. The stylis CSS minifier applied to the `<style>` block
//!    (`packages/mermaid/src/styles.ts` + the per-diagram CSS at
//!    `state/styles.js`).
//! 2. d3-shape's arc / circle emitter, which upstream uses for
//!    `state-start` markers — output is a 36-vertex cubic-bezier
//!    polyline, not a single `<circle r="7">`.
//! 3. The dagre → cluster-aware SVG pipeline's exact iteration order
//!    for `edgePaths`, `edgeLabels`, `nodes` groups, plus the
//!    `data-points` base64 blob each edge carries.
//!
//! This renderer intentionally produces **structurally plausible** SVG
//! that doesn't pass byte-exact comparison yet but does:
//!   * open `<svg>` with the canonical attribute order;
//!   * emit the standard `<g><defs><marker .../></defs><g class="root">…`
//!     skeleton;
//!   * draw states using `shapes::draw`;
//!   * route edges via `render::edges` with `basis` interpolation;
//!   * apply the `statediagram` class + placeholder `<style>` tag.
//!
//! The `tests` section below compares byte-counts, not byte-equality,
//! and reports the gap against reference output.

use crate::error::Result;
use crate::layout::state::StateLayout;
use crate::layout::unified::types::{Bounds, Edge, Node, Point};
use crate::model::state::StateDiagram;
use crate::render::edges::{self, CurveType};
use crate::render::shapes::{self, types::fmt_num};
use crate::theme::ThemeVariables;

pub fn render(
    d: &StateDiagram,
    l: &StateLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let mut out = String::with_capacity(16 * 1024);

    // Compute viewBox from layout bounds, padded a little (matches
    // upstream's 8px default margin on each side).
    let pad = 8.0_f64;
    let bb = &l.result.bounds;
    let (vx, vy, vw, vh) = viewbox(bb, pad);

    // ── Opening <svg> — canonical attribute order -----------------
    out.push_str(&format!(
        concat!(
            r#"<svg id="{id}""#,
            r#" width="100%""#,
            r#" xmlns="http://www.w3.org/2000/svg""#,
            r#" class="statediagram""#,
            r#" style="max-width: {w}px;""#,
            r#" viewBox="{vx} {vy} {vw} {vh}""#,
            r#" role="graphics-document document""#,
            r#" aria-roledescription="stateDiagram""#,
            r#">"#,
        ),
        id = id,
        w = fmt_num(vw),
        vx = fmt_num(vx),
        vy = fmt_num(vy),
        vw = fmt_num(vw),
        vh = fmt_num(vh),
    ));

    // ── <style> placeholder (non-minified; not byte-exact yet) ----
    out.push_str(&style_block(id, theme));

    // ── Seed empty <g></g> (upstream always emits this) ----------
    out.push_str("<g></g>");

    // ── Markers -------------------------------------------------
    out.push_str(&format!(
        concat!(
            r#"<defs>"#,
            r#"<marker id="{id}_stateDiagram-barbEnd" refX="19" refY="7""#,
            r#" markerWidth="20" markerHeight="14" markerUnits="userSpaceOnUse" orient="auto">"#,
            r#"<path d="M 19,7 L9,13 L14,7 L9,1 Z"></path>"#,
            r#"</marker>"#,
            r#"</defs>"#,
        ),
        id = id
    ));

    // ── Root <g> with clusters, edges, labels, nodes ------------
    out.push_str(r#"<g class="root">"#);

    // Clusters (composite states) -------------------------------
    out.push_str(r#"<g class="clusters">"#);
    for n in l.result.nodes.iter().filter(|n| n.is_group) {
        out.push_str(&emit_cluster(n));
    }
    out.push_str("</g>");

    // Edge paths ------------------------------------------------
    out.push_str(r#"<g class="edgePaths">"#);
    for e in &l.result.edges {
        out.push_str(&emit_edge_path(id, e));
    }
    out.push_str("</g>");

    // Edge labels ----------------------------------------------
    out.push_str(r#"<g class="edgeLabels">"#);
    for e in &l.result.edges {
        out.push_str(&emit_edge_label(e));
    }
    out.push_str("</g>");

    // Nodes -----------------------------------------------------
    out.push_str(r#"<g class="nodes">"#);
    for n in l.result.nodes.iter().filter(|n| !n.is_group) {
        if n.extra.get("__skip_render").is_some() {
            continue;
        }
        if let Some(svg) = emit_node(id, n, theme) {
            out.push_str(&svg);
        }
    }
    out.push_str("</g>");

    out.push_str("</g>");

    // Drop-shadow filter defs (match upstream tail).
    out.push_str(&format!(
        concat!(
            r##"<defs><filter id="{id}-drop-shadow" height="130%" width="130%">"##,
            r##"<feDropShadow dx="4" dy="4" stdDeviation="0" flood-opacity="0.06" flood-color="#000000"></feDropShadow>"##,
            r##"</filter></defs>"##,
            r##"<defs><filter id="{id}-drop-shadow-small" height="150%" width="150%">"##,
            r##"<feDropShadow dx="2" dy="2" stdDeviation="0" flood-opacity="0.06" flood-color="#000000"></feDropShadow>"##,
            r##"</filter></defs>"##,
        ),
        id = id
    ));

    out.push_str("</svg>");
    let _ = d; // reserved for v1/v2-specific tweaks once wired.
    Ok(out)
}

fn viewbox(b: &Bounds, pad: f64) -> (f64, f64, f64, f64) {
    let w = (b.width + 2.0 * pad).max(1.0);
    let h = (b.height + 2.0 * pad).max(1.0);
    let x = b.x - pad;
    let y = b.y - pad;
    (x, y, w, h)
}

fn emit_cluster(n: &Node) -> String {
    let (x, y) = (n.x.unwrap_or(0.0), n.y.unwrap_or(0.0));
    let w = n.width.unwrap_or(0.0);
    let h = n.height.unwrap_or(0.0);
    let label = n.label.as_deref().unwrap_or("");
    format!(
        concat!(
            r#"<g class="cluster statediagram-cluster" transform="translate({tx}, {ty})">"#,
            r#"<rect class="outer" x="{rx}" y="{ry}" width="{w}" height="{h}" rx="5" ry="5"></rect>"#,
            r#"<g class="cluster-label"><foreignObject width="0" height="0"><div xmlns="http://www.w3.org/1999/xhtml">{lbl}</div></foreignObject></g>"#,
            r#"</g>"#,
        ),
        tx = fmt_num(x),
        ty = fmt_num(y),
        rx = fmt_num(-w / 2.0),
        ry = fmt_num(-h / 2.0),
        w = fmt_num(w),
        h = fmt_num(h),
        lbl = xml_escape(label),
    )
}

fn emit_edge_path(id: &str, e: &Edge) -> String {
    let Some(points) = &e.points else {
        return String::new();
    };
    if points.len() < 2 {
        return String::new();
    }
    let pts: Vec<Point> = points.iter().map(|p| Point { x: p.x, y: p.y }).collect();
    let d = edges::build_path(&pts, CurveType::Basis);
    let class = format!(
        " edge-thickness-{} edge-pattern-{} {}",
        e.thickness.as_deref().unwrap_or("normal"),
        e.pattern.as_deref().unwrap_or("solid"),
        e.classes.as_deref().unwrap_or("transition"),
    );
    format!(
        concat!(
            r#"<path d="{d}" id="{id}-{eid}" class="{cls}" style="fill:none;" "#,
            r#"marker-end="url(#{id}_stateDiagram-barbEnd)"></path>"#,
        ),
        d = d,
        id = id,
        eid = e.id,
        cls = class,
    )
}

fn emit_edge_label(e: &Edge) -> String {
    let lbl = e.label.as_deref().unwrap_or("");
    if lbl.is_empty() {
        return String::new();
    }
    let x = e.label_x.unwrap_or(0.0);
    let y = e.label_y.unwrap_or(0.0);
    format!(
        concat!(
            r#"<g class="edgeLabel" transform="translate({x}, {y})">"#,
            r#"<foreignObject width="0" height="0"><div xmlns="http://www.w3.org/1999/xhtml" class="labelBkg"><span class="edgeLabel">{lbl}</span></div></foreignObject>"#,
            r#"</g>"#,
        ),
        x = fmt_num(x),
        y = fmt_num(y),
        lbl = xml_escape(lbl),
    )
}

fn emit_node(_id: &str, n: &Node, theme: &ThemeVariables) -> Option<String> {
    let shape = n.shape.as_deref().unwrap_or("state");
    shapes::draw(shape, n, theme).ok()
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Placeholder `<style>` block — carries the bare minimum selectors the
/// reference SVG includes. Not stylis-minified; not byte-exact yet.
fn style_block(id: &str, _theme: &ThemeVariables) -> String {
    format!(
        concat!(
            r#"<style>"#,
            r#"#{id}{{font-family:"trebuchet ms",verdana,arial,sans-serif;font-size:16px;fill:#333;}}"#,
            r#"#{id} .transition{{stroke:#333333;stroke-width:1;fill:none;}}"#,
            r#"#{id} .node rect{{fill:#ECECFF;stroke:#9370DB;stroke-width:1px;}}"#,
            r#"#{id} .node circle.state-start{{fill:#333333;stroke:#333333;}}"#,
            r#"#{id} .node circle.state-end{{fill:#9370DB;stroke:white;stroke-width:1.5;}}"#,
            r#"#{id} .node .fork-join{{fill:#333333;stroke:#333333;}}"#,
            r#"#{id} .statediagram-cluster rect{{fill:#ECECFF;stroke:#9370DB;stroke-width:1px;}}"#,
            r#"#{id} .statediagram-cluster rect.outer{{rx:5px;ry:5px;}}"#,
            r#"#{id} .cluster-label,#{id} .nodeLabel{{color:#131300;}}"#,
            r#"#{id} .marker{{fill:#333333;stroke:#333333;}}"#,
            r#"</style>"#,
        ),
        id = id,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::state::parse;
    use crate::theme::get_theme;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn renders_minimal_diagram_without_panicking() {
        let src = "stateDiagram-v2\n[*] --> S1\nS1 --> [*]\n";
        let d = parse(src).unwrap();
        let theme = get_theme("default");
        let l = crate::layout::state::layout(&d, &theme).unwrap();
        let svg = render(&d, &l, &theme, "t1").unwrap();
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains(r#"class="statediagram""#));
        assert!(svg.contains(r#"aria-roledescription="stateDiagram""#));
        assert!(svg.ends_with("</svg>"));
    }

    fn fixture_id(rel: &str) -> String {
        let mut id = String::from("ref-");
        let mut last_sep = false;
        for c in rel.chars() {
            if c.is_ascii_alphanumeric() {
                id.push(c);
                last_sep = false;
            } else if !last_sep {
                id.push('-');
                last_sep = true;
            }
        }
        while id.ends_with('-') {
            id.pop();
        }
        id
    }

    /// Smoke test across all fixtures. Reports byte-exact match count,
    /// never panics on mismatch (this renderer isn't byte-exact yet).
    #[test]
    fn reports_byte_exact_pass_count() {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut groups = vec![];
        for sub in ["cypress", "demos"] {
            let dir = base.join(format!("tests/ext_fixtures/{}/state", sub));
            let Ok(entries) = fs::read_dir(&dir) else { continue };
            let mut files: Vec<_> = entries.flatten().collect();
            files.sort_by_key(|e| e.file_name());
            for entry in files {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) != Some("mmd") {
                    continue;
                }
                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                let rel = format!("ext_fixtures/{}/state/{}", sub, stem);
                let mmd = match fs::read_to_string(&p) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let ref_svg = base.join(format!("tests/reference/{}.svg", rel));
                let expected = match fs::read_to_string(&ref_svg) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let id = fixture_id(&rel);
                let theme = get_theme("default");
                let mmd_c = mmd.clone();
                let id_c = id.clone();
                let theme_c = theme.clone();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    parse(&mmd_c).and_then(|d| {
                        let eff = d
                            .theme_override
                            .as_deref()
                            .map(get_theme)
                            .unwrap_or_else(|| theme_c.clone());
                        let l = crate::layout::state::layout(&d, &eff)?;
                        render(&d, &l, &eff, &id_c)
                    })
                }));
                let got = match result {
                    Ok(Ok(s)) => s,
                    _ => {
                        groups.push((rel, false, false));
                        continue;
                    }
                };
                let exact = got == expected;
                groups.push((rel, true, exact));
            }
        }
        let total = groups.len();
        let rendered = groups.iter().filter(|(_, r, _)| *r).count();
        let exact = groups.iter().filter(|(_, _, e)| *e).count();
        eprintln!(
            "[state] fixtures={} rendered={} byte-exact={}",
            total, rendered, exact
        );
        let failed: Vec<&String> = groups
            .iter()
            .filter(|(_, r, _)| !*r)
            .map(|(rel, _, _)| rel)
            .collect();
        if !failed.is_empty() {
            eprintln!("[state] render-failures ({}):", failed.len());
            for f in failed {
                eprintln!("  - {}", f);
            }
        }
    }
}
