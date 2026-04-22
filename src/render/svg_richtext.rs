//! Rich-text → SVG `<tspan>` emitter.
//!
//! Consumes a `Vec<TextSpan>` (produced by
//! [`crate::parser::richtext`]) and emits the inner contents of a
//! `<text>` element — a sequence of `<tspan>` nodes carrying styling
//! attributes.
//!
//! Upstream mermaid routes labels through a DOM tree; we translate
//! directly to the final byte string matching that tree's serialised
//! form. The `<text>` wrapper (with its `x`/`y`/`text-anchor`
//! attributes) is emitted by the per-diagram renderer; this module
//! only owns what goes inside.

use crate::model::richtext::TextSpan;

/// Emit the inner content of a `<text>` element as tspans.
///
/// `line_height` and `line_count` context are the caller's concern —
/// this function emits each line as a `<tspan x="..." dy="...">`
/// only when a `HardBreak` is encountered.
pub fn emit(spans: &[TextSpan], base_x: f64) -> String {
    let mut buf = String::new();
    let ctx = Ctx::default();
    emit_spans(spans, &mut buf, &ctx, base_x, &mut 0);
    buf
}

#[derive(Debug, Default, Clone)]
struct Ctx {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    color: Option<String>,
    bg: Option<String>,
    font_family: Option<String>,
    size: Option<f64>,
    baseline_shift: Option<&'static str>,
}

impl Ctx {
    fn style_attr(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.bold {
            parts.push("font-weight:bold".into());
        }
        if self.italic {
            parts.push("font-style:italic".into());
        }
        let deco: Vec<&str> = [
            self.underline.then_some("underline"),
            self.strike.then_some("line-through"),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !deco.is_empty() {
            parts.push(format!("text-decoration:{}", deco.join(" ")));
        }
        if let Some(c) = &self.color {
            parts.push(format!("fill:{}", c));
        }
        if let Some(f) = &self.font_family {
            parts.push(format!("font-family:{}", f));
        }
        if let Some(s) = self.size {
            parts.push(format!("font-size:{}px", s));
        }
        parts.join(";")
    }
}

fn emit_spans(spans: &[TextSpan], buf: &mut String, ctx: &Ctx, base_x: f64, line_idx: &mut usize) {
    for span in spans {
        emit_span(span, buf, ctx, base_x, line_idx);
    }
}

fn emit_span(span: &TextSpan, buf: &mut String, ctx: &Ctx, base_x: f64, line_idx: &mut usize) {
    match span {
        TextSpan::Plain(s) => push_tspan(buf, s, ctx, None),
        TextSpan::Monospace(s) => {
            let mut c = ctx.clone();
            c.font_family = Some("monospace".into());
            push_tspan(buf, s, &c, None);
        }
        TextSpan::HardBreak => {
            *line_idx += 1;
            // Subsequent plain text emits a new tspan that jumps a
            // line. The per-diagram renderer uses `em` units by
            // default to match upstream.
            buf.push_str(&format!(
                r#"<tspan x="{}" dy="1em"></tspan>"#,
                fmt_num(base_x)
            ));
        }
        TextSpan::Bold(inner) => {
            let mut c = ctx.clone();
            c.bold = true;
            emit_spans(inner, buf, &c, base_x, line_idx);
        }
        TextSpan::Italic(inner) => {
            let mut c = ctx.clone();
            c.italic = true;
            emit_spans(inner, buf, &c, base_x, line_idx);
        }
        TextSpan::Underline(inner) => {
            let mut c = ctx.clone();
            c.underline = true;
            emit_spans(inner, buf, &c, base_x, line_idx);
        }
        TextSpan::Strikethrough(inner) => {
            let mut c = ctx.clone();
            c.strike = true;
            emit_spans(inner, buf, &c, base_x, line_idx);
        }
        TextSpan::Colored { color, content } => {
            let mut c = ctx.clone();
            c.color = Some(color.clone());
            emit_spans(content, buf, &c, base_x, line_idx);
        }
        TextSpan::BackHighlight { color, content } => {
            let mut c = ctx.clone();
            c.bg = Some(color.clone());
            // TODO: SVG has no native background highlight on text;
            // upstream emits a `<rect>` behind the run. Wave 3 lands this.
            emit_spans(content, buf, &c, base_x, line_idx);
        }
        TextSpan::Sized { size, content } => {
            let mut c = ctx.clone();
            c.size = Some(*size);
            emit_spans(content, buf, &c, base_x, line_idx);
        }
        TextSpan::Subscript(inner) => {
            let mut c = ctx.clone();
            c.baseline_shift = Some("sub");
            emit_spans(inner, buf, &c, base_x, line_idx);
        }
        TextSpan::Superscript(inner) => {
            let mut c = ctx.clone();
            c.baseline_shift = Some("super");
            emit_spans(inner, buf, &c, base_x, line_idx);
        }
        TextSpan::FontFamily { family, content } => {
            let mut c = ctx.clone();
            c.font_family = Some(family.clone());
            emit_spans(content, buf, &c, base_x, line_idx);
        }
        TextSpan::Link { url, label, .. } => {
            let text = label.as_deref().unwrap_or(url);
            buf.push_str(&format!(
                r#"<a href="{}"><tspan>{}</tspan></a>"#,
                escape_attr(url),
                escape_text(text)
            ));
        }
    }
}

fn push_tspan(buf: &mut String, text: &str, ctx: &Ctx, dy: Option<&str>) {
    let style = ctx.style_attr();
    let style_attr = if style.is_empty() {
        String::new()
    } else {
        format!(r#" style="{}""#, style)
    };
    let dy_attr = dy.map(|s| format!(r#" dy="{}""#, s)).unwrap_or_default();
    let shift_attr = ctx
        .baseline_shift
        .map(|s| format!(r#" baseline-shift="{}""#, s))
        .unwrap_or_default();
    buf.push_str(&format!(
        r#"<tspan{}{}{}>{}</tspan>"#,
        style_attr,
        dy_attr,
        shift_attr,
        escape_text(text)
    ));
}

fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::richtext::parse;

    #[test]
    fn plain_to_tspan() {
        let spans = parse("hello");
        assert_eq!(emit(&spans, 0.0), "<tspan>hello</tspan>");
    }

    #[test]
    fn bold_adds_font_weight() {
        let spans = parse("<b>x</b>");
        assert_eq!(
            emit(&spans, 0.0),
            r#"<tspan style="font-weight:bold">x</tspan>"#
        );
    }

    #[test]
    fn escapes_xml_special() {
        let spans = parse("a & <c");
        assert!(emit(&spans, 0.0).contains("a &amp; &lt;c"));
    }

    #[test]
    fn hardbreak_emits_dy_tspan() {
        let spans = parse("a<br>b");
        let out = emit(&spans, 5.0);
        assert!(out.contains(r#"<tspan x="5" dy="1em"></tspan>"#));
    }
}
