//! d2-latex: LaTeX → SVG via a pluggable JS engine running MathJax.
//!
//! The crate ships the MathJax bundle (`MATHJAX_JS` + `SETUP_JS`) but does
//! **not** bundle a JS runtime.  Embedders inject a renderer via
//! [`set_engine`]:
//!
//! - Web/wasm: bridge to the browser's JS engine via wasm-bindgen, so
//!   d2-little doesn't pay a second JS-engine bloat on top of V8.
//! - React Native: bridge to Hermes/JSC via JSI.
//! - Native: link a small in-process engine (e.g. rquickjs); see
//!   `tests/common/latex_engine.rs` for a reference implementation that
//!   reuses one runtime per thread.
//!
//! Without an engine installed, [`render`] / [`measure`] return an error and
//! the higher-level renderer leaves the LaTeX block unrendered (callers in
//! `svg_render` use `if let Ok(...)` to silently degrade).

use std::sync::OnceLock;

/// Pixels per ex unit (matches Go d2latex.pxPerEx = 8).
const PX_PER_EX: i32 = 8;

/// MathJax bundle (custom build, ~1.8MB).  Public so engine impls can load
/// it once into their JS runtime; identical to Go d2's `mathjax.js`.
pub static MATHJAX_JS: &str = include_str!("../mathjax.js");

/// MathJax `liteAdaptor` setup; assumes `MATHJAX_JS` has been evaluated and
/// installs `adaptor` + `html` on the JS global.
pub static SETUP_JS: &str = include_str!("../setup.js");

/// Pluggable engine signature.  Receives the raw LaTeX as written by the
/// user; the implementation is expected to feed it into the embedded
/// MathJax bundle and return the SVG markup.
type EngineFn = Box<dyn Fn(&str) -> Result<String, String> + Send + Sync>;

static ENGINE: OnceLock<EngineFn> = OnceLock::new();

/// Install the LaTeX rendering engine.  Idempotent: only the first
/// installation takes effect (subsequent calls are silently dropped, so
/// libraries can call this defensively without checking).
///
/// The closure must be `Send + Sync` because `OnceLock` may surface it from
/// any thread.  If your underlying JS runtime is `!Send` (rquickjs, etc.)
/// hide it behind a `thread_local!` inside the closure — see
/// `tests/common/latex_engine.rs`.
pub fn set_engine<F>(f: F)
where
    F: Fn(&str) -> Result<String, String> + Send + Sync + 'static,
{
    let _ = ENGINE.set(Box::new(f));
}

/// Convenience for engine impls: prepare a `html.convert(...)` JS snippet
/// for the given LaTeX, applying the same backslash-doubling and
/// template-literal escaping the Go reference uses.  Engines typically eval
/// this and return the resulting string.
pub fn build_convert_expr(latex: &str) -> String {
    let doubled = double_backslashes(latex);
    let escaped = escape_template_literal(&doubled);
    format!(
        "adaptor.innerHTML(html.convert(`{}`, {{ em: {}, ex: {} }}))",
        escaped,
        PX_PER_EX * 2,
        PX_PER_EX
    )
}

/// Render a LaTeX string to SVG markup.  Returns an error if no engine is
/// installed; production embedders are responsible for calling
/// [`set_engine`] during initialization.
pub fn render(latex: &str) -> Result<String, String> {
    let Some(engine) = ENGINE.get() else {
        return Err(
            "d2-latex: no engine installed. Call latex::set_engine() with a function \
             that evaluates the embedded MATHJAX_JS + SETUP_JS in a JS runtime and \
             evaluates latex::build_convert_expr(latex) to obtain the SVG."
                .into(),
        );
    };
    engine(latex)
}

/// Measure a LaTeX string, returning (width, height) in pixels.
///
/// Mirrors Go `d2latex.Measure`: renders the LaTeX, parses the SVG dimensions
/// (in ex units), and converts to pixels.
pub fn measure(latex: &str) -> Result<(i32, i32), String> {
    let svg = render(latex)?;

    // Parse width/height from the SVG: width="Xex" height="Yex"
    let w_ex = extract_ex_dim(&svg, "width")?;
    let h_ex = extract_ex_dim(&svg, "height")?;

    let w = (w_ex * PX_PER_EX as f64).ceil() as i32;
    let h = (h_ex * PX_PER_EX as f64).ceil() as i32;
    Ok((w, h))
}

/// Double backslashes in the LaTeX string (mirrors Go doubleBackslashes).
fn double_backslashes(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        if c == '\\' {
            result.push_str("\\\\");
        } else {
            result.push(c);
        }
    }
    result
}

/// Escape a string for embedding in a JS template literal (backtick string).
/// Note: backslashes are NOT escaped here because `double_backslashes` already
/// doubled them for MathJax. We only escape template literal metacharacters.
fn escape_template_literal(s: &str) -> String {
    s.replace('`', "\\`").replace('$', "\\$")
}

/// Extract a dimension in `ex` units from an SVG tag attribute.
fn extract_ex_dim(svg: &str, attr: &str) -> Result<f64, String> {
    let pattern = format!(r#"{}=""#, attr);
    let start = svg
        .find(&pattern)
        .ok_or_else(|| format!("missing {} in SVG", attr))?
        + pattern.len();
    let end = svg[start..]
        .find("ex\"")
        .ok_or_else(|| format!("missing ex unit for {} in SVG", attr))?
        + start;
    svg[start..end]
        .parse::<f64>()
        .map_err(|e| format!("failed to parse {}: {}", attr, e))
}
