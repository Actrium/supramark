//! Renderer for Mermaid's built-in `info` diagram.
//!
//! Upstream registers this as a normal diagram type and renders only the
//! Mermaid version text. No parser/layout stage is needed.

use crate::error::Result;
use crate::theme::{css as theme_css, ThemeVariables};

pub fn render(theme: &ThemeVariables, id: &str) -> Result<String> {
    let version = env!("CARGO_PKG_VERSION")
        .split('-')
        .next()
        .unwrap_or(env!("CARGO_PKG_VERSION"));

    let mut out = String::with_capacity(4096);
    out.push_str(&format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" style="max-width: 400px;" role="graphics-document document" aria-roledescription="info">"#
    ));
    out.push_str("<style>");
    out.push_str(&theme_css::base_preamble(id, theme));
    out.push_str(&theme_css::neo_look_block(id, theme));
    out.push_str("</style>");
    out.push_str("<g></g><g><text x=\"100\" y=\"40\" class=\"version\" font-size=\"32\" style=\"text-anchor: middle;\">v");
    out.push_str(version);
    out.push_str("</text></g></svg>");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::get_theme;

    #[test]
    fn renders_version_shell() {
        let svg = render(&get_theme("default"), "ref-ext-fixtures-cypress-state-05").unwrap();
        assert!(svg.contains(r#"aria-roledescription="info""#));
        assert!(svg.contains(">v11.14.0</text>"));
    }
}
