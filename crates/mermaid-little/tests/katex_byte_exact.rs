//! Verify that our embedded KaTeX (rquickjs + katex.min.js) produces the
//! same byte-for-byte output as the upstream npm `katex` package run under
//! Node.js. The baselines under `tests/katex_baselines/` were captured by:
//!
//! ```
//! cd tests/support
//! node -e 'process.stdout.write(require("katex").renderToString(<latex>, \
//!   { throwOnError:true, displayMode:true, output:"htmlAndMathml" }))'
//! ```
//!
//! Regenerate them only when bumping `src/katex/vendor/katex.min.js`.

#![cfg(feature = "katex")]

use mermaid_little::katex::render;

fn baseline(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/katex_baselines")
        .join(format!("{}.txt", name));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read baseline {:?}: {}", path, e))
}

#[test]
fn alpha_beta_gamma() {
    let rust = render(r"\alpha\beta\gamma", true).unwrap();
    assert_eq!(rust, baseline("alpha"));
}

#[test]
fn frac_a_b() {
    let rust = render(r"\frac{a}{b}", true).unwrap();
    assert_eq!(rust, baseline("frac"));
}

#[test]
fn overbrace_text() {
    let rust = render(r"\overbrace{a+b+c}^{\text{note}}", true).unwrap();
    assert_eq!(rust, baseline("overbrace"));
}

#[test]
fn cases_environment() {
    let rust = render(r"\begin{cases} a &\text{if } b \\ c &\text{if } d \end{cases}", true).unwrap();
    assert_eq!(rust, baseline("cases"));
}

#[test]
fn integral() {
    let rust = render(r"\int_{-\infty}^\infty \hat{f}(\xi)\,e^{2 \pi i \xi x}\,d\xi", true).unwrap();
    assert_eq!(rust, baseline("integral"));
}

/// End-to-end test: feeding the *raw* KaTeX label exactly as it appears in
/// the .mmd source must produce the same byte-stream that mermaid's
/// `renderKatexSanitized` writes into the reference SVG (after extracting
/// the `<span class="nodeLabel ">…</span>` body).
///
/// Source: `tests/ext_fixtures/demos/flowchart/44.mmd`, node A.
#[test]
fn end_to_end_lowercase_greek() {
    use mermaid_little::katex::render_label;
    let label =
        r"$$\alpha\beta\gamma\delta\epsilon\zeta\eta\theta\iota\kappa\lambda\mu\nu\xi\omicron\pi\rho\sigma\tau\upsilon\phi\chi\psi\omega$$";
    let rust = render_label(label).unwrap();

    // Extract the span body from the reference SVG so the test stays
    // anchored to the upstream fixture.
    let svg = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/reference/ext_fixtures/demos/flowchart/44.svg"
    ))
    .unwrap();
    let needle = r#"<span class="nodeLabel ">"#;
    let start = svg.find(needle).unwrap() + needle.len();
    let body = &svg[start..];
    let end = body.find("</span></div></foreignObject>").unwrap();
    let mermaid_body = &body[..end];

    diff_then_panic(&rust, mermaid_body);
}

fn diff_then_panic(rust: &str, mermaid: &str) {
    if rust == mermaid {
        return;
    }
    for (i, (a, b)) in rust.bytes().zip(mermaid.bytes()).enumerate() {
        if a != b {
            let lo = i.saturating_sub(50);
            let hi_a = (i + 100).min(rust.len());
            let hi_b = (i + 100).min(mermaid.len());
            panic!(
                "first diff at byte {} (rust_len={} mermaid_len={})\n  rust:    {:?}\n  mermaid: {:?}",
                i,
                rust.len(),
                mermaid.len(),
                &rust[lo..hi_a],
                &mermaid[lo..hi_b]
            );
        }
    }
    panic!("lengths differ: rust={} mermaid={}", rust.len(), mermaid.len());
}

fn extract_node_label_body(svg: &str, after_id_substring: &str) -> String {
    // Find the foreignObject after a node identified by `after_id_substring`
    // (e.g. "flowchart-A-0"), then extract the body of the inner span.
    let id_pos = svg
        .find(after_id_substring)
        .unwrap_or_else(|| panic!("substring {} not in svg", after_id_substring));
    let needle = r#"<span class="nodeLabel ">"#;
    let start = svg[id_pos..]
        .find(needle)
        .map(|p| id_pos + p + needle.len())
        .unwrap_or_else(|| panic!("nodeLabel span not found after {}", after_id_substring));
    let close = svg[start..]
        .find("</span></div></foreignObject>")
        .unwrap_or_else(|| panic!("nodeLabel close not found"));
    svg[start..start + close].to_owned()
}

#[test]
fn end_to_end_cases_environment_with_amp() {
    use mermaid_little::katex::render_label;
    // The .mmd source has literal `&` for cases-environment column splits;
    // by the time the label reaches our renderer it is XML-escaped to
    // `&amp;`. The KaTeX integration must decode `&amp;` -> `&` inside
    // `$$..$$` blocks before handing them to KaTeX.
    let label = r"$$x = \begin{cases} a &amp;\text{if } b \\ c &amp;\text{if } d \end{cases}$$";
    let rust = render_label(label).unwrap();

    let svg = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/reference/ext_fixtures/demos/flowchart/42.svg"
    ))
    .unwrap();
    let mermaid_body = extract_node_label_body(&svg, "flowchart-D-5");
    diff_then_panic(&rust, &mermaid_body);
}
