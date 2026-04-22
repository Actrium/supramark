//! Structural diff between two SVGs (general primitives only).
//!
//! Adapted from selkie (https://github.com/btucker/selkie), MIT license.
//! Originally `src/eval/checks.rs` (+ `src/render/svg/structure.rs`).
//! Significant trimming and adaptation to mermaid-little's fixture layout
//! and feature set — diagram-specific checks (timeline, architecture, ER,
//! mindmap, etc.) and CSS-aware stroke/visibility analysis are stripped.
//! Retained: node/edge counts, labels, dimensions, shape buckets, z-order
//! summary, marker count, color buckets.

use roxmltree::{Document, Node};
use std::collections::HashSet;
use std::fmt;

// -- Public data model -------------------------------------------------------

/// Severity of a single structural discrepancy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Structural break — diagram is functionally wrong.
    Error,
    /// Significant difference — diagram may look noticeably different.
    Warning,
    /// Acceptable variation — likely intentional implementation difference.
    Info,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Level::Error => "ERROR",
            Level::Warning => "WARN",
            Level::Info => "INFO",
        };
        f.write_str(s)
    }
}

/// A single structural discrepancy between candidate and reference.
#[derive(Debug, Clone)]
pub struct Issue {
    pub level: Level,
    /// Short machine-friendly tag, e.g. `"node_count"`, `"dimensions"`.
    pub check: String,
    /// Human-readable description.
    pub message: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
}

impl Issue {
    pub fn error(check: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: Level::Error,
            check: check.into(),
            message: message.into(),
            expected: None,
            actual: None,
        }
    }

    pub fn warning(check: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: Level::Warning,
            check: check.into(),
            message: message.into(),
            expected: None,
            actual: None,
        }
    }

    pub fn info(check: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: Level::Info,
            check: check.into(),
            message: message.into(),
            expected: None,
            actual: None,
        }
    }

    pub fn with_values(mut self, expected: impl Into<String>, actual: impl Into<String>) -> Self {
        self.expected = Some(expected.into());
        self.actual = Some(actual.into());
        self
    }
}

/// Result of a structural comparison.
#[derive(Debug, Clone, Default)]
pub struct Diff {
    pub issues: Vec<Issue>,
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.issues.is_empty()
    }

    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|i| i.level == Level::Error)
    }

    pub fn has_warnings(&self) -> bool {
        self.issues.iter().any(|i| i.level == Level::Warning)
    }

    pub fn errors(&self) -> impl Iterator<Item = &Issue> {
        self.issues.iter().filter(|i| i.level == Level::Error)
    }

    /// A one-issue-per-line, human-readable report.
    pub fn report_text(&self) -> String {
        if self.issues.is_empty() {
            return "(no structural differences)".to_string();
        }
        let mut out = String::new();
        for issue in &self.issues {
            out.push_str(&format!(
                "[{}] {}: {}",
                issue.level, issue.check, issue.message
            ));
            if let (Some(exp), Some(act)) = (&issue.expected, &issue.actual) {
                out.push_str(&format!(" (expected: {}, actual: {})", exp, act));
            }
            out.push('\n');
        }
        out
    }
}

// -- Config ------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CheckConfig {
    /// Dimension difference threshold for warnings (0.2 = 20%).
    pub dimension_warning_threshold: f64,
    /// Dimension difference threshold for info (0.05 = 5%).
    pub dimension_info_threshold: f64,
    /// Shape-count percentage difference beyond which we emit a warning.
    pub shape_count_warning_pct: f64,
    /// Fill-color overlap percentage below which we emit a warning.
    pub color_warning_match_pct: f64,
}

impl Default for CheckConfig {
    fn default() -> Self {
        Self {
            dimension_warning_threshold: 0.20,
            dimension_info_threshold: 0.05,
            shape_count_warning_pct: 20.0,
            color_warning_match_pct: 50.0,
        }
    }
}

// -- Parsed SVG structure ----------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShapeCounts {
    pub rect: usize,
    pub circle: usize,
    pub ellipse: usize,
    pub polygon: usize,
    pub path: usize,
    pub line: usize,
    pub polyline: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZOrderSummary {
    /// Text rendered before shapes inside the same <g> (possibly obscured).
    pub text_before_shapes: usize,
    /// Text rendered after shapes inside the same <g> (correct order).
    pub text_after_shapes: usize,
    /// Labels that may be obscured.
    pub potentially_obscured_labels: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ColorSummary {
    /// Normalized fill colors (lowercase, deduped, sorted).
    pub fill_colors: Vec<String>,
    /// Normalized stroke colors (lowercase, deduped, sorted).
    pub stroke_colors: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SvgStructure {
    pub width: f64,
    pub height: f64,
    pub node_count: usize,
    pub edge_count: usize,
    pub labels: Vec<String>,
    pub shapes: ShapeCounts,
    pub marker_count: usize,
    pub has_defs: bool,
    pub has_style: bool,
    pub z_order: ZOrderSummary,
    pub colors: ColorSummary,
}

/// Node class patterns used by mermaid.js and mermaid-little.
const NODE_CLASSES: &[&str] = &[
    "node",
    "flowchart-node",
    "class-node",
    "state-node",
    "entity-node",
];

/// Edge class patterns.
const EDGE_CLASSES: &[&str] = &["edge", "relation", "transition", "relationship"];

impl SvgStructure {
    /// Parse an SVG string and extract its structure.
    pub fn from_svg(svg: &str) -> Result<Self, String> {
        let doc =
            Document::parse(svg).map_err(|e| format!("Failed to parse SVG: {}", e))?;

        let root = doc.root_element();
        if root.tag_name().name() != "svg" {
            return Err("Root element is not <svg>".into());
        }

        let (width, height) = parse_dimensions(&root);
        let shapes = count_shapes(&doc);
        let (node_count, edge_count) = count_nodes_and_edges(&doc);
        let labels = extract_labels(&doc);
        let marker_count = count_elements(&doc, "marker");
        let has_defs = doc.descendants().any(|n| n.tag_name().name() == "defs");
        let has_style = doc.descendants().any(|n| n.tag_name().name() == "style");
        let z_order = analyze_z_order(&doc);
        let colors = analyze_colors(&doc);

        Ok(SvgStructure {
            width,
            height,
            node_count,
            edge_count,
            labels,
            shapes,
            marker_count,
            has_defs,
            has_style,
            z_order,
            colors,
        })
    }
}

// -- Public entry points -----------------------------------------------------

/// Compare a candidate SVG against a reference SVG with default thresholds.
pub fn compare(candidate: &str, reference: &str) -> Result<Diff, String> {
    compare_with_config(candidate, reference, &CheckConfig::default())
}

/// Compare a candidate SVG against a reference SVG with custom thresholds.
pub fn compare_with_config(
    candidate: &str,
    reference: &str,
    config: &CheckConfig,
) -> Result<Diff, String> {
    let cand = SvgStructure::from_svg(candidate)?;
    let refer = SvgStructure::from_svg(reference)?;
    Ok(compare_structures(&cand, &refer, config))
}

/// Compare already-parsed structures.
pub fn compare_structures(
    candidate: &SvgStructure,
    reference: &SvgStructure,
    config: &CheckConfig,
) -> Diff {
    let mut issues = Vec::new();

    // Errors — structural breaks.
    check_node_count(candidate, reference, &mut issues);
    check_edge_count(candidate, reference, &mut issues);
    check_missing_labels(candidate, reference, &mut issues);

    // Warnings — notable differences.
    check_dimensions(candidate, reference, config, &mut issues);
    check_shape_counts(candidate, reference, config, &mut issues);
    check_z_order(candidate, reference, &mut issues);
    check_colors(candidate, reference, config, &mut issues);

    // Info — acceptable variations.
    check_extra_labels(candidate, reference, &mut issues);
    check_markers(candidate, reference, &mut issues);

    Diff { issues }
}

// -- Checks ------------------------------------------------------------------

fn check_node_count(c: &SvgStructure, r: &SvgStructure, out: &mut Vec<Issue>) {
    if c.node_count != r.node_count {
        out.push(
            Issue::error(
                "node_count",
                format!(
                    "Node count mismatch: expected {}, got {}",
                    r.node_count, c.node_count
                ),
            )
            .with_values(r.node_count.to_string(), c.node_count.to_string()),
        );
    }
}

fn check_edge_count(c: &SvgStructure, r: &SvgStructure, out: &mut Vec<Issue>) {
    if c.edge_count != r.edge_count {
        out.push(
            Issue::error(
                "edge_count",
                format!(
                    "Edge count mismatch: expected {}, got {}",
                    r.edge_count, c.edge_count
                ),
            )
            .with_values(r.edge_count.to_string(), c.edge_count.to_string()),
        );
    }
}

fn check_missing_labels(c: &SvgStructure, r: &SvgStructure, out: &mut Vec<Issue>) {
    let cand: HashSet<_> = c.labels.iter().collect();
    let refer: HashSet<_> = r.labels.iter().collect();
    let missing: Vec<_> = refer.difference(&cand).cloned().collect();
    if !missing.is_empty() {
        out.push(
            Issue::error("labels_missing", format!("Missing labels: {:?}", missing))
                .with_values(format!("{:?}", r.labels), format!("{:?}", c.labels)),
        );
    }
}

fn check_extra_labels(c: &SvgStructure, r: &SvgStructure, out: &mut Vec<Issue>) {
    let cand: HashSet<_> = c.labels.iter().collect();
    let refer: HashSet<_> = r.labels.iter().collect();
    let extra: Vec<_> = cand.difference(&refer).cloned().collect();
    if !extra.is_empty() {
        out.push(Issue::info(
            "labels_extra",
            format!("Extra labels in candidate: {:?}", extra),
        ));
    }
}

fn check_dimensions(
    c: &SvgStructure,
    r: &SvgStructure,
    config: &CheckConfig,
    out: &mut Vec<Issue>,
) {
    let width_diff = if r.width > 0.0 {
        (c.width - r.width).abs() / r.width
    } else {
        0.0
    };
    if width_diff > config.dimension_warning_threshold {
        out.push(
            Issue::warning(
                "dimensions",
                format!(
                    "Width differs by {:.0}%: expected {:.0}, got {:.0}",
                    width_diff * 100.0,
                    r.width,
                    c.width
                ),
            )
            .with_values(format!("{:.0}", r.width), format!("{:.0}", c.width)),
        );
    } else if width_diff > config.dimension_info_threshold {
        out.push(Issue::info(
            "dimensions",
            format!(
                "Width differs by {:.0}%: expected {:.0}, got {:.0}",
                width_diff * 100.0,
                r.width,
                c.width
            ),
        ));
    }

    let height_diff = if r.height > 0.0 {
        (c.height - r.height).abs() / r.height
    } else {
        0.0
    };
    if height_diff > config.dimension_warning_threshold {
        out.push(
            Issue::warning(
                "dimensions",
                format!(
                    "Height differs by {:.0}%: expected {:.0}, got {:.0}",
                    height_diff * 100.0,
                    r.height,
                    c.height
                ),
            )
            .with_values(format!("{:.0}", r.height), format!("{:.0}", c.height)),
        );
    } else if height_diff > config.dimension_info_threshold {
        out.push(Issue::info(
            "dimensions",
            format!(
                "Height differs by {:.0}%: expected {:.0}, got {:.0}",
                height_diff * 100.0,
                r.height,
                c.height
            ),
        ));
    }
}

fn check_shape_counts(
    c: &SvgStructure,
    r: &SvgStructure,
    config: &CheckConfig,
    out: &mut Vec<Issue>,
) {
    let pairs = [
        ("rect", c.shapes.rect, r.shapes.rect),
        ("circle", c.shapes.circle, r.shapes.circle),
        ("ellipse", c.shapes.ellipse, r.shapes.ellipse),
        ("polygon", c.shapes.polygon, r.shapes.polygon),
        ("path", c.shapes.path, r.shapes.path),
        ("line", c.shapes.line, r.shapes.line),
        ("polyline", c.shapes.polyline, r.shapes.polyline),
    ];

    for (name, got, expected) in pairs {
        if got == expected {
            continue;
        }
        let diff_pct = if expected > 0 {
            ((got as f64 - expected as f64) / expected as f64 * 100.0).abs()
        } else if got > 0 {
            100.0
        } else {
            0.0
        };
        if diff_pct > config.shape_count_warning_pct {
            out.push(
                Issue::warning(
                    "shapes",
                    format!(
                        "{} count differs: expected {}, got {} ({:.0}% diff)",
                        name, expected, got, diff_pct
                    ),
                )
                .with_values(expected.to_string(), got.to_string()),
            );
        }
    }
}

fn check_markers(c: &SvgStructure, r: &SvgStructure, out: &mut Vec<Issue>) {
    if c.marker_count != r.marker_count {
        out.push(Issue::info(
            "markers",
            format!(
                "Marker count differs: expected {}, got {}",
                r.marker_count, c.marker_count
            ),
        ));
    }
}

fn check_z_order(c: &SvgStructure, r: &SvgStructure, out: &mut Vec<Issue>) {
    // If candidate has text-before-shapes violations and reference has none
    // (or fewer), that is a rendering-order regression.
    if c.z_order.text_before_shapes > r.z_order.text_before_shapes
        && !c.z_order.potentially_obscured_labels.is_empty()
    {
        out.push(Issue::warning(
            "z_order",
            format!(
                "Candidate has {} text-before-shape cases vs {} in reference; potentially obscured: {:?}",
                c.z_order.text_before_shapes,
                r.z_order.text_before_shapes,
                c.z_order.potentially_obscured_labels
            ),
        ));
    }
}

fn check_colors(c: &SvgStructure, r: &SvgStructure, config: &CheckConfig, out: &mut Vec<Issue>) {
    let cand: HashSet<_> = c.colors.fill_colors.iter().collect();
    let refer: HashSet<_> = r.colors.fill_colors.iter().collect();
    if cand.is_empty() && refer.is_empty() {
        return;
    }
    let missing: Vec<_> = refer.difference(&cand).cloned().collect();
    let extra: Vec<_> = cand.difference(&refer).cloned().collect();
    if missing.is_empty() && extra.is_empty() {
        return;
    }
    let total_unique = cand.len().max(refer.len());
    let matching = cand.intersection(&refer).count();
    let match_pct = if total_unique > 0 {
        matching as f64 / total_unique as f64 * 100.0
    } else {
        100.0
    };
    if match_pct < config.color_warning_match_pct {
        let mut msg = String::new();
        if !missing.is_empty() {
            msg.push_str(&format!("missing fills: {:?}", missing));
        }
        if !extra.is_empty() {
            if !msg.is_empty() {
                msg.push_str("; ");
            }
            msg.push_str(&format!("extra fills: {:?}", extra));
        }
        out.push(
            Issue::warning(
                "colors",
                format!("Color mismatch ({:.0}% match): {}", match_pct, msg),
            )
            .with_values(
                format!("{:?}", r.colors.fill_colors),
                format!("{:?}", c.colors.fill_colors),
            ),
        );
    }
}

// -- Parsing helpers ---------------------------------------------------------

fn parse_dimensions(root: &Node) -> (f64, f64) {
    if let Some(viewbox) = root.attribute("viewBox") {
        let parts: Vec<f64> = viewbox
            .split_whitespace()
            .filter_map(|s| s.parse().ok())
            .collect();
        if parts.len() >= 4 {
            return (parts[2], parts[3]);
        }
    }
    let width = root
        .attribute("width")
        .and_then(|s| s.trim_end_matches("px").parse().ok())
        .unwrap_or(0.0);
    let height = root
        .attribute("height")
        .and_then(|s| s.trim_end_matches("px").parse().ok())
        .unwrap_or(0.0);
    (width, height)
}

fn count_elements(doc: &Document, tag: &str) -> usize {
    doc.descendants().filter(|n| n.tag_name().name() == tag).count()
}

fn count_shapes(doc: &Document) -> ShapeCounts {
    ShapeCounts {
        rect: count_visible_rects(doc),
        circle: count_elements(doc, "circle"),
        ellipse: count_elements(doc, "ellipse"),
        polygon: count_elements(doc, "polygon"),
        path: count_visible_paths(doc),
        line: count_elements(doc, "line"),
        polyline: count_elements(doc, "polyline"),
    }
}

fn count_visible_rects(doc: &Document) -> usize {
    doc.descendants()
        .filter(|n| n.tag_name().name() == "rect")
        .filter(|n| {
            let class = n.attribute("class").unwrap_or("");
            if class.contains("edge-label-bg") {
                return false;
            }
            let width = n
                .attribute("width")
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);
            let height = n
                .attribute("height")
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);
            width > 0.0 && height > 0.0
        })
        .count()
}

fn count_visible_paths(doc: &Document) -> usize {
    doc.descendants()
        .filter(|n| n.tag_name().name() == "path")
        .filter(|n| {
            let class = n.attribute("class").unwrap_or("");
            if class.contains("label-bg") {
                return false;
            }
            if n.attribute("stroke") == Some("none") {
                return false;
            }
            if let Some(width) = n.attribute("stroke-width") {
                if width.parse::<f64>().ok() == Some(0.0) {
                    return false;
                }
            }
            true
        })
        .count()
}

fn count_nodes_and_edges(doc: &Document) -> (usize, usize) {
    let mut node_count = 0;
    let mut edge_count = 0;

    for node in doc.descendants() {
        // mermaid.js marks edges with a `data-edge` attribute.
        if node.attribute("data-edge").is_some() {
            edge_count += 1;
            continue;
        }
        if let Some(class) = node.attribute("class") {
            let classes: Vec<&str> = class.split_whitespace().collect();
            if classes.iter().any(|c| NODE_CLASSES.contains(c)) {
                node_count += 1;
            }
            if classes.iter().any(|c| EDGE_CLASSES.contains(c)) {
                let tag = node.tag_name().name();
                if tag == "g" || tag == "path" {
                    edge_count += 1;
                }
            }
        }
    }

    (node_count, edge_count)
}

fn extract_labels(doc: &Document) -> Vec<String> {
    let mut labels = Vec::new();
    let mut seen = HashSet::new();

    for node in doc.descendants() {
        let tag = node.tag_name().name();
        if tag == "text" || tag == "p" || tag == "span" {
            let combined = collect_text_content(&node);
            let combined: String = combined.split_whitespace().collect::<Vec<_>>().join(" ");
            if !combined.is_empty() && !seen.contains(&combined) {
                seen.insert(combined.clone());
                labels.push(combined);
            }
        }
    }

    labels.sort();
    labels
}

fn collect_text_content(node: &Node) -> String {
    let mut result = String::new();
    for child in node.children() {
        if child.is_text() {
            if let Some(text) = child.text() {
                result.push_str(text);
            }
        } else {
            let tag = child.tag_name().name();
            if !result.is_empty()
                && !result.ends_with(' ')
                && !result.ends_with('\n')
                && (tag == "tspan" || tag == "br")
            {
                result.push(' ');
            }
            result.push_str(&collect_text_content(&child));
        }
    }
    result
}

fn analyze_z_order(doc: &Document) -> ZOrderSummary {
    let mut summary = ZOrderSummary::default();
    const SHAPE_TAGS: &[&str] = &[
        "rect", "circle", "ellipse", "polygon", "path", "line", "polyline",
    ];
    const TEXT_TAGS: &[&str] = &["text", "tspan", "foreignObject"];

    for group in doc.descendants().filter(|n| n.tag_name().name() == "g") {
        let mut last_shape_index: Option<usize> = None;
        let mut last_text_index: Option<usize> = None;

        for (i, child) in group.children().enumerate() {
            let tag = child.tag_name().name();
            if SHAPE_TAGS.contains(&tag) {
                last_shape_index = Some(i);
                if let Some(text_idx) = last_text_index {
                    if text_idx < i {
                        summary.text_before_shapes += 1;
                        if let Some(text_node) = group.children().nth(text_idx) {
                            let label = collect_text_content(&text_node)
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" ");
                            if !label.is_empty()
                                && !summary.potentially_obscured_labels.contains(&label)
                            {
                                summary.potentially_obscured_labels.push(label);
                            }
                        }
                    }
                }
            }
            if TEXT_TAGS.contains(&tag) {
                last_text_index = Some(i);
                if last_shape_index.is_some() {
                    summary.text_after_shapes += 1;
                }
            }
        }
    }
    summary
}

fn analyze_colors(doc: &Document) -> ColorSummary {
    let mut fills = HashSet::new();
    let mut strokes = HashSet::new();
    for node in doc.descendants() {
        if let Some(fill) = node.attribute("fill") {
            let v = fill.trim().to_lowercase();
            if !v.is_empty() && v != "none" {
                fills.insert(v);
            }
        }
        if let Some(stroke) = node.attribute("stroke") {
            let v = stroke.trim().to_lowercase();
            if !v.is_empty() && v != "none" {
                strokes.insert(v);
            }
        }
    }
    let mut fill_colors: Vec<_> = fills.into_iter().collect();
    fill_colors.sort();
    let mut stroke_colors: Vec<_> = strokes.into_iter().collect();
    stroke_colors.sort();
    ColorSummary {
        fill_colors,
        stroke_colors,
    }
}

// -- Tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50">
        <g class="node"><rect x="0" y="0" width="10" height="10"/><text>A</text></g>
        <g class="edge"><path d="M0 0L10 10" stroke="#000"/></g>
    </svg>"##;

    #[test]
    fn parses_minimal_svg() {
        let s = SvgStructure::from_svg(MINIMAL_SVG).unwrap();
        assert_eq!(s.width, 100.0);
        assert_eq!(s.height, 50.0);
        assert_eq!(s.node_count, 1);
        assert_eq!(s.edge_count, 1);
        assert_eq!(s.labels, vec!["A".to_string()]);
        assert_eq!(s.shapes.rect, 1);
        assert_eq!(s.shapes.path, 1);
    }

    #[test]
    fn identical_svgs_have_no_diff() {
        let diff = compare(MINIMAL_SVG, MINIMAL_SVG).unwrap();
        assert!(diff.is_empty(), "expected empty diff, got: {}", diff.report_text());
    }

    #[test]
    fn detects_node_count_mismatch() {
        let other = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50">
            <g class="node"><rect x="0" y="0" width="10" height="10"/><text>A</text></g>
            <g class="node"><rect x="0" y="0" width="10" height="10"/><text>B</text></g>
            <g class="edge"><path d="M0 0L10 10" stroke="#000"/></g>
        </svg>"##;
        let diff = compare(other, MINIMAL_SVG).unwrap();
        assert!(diff.has_errors());
        assert!(diff.errors().any(|i| i.check == "node_count"));
    }

    #[test]
    fn detects_missing_labels() {
        let cand = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50">
            <g class="node"><rect x="0" y="0" width="10" height="10"/><text>X</text></g>
            <g class="edge"><path d="M0 0L10 10" stroke="#000"/></g>
        </svg>"##;
        let diff = compare(cand, MINIMAL_SVG).unwrap();
        assert!(diff.errors().any(|i| i.check == "labels_missing"));
    }
}
