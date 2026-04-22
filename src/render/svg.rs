//! Root `<svg>` scaffold + `<defs>` section.
//!
//! Consumes a [`crate::layout::DiagramLayout`] and a
//! [`crate::theme::ThemeVariables`], emits the surrounding SVG
//! element matching upstream mermaid's attribute order.
//!
//! Upstream reference: `packages/mermaid/src/mermaidAPI.ts:render`
//! — produces `<svg id=... width="100%" xmlns=... viewBox=...
//! style="max-width: Npx;" role="graphics-document document"
//! aria-roledescription="<kind>">`.

use crate::layout::DiagramLayout;
use crate::theme::ThemeVariables;

/// SVG viewport.
#[derive(Debug, Clone, Copy, Default)]
pub struct ViewBox {
    pub min_x: f64,
    pub min_y: f64,
    pub width: f64,
    pub height: f64,
}

impl ViewBox {
    pub fn to_attr(&self) -> String {
        // mermaid's formatting: no superfluous trailing zeroes, space-separated
        format!("{} {} {} {}", fmt(self.min_x), fmt(self.min_y), fmt(self.width), fmt(self.height))
    }
}

fn fmt(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

/// Build the opening `<svg>` tag matching upstream's attribute order.
///
/// Arguments are supplied by the per-diagram renderer after layout.
pub fn open_svg(id: &str, kind: &str, viewbox: ViewBox) -> String {
    format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" viewBox="{vb}" style="max-width: {w}px;" role="graphics-document document" aria-roledescription="{kind}">"#,
        id = id,
        vb = viewbox.to_attr(),
        w = fmt(viewbox.width),
        kind = kind,
    )
}

/// Build the closing `</svg>` tag.
pub const fn close_svg() -> &'static str {
    "</svg>"
}

/// Emit the `<style>` block that mermaid inserts before the drawing
/// content. Takes a pre-composed CSS string (per-diagram styles.rs
/// produces this from [`ThemeVariables`]).
pub fn style_block(id: &str, css: &str) -> String {
    format!(r#"<style>{css}</style>"#, css = prefix_selectors(id, css))
}

/// Prefix every CSS selector in `css` with `#<id>` so styles scope
/// to the single `<svg>`. Upstream mermaid does this via `stylis`;
/// our substitute is a simple rewrite since our CSS is generated.
fn prefix_selectors(_id: &str, css: &str) -> String {
    // TODO: implement real scoping once a diagram actually emits CSS.
    // Placeholder returns the input unchanged; the Wave 1 pie port
    // will be the first caller and shape this function's contract.
    css.to_string()
}

/// Entry point — dispatch on the layout variant.
///
/// Currently every variant returns `Err(Unsupported)` because no
/// per-diagram renderer is implemented yet. Each Wave 1+ milestone
/// fills in one arm.
pub fn render(layout: &DiagramLayout, _theme: &ThemeVariables) -> crate::error::Result<String> {
    Err(crate::error::MermaidError::Unsupported(format!(
        "render(DiagramLayout::{:?}) not yet implemented",
        layout_name(layout),
    )))
}

fn layout_name(layout: &DiagramLayout) -> &'static str {
    match layout {
        DiagramLayout::Pie(_) => "Pie",
        DiagramLayout::Packet(_) => "Packet",
        DiagramLayout::Radar(_) => "Radar",
        DiagramLayout::Ishikawa(_) => "Ishikawa",
        DiagramLayout::Journey(_) => "Journey",
        DiagramLayout::Timeline(_) => "Timeline",
        DiagramLayout::Quadrant(_) => "Quadrant",
        DiagramLayout::Xychart(_) => "Xychart",
        DiagramLayout::Wardley(_) => "Wardley",
        DiagramLayout::Gantt(_) => "Gantt",
        DiagramLayout::Sankey(_) => "Sankey",
        DiagramLayout::Treemap(_) => "Treemap",
        DiagramLayout::Kanban(_) => "Kanban",
        DiagramLayout::Er(_) => "Er",
        DiagramLayout::Requirement(_) => "Requirement",
        DiagramLayout::Class(_) => "Class",
        DiagramLayout::State(_) => "State",
        DiagramLayout::Flowchart(_) => "Flowchart",
        DiagramLayout::Block(_) => "Block",
        DiagramLayout::Mindmap(_) => "Mindmap",
        DiagramLayout::Sequence(_) => "Sequence",
        DiagramLayout::C4(_) => "C4",
        DiagramLayout::GitGraph(_) => "GitGraph",
        DiagramLayout::Architecture(_) => "Architecture",
        DiagramLayout::Venn(_) => "Venn",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_integers_as_no_fraction() {
        assert_eq!(fmt(5.0), "5");
        assert_eq!(fmt(-5.0), "-5");
        assert_eq!(fmt(0.0), "0");
    }

    #[test]
    fn fmt_keeps_fraction() {
        assert_eq!(fmt(5.5), "5.5");
    }

    #[test]
    fn open_svg_matches_upstream_attr_order() {
        let vb = ViewBox { min_x: -38.0, min_y: -23.0, width: 76.0, height: 47.296875 };
        let got = open_svg("ref-x", "pie", vb);
        assert!(got.starts_with(r#"<svg id="ref-x" width="100%" xmlns="http://www.w3.org/2000/svg" viewBox="-38 -23 76 47.296875""#));
        assert!(got.contains(r#"aria-roledescription="pie""#));
    }
}
