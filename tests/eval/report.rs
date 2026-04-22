//! Report emitters for structural-diff results.
//!
//! Adapted from selkie (https://github.com/btucker/selkie), MIT license.
//! Originally `src/eval/report.rs`. Significant trimming: selkie's reporter
//! tracks SSIM averages, per-type statistics, and embeds comparison PNGs;
//! here we keep only text / JSON / HTML emission for a flat list of
//! per-fixture structural diffs, which is all mermaid-little needs today.

use std::fmt::Write;
use std::fs;
use std::io;
use std::path::Path;

use super::structural_diff::{Diff, Issue, Level};

/// A single fixture's evaluation entry.
#[derive(Debug, Clone)]
pub struct FixtureReport {
    /// Identifier, e.g. `"flowchart/01"`.
    pub name: String,
    /// Optional source-file path relative to the crate root.
    pub source: Option<String>,
    /// Optional detected diagram type (e.g. `"flowchart"`).
    pub diagram_type: Option<String>,
    /// Structural diff for this fixture.
    pub diff: Diff,
}

impl FixtureReport {
    pub fn new(name: impl Into<String>, diff: Diff) -> Self {
        Self {
            name: name.into(),
            source: None,
            diagram_type: None,
            diff,
        }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_type(mut self, diagram_type: impl Into<String>) -> Self {
        self.diagram_type = Some(diagram_type.into());
        self
    }
}

/// Aggregate of multiple fixture reports.
#[derive(Debug, Clone, Default)]
pub struct EvalReport {
    pub fixtures: Vec<FixtureReport>,
}

impl EvalReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, fixture: FixtureReport) {
        self.fixtures.push(fixture);
    }

    pub fn total(&self) -> usize {
        self.fixtures.len()
    }

    pub fn matching(&self) -> usize {
        self.fixtures.iter().filter(|f| f.diff.is_empty()).count()
    }

    pub fn with_errors(&self) -> usize {
        self.fixtures.iter().filter(|f| f.diff.has_errors()).count()
    }

    pub fn with_warnings(&self) -> usize {
        self.fixtures
            .iter()
            .filter(|f| !f.diff.has_errors() && f.diff.has_warnings())
            .count()
    }

    pub fn parity_pct(&self) -> f64 {
        if self.fixtures.is_empty() {
            0.0
        } else {
            100.0 * self.matching() as f64 / self.fixtures.len() as f64
        }
    }

    // -- Text ---------------------------------------------------------------

    /// Plain-text summary suitable for terminal output.
    pub fn text_summary(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "mermaid-little Evaluation Report");
        let _ = writeln!(out, "================================");
        let _ = writeln!(
            out,
            "Parity: {:.1}% ({} / {} fixtures match reference)",
            self.parity_pct(),
            self.matching(),
            self.total()
        );
        let _ = writeln!(out, "Errors:   {} fixtures", self.with_errors());
        let _ = writeln!(
            out,
            "Warnings: {} fixtures (no errors)",
            self.with_warnings()
        );
        out.push('\n');

        for fixture in &self.fixtures {
            if fixture.diff.is_empty() {
                continue;
            }
            let _ = writeln!(out, "--- {} ---", fixture.name);
            for issue in &fixture.diff.issues {
                let _ = write!(
                    out,
                    "  [{}] {}: {}",
                    issue.level, issue.check, issue.message
                );
                if let (Some(e), Some(a)) = (&issue.expected, &issue.actual) {
                    let _ = write!(out, " (expected {}, actual {})", e, a);
                }
                out.push('\n');
            }
        }

        out
    }

    // -- JSON --------------------------------------------------------------

    /// Machine-readable JSON report. Hand-rolled to avoid pulling in
    /// `serde_json` as a dev-dep.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push('{');
        out.push_str(&format!("\"total\":{},", self.total()));
        out.push_str(&format!("\"matching\":{},", self.matching()));
        out.push_str(&format!("\"errors\":{},", self.with_errors()));
        out.push_str(&format!("\"warnings\":{},", self.with_warnings()));
        out.push_str(&format!("\"parity_pct\":{:.3},", self.parity_pct()));
        out.push_str("\"fixtures\":[");
        for (i, fx) in self.fixtures.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&fixture_to_json(fx));
        }
        out.push_str("]}");
        out
    }

    // -- HTML --------------------------------------------------------------

    /// Self-contained HTML summary.
    pub fn to_html(&self) -> String {
        let mut out = String::new();
        out.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
        out.push_str("<title>mermaid-little eval report</title>");
        out.push_str("<style>");
        out.push_str(
            "body{font-family:system-ui,-apple-system,Segoe UI,sans-serif;margin:2rem;color:#222}\
             h1{margin-bottom:0.25rem}\
             .summary{margin:0.5rem 0 1.5rem;font-size:0.95rem;color:#555}\
             table{border-collapse:collapse;width:100%;font-size:0.9rem}\
             th,td{border-bottom:1px solid #ddd;padding:0.35rem 0.5rem;text-align:left;vertical-align:top}\
             tr.status-match td{background:#f2fbf2}\
             tr.status-warn td{background:#fff8e1}\
             tr.status-error td{background:#fdecec}\
             .lvl-ERROR{color:#b11}\
             .lvl-WARN{color:#a60}\
             .lvl-INFO{color:#555}\
             code{background:#f4f4f4;padding:0 3px;border-radius:2px}\
             details summary{cursor:pointer}",
        );
        out.push_str("</style></head><body>");

        out.push_str("<h1>mermaid-little evaluation</h1>");
        out.push_str(&format!(
            "<div class=\"summary\">Parity <strong>{:.1}%</strong> \
             &nbsp;|&nbsp; {} matching &nbsp;|&nbsp; {} errors &nbsp;|&nbsp; {} warnings \
             &nbsp;|&nbsp; total {}</div>",
            self.parity_pct(),
            self.matching(),
            self.with_errors(),
            self.with_warnings(),
            self.total()
        ));

        out.push_str("<table><thead><tr><th>Fixture</th><th>Type</th><th>Status</th><th>Issues</th></tr></thead><tbody>");
        for fx in &self.fixtures {
            let (status_class, status_label) = if fx.diff.has_errors() {
                ("status-error", "ERROR")
            } else if fx.diff.has_warnings() {
                ("status-warn", "WARN")
            } else {
                ("status-match", "MATCH")
            };
            out.push_str(&format!("<tr class=\"{}\">", status_class));
            out.push_str(&format!("<td>{}</td>", html_escape(&fx.name)));
            out.push_str(&format!(
                "<td>{}</td>",
                html_escape(fx.diagram_type.as_deref().unwrap_or(""))
            ));
            out.push_str(&format!("<td>{}</td>", status_label));
            out.push_str("<td>");
            if fx.diff.is_empty() {
                out.push_str("&mdash;");
            } else {
                out.push_str(&format!(
                    "<details><summary>{} issue{}</summary><ul>",
                    fx.diff.issues.len(),
                    if fx.diff.issues.len() == 1 { "" } else { "s" }
                ));
                for issue in &fx.diff.issues {
                    out.push_str(&issue_to_html_li(issue));
                }
                out.push_str("</ul></details>");
            }
            out.push_str("</td></tr>");
        }
        out.push_str("</tbody></table>");
        out.push_str("</body></html>");
        out
    }

    /// Write all three formats into `dir` (`report.txt`, `report.json`,
    /// `report.html`). Creates the directory if missing.
    pub fn write_all(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        fs::write(dir.join("report.txt"), self.text_summary())?;
        fs::write(dir.join("report.json"), self.to_json())?;
        fs::write(dir.join("report.html"), self.to_html())?;
        Ok(())
    }
}

// -- Helpers ---------------------------------------------------------------

fn fixture_to_json(fx: &FixtureReport) -> String {
    let mut out = String::new();
    out.push('{');
    out.push_str(&format!("\"name\":{}", json_string(&fx.name)));
    if let Some(src) = &fx.source {
        out.push_str(&format!(",\"source\":{}", json_string(src)));
    }
    if let Some(t) = &fx.diagram_type {
        out.push_str(&format!(",\"diagram_type\":{}", json_string(t)));
    }
    let status = if fx.diff.has_errors() {
        "error"
    } else if fx.diff.has_warnings() {
        "warning"
    } else {
        "match"
    };
    out.push_str(&format!(",\"status\":\"{}\"", status));
    out.push_str(",\"issues\":[");
    for (i, issue) in fx.diff.issues.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&issue_to_json(issue));
    }
    out.push_str("]}");
    out
}

fn issue_to_json(i: &Issue) -> String {
    let level = match i.level {
        Level::Error => "error",
        Level::Warning => "warning",
        Level::Info => "info",
    };
    let mut out = format!(
        "{{\"level\":\"{}\",\"check\":{},\"message\":{}",
        level,
        json_string(&i.check),
        json_string(&i.message)
    );
    if let Some(e) = &i.expected {
        out.push_str(&format!(",\"expected\":{}", json_string(e)));
    }
    if let Some(a) = &i.actual {
        out.push_str(&format!(",\"actual\":{}", json_string(a)));
    }
    out.push('}');
    out
}

fn issue_to_html_li(i: &Issue) -> String {
    let extras = match (&i.expected, &i.actual) {
        (Some(e), Some(a)) => format!(
            " <code>expected {} / actual {}</code>",
            html_escape(e),
            html_escape(a)
        ),
        _ => String::new(),
    };
    format!(
        "<li><span class=\"lvl-{}\">[{}]</span> <strong>{}</strong>: {}{}</li>",
        i.level,
        i.level,
        html_escape(&i.check),
        html_escape(&i.message),
        extras
    )
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::structural_diff;
    use super::*;

    #[test]
    fn empty_report_renders_cleanly() {
        let r = EvalReport::new();
        assert!(r.text_summary().contains("Parity: 0.0%"));
        assert!(r.to_json().contains("\"total\":0"));
        assert!(r.to_html().contains("<table"));
    }

    #[test]
    fn report_captures_issues() {
        let a = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50">
            <g class="node"><rect x="0" y="0" width="10" height="10"/><text>A</text></g>
        </svg>"##;
        let b = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50">
            <g class="node"><rect x="0" y="0" width="10" height="10"/><text>A</text></g>
            <g class="node"><rect x="0" y="0" width="10" height="10"/><text>B</text></g>
        </svg>"##;
        let diff = structural_diff::compare(a, b).unwrap();
        let mut r = EvalReport::new();
        r.push(FixtureReport::new("fx/1", diff).with_type("flowchart"));
        assert_eq!(r.total(), 1);
        assert_eq!(r.with_errors(), 1);
        let json = r.to_json();
        assert!(json.contains("\"status\":\"error\""));
        assert!(json.contains("node_count"));
        let html = r.to_html();
        assert!(html.contains("status-error"));
    }
}
