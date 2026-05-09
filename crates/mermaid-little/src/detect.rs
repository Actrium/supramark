//! Diagram-type detection — linear regex scan that mirrors upstream
//! `diagram-api/detectType.ts` and the registration order in
//! `diagram-orchestration.ts`.
//!
//! Upstream iterates its `detectors` map in insertion order; the first
//! detector whose regex matches wins. We replicate that exactly, with
//! **v2 detectors placed before v1** for the three diagram types
//! that ship both (class, state, flowchart) — this matches the
//! registration order at `diagram-orchestration.ts` ll. 84-113.
//!
//! Order matters: `graph` (flowchart-v2/v1) comes after `gantt`,
//! `journey`, `gitGraph`, `sequenceDiagram`, etc. because the more
//! specific keywords must win first. Upstream's ordering is
//! preserved verbatim in [`detect`].
//!
//! The scan operates on whichever text we're given. Like upstream, we
//! first strip frontmatter, directives, and comments so a `%%{init:
//! ...}%%` line starting with a `{` doesn't derail detection. That
//! matches `detectType.ts` ll. 37-40.

use crate::config::{directive, frontmatter};
use regex::Regex;
use std::sync::OnceLock;

/// One variant per user-facing diagram type in `mermaid@11.14.0`,
/// plus:
///   - `Info`  — undocumented `info` diagram (still registered upstream),
///   - `Error` — the synthetic `error` diagram the JS side uses as a
///     last-resort renderer,
///   - `Unknown` — no detector matched.
///
/// Kept intentionally separate from [`crate::model::Diagram`]: this
/// enum is produced **before** parsing, and the parser can then build
/// the matching `Diagram` variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagramKind {
    C4,
    Kanban,
    Class,
    Er,
    Gantt,
    Info,
    Pie,
    Requirement,
    Sequence,
    Flowchart,
    Timeline,
    GitGraph,
    State,
    Journey,
    Quadrant,
    Sankey,
    Packet,
    Xychart,
    Block,
    Treemap,
    Radar,
    Ishikawa,
    Mindmap,
    Architecture,
    Venn,
    Wardley,
    /// Synthetic diagram mermaid renders when every other detector
    /// fails (`/^error$/i`).
    Error,
    /// Detection produced no match.
    Unknown,
}

impl DiagramKind {
    /// Short identifier mermaid uses internally for each detector
    /// (matches the `id` strings in upstream `*Detector.ts` files).
    pub fn id(&self) -> &'static str {
        match self {
            DiagramKind::C4 => "c4",
            DiagramKind::Kanban => "kanban",
            DiagramKind::Class => "class",
            DiagramKind::Er => "er",
            DiagramKind::Gantt => "gantt",
            DiagramKind::Info => "info",
            DiagramKind::Pie => "pie",
            DiagramKind::Requirement => "requirement",
            DiagramKind::Sequence => "sequence",
            DiagramKind::Flowchart => "flowchart",
            DiagramKind::Timeline => "timeline",
            DiagramKind::GitGraph => "gitGraph",
            DiagramKind::State => "state",
            DiagramKind::Journey => "journey",
            DiagramKind::Quadrant => "quadrantChart",
            DiagramKind::Sankey => "sankey",
            DiagramKind::Packet => "packet",
            DiagramKind::Xychart => "xychart",
            DiagramKind::Block => "block",
            DiagramKind::Treemap => "treemap",
            DiagramKind::Radar => "radar",
            DiagramKind::Ishikawa => "ishikawa",
            DiagramKind::Mindmap => "mindmap",
            DiagramKind::Architecture => "architecture",
            DiagramKind::Venn => "venn",
            DiagramKind::Wardley => "wardley",
            DiagramKind::Error => "error",
            DiagramKind::Unknown => "unknown",
        }
    }
}

/// One entry in the detector table.
struct Detector {
    regex: &'static OnceLock<Regex>,
    source: &'static str,
    kind: DiagramKind,
}

fn compile(slot: &'static OnceLock<Regex>, src: &'static str) -> &'static Regex {
    slot.get_or_init(|| Regex::new(src).expect("valid detector regex"))
}

macro_rules! detector {
    ($name:ident, $pat:literal, $kind:expr) => {{
        static SLOT: OnceLock<Regex> = OnceLock::new();
        Detector {
            regex: &SLOT,
            source: $pat,
            kind: $kind,
        }
    }};
}

/// Returns the ordered detector list. Built fresh on every call —
/// cheap, because each `regex` is a `&OnceLock<Regex>` that only
/// compiles the inner pattern on first use.
fn detectors() -> Vec<Detector> {
    vec![
        // --- upstream order from diagram-orchestration.ts ll. 84-113 ---
        detector!(
            c4,
            r"^\s*C4Context|C4Container|C4Component|C4Dynamic|C4Deployment",
            DiagramKind::C4
        ),
        detector!(kanban, r"^\s*kanban", DiagramKind::Kanban),
        // classDiagram-v2 before classDiagram (upstream order).
        detector!(class_v2, r"^\s*classDiagram-v2", DiagramKind::Class),
        detector!(class_v1, r"^\s*classDiagram", DiagramKind::Class),
        detector!(er, r"^\s*erDiagram", DiagramKind::Er),
        detector!(gantt, r"^\s*gantt", DiagramKind::Gantt),
        detector!(info, r"^\s*info", DiagramKind::Info),
        detector!(pie, r"^\s*pie", DiagramKind::Pie),
        detector!(
            requirement,
            r"^\s*requirement(Diagram)?",
            DiagramKind::Requirement
        ),
        detector!(sequence, r"^\s*sequenceDiagram", DiagramKind::Sequence),
        // flowchart-v2 before flowchart. `flowchart` and `graph` are
        // the v2 / v1 keywords respectively.
        detector!(flow_v2, r"^\s*flowchart", DiagramKind::Flowchart),
        detector!(flow_v1, r"^\s*graph", DiagramKind::Flowchart),
        detector!(timeline, r"^\s*timeline", DiagramKind::Timeline),
        detector!(git, r"^\s*gitGraph", DiagramKind::GitGraph),
        // stateDiagram-v2 before stateDiagram.
        detector!(state_v2, r"^\s*stateDiagram-v2", DiagramKind::State),
        detector!(state_v1, r"^\s*stateDiagram", DiagramKind::State),
        detector!(journey, r"^\s*journey", DiagramKind::Journey),
        detector!(quadrant, r"^\s*quadrantChart", DiagramKind::Quadrant),
        detector!(sankey, r"^\s*sankey(-beta)?", DiagramKind::Sankey),
        detector!(packet, r"^\s*packet(-beta)?", DiagramKind::Packet),
        detector!(xychart, r"^\s*xychart(-beta)?", DiagramKind::Xychart),
        detector!(block, r"^\s*block(-beta)?", DiagramKind::Block),
        // upstream registers `treeView` between block and radar, but
        // that one has no public key; omit to match observed detectors.
        detector!(radar, r"^\s*radar-beta", DiagramKind::Radar),
        detector!(
            ishikawa,
            r"(?i)^\s*ishikawa(-beta)?\b",
            DiagramKind::Ishikawa
        ),
        detector!(treemap, r"^\s*treemap", DiagramKind::Treemap),
        detector!(venn, r"^\s*venn-beta", DiagramKind::Venn),
        detector!(wardley, r"(?i)^\s*wardley-beta", DiagramKind::Wardley),
        // The three large-feature detectors are registered earlier
        // upstream when `injected.includeLargeFeatures` is on. We
        // always want them available, so append them unconditionally.
        detector!(mindmap, r"^\s*mindmap", DiagramKind::Mindmap),
        detector!(architecture, r"^\s*architecture", DiagramKind::Architecture),
    ]
}

/// Detect the diagram type of `source`.
///
/// Steps (mirroring upstream `detectType`):
///   1. Strip frontmatter, directives and `%%...` comment lines — they
///      can start with characters that otherwise collide with a regex
///      anchor (e.g. `---` or `%%{init:...}`).
///   2. Trim leading whitespace on each logical line.
///   3. Try detectors in insertion order — first match wins.
///   4. Fall back to [`DiagramKind::Error`] for an explicit `error`
///      keyword (upstream registers this as its very first detector),
///      else [`DiagramKind::Unknown`].
pub fn detect(source: &str) -> DiagramKind {
    // Run the same three strips upstream does before its detector loop.
    let (_, stripped) = frontmatter::parse_frontmatter(source);
    let stripped = directive::remove_directives(stripped);
    let stripped = strip_percent_comments(&stripped);
    let probe = stripped.trim_start();

    // Upstream registers `error` and `---` synthetic detectors first.
    // The `---` one only fires when frontmatter extraction already
    // failed, which can't happen for us here (step 1 handled it). We
    // only honour the literal `error` diagram.
    if probe.trim().eq_ignore_ascii_case("error") {
        return DiagramKind::Error;
    }

    for d in detectors() {
        let re = compile(d.regex, d.source);
        if re.is_match(probe) {
            return d.kind;
        }
    }

    DiagramKind::Unknown
}

/// Strip every `%%`-prefixed comment line (but leave `%%{` directives
/// alone — they were already removed by the caller).
fn strip_percent_comments(text: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?m)^\s*%%(?:[^\{\n][^\n]*)?\n?").unwrap());
    re.replace_all(text, "").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(src: &str) -> DiagramKind {
        detect(src)
    }

    #[test]
    fn detects_flowchart_v2() {
        assert_eq!(kind("flowchart TD\nA-->B\n"), DiagramKind::Flowchart);
    }

    #[test]
    fn detects_flowchart_v1_graph_keyword() {
        assert_eq!(kind("graph LR\nA-->B\n"), DiagramKind::Flowchart);
    }

    #[test]
    fn detects_class_v2_before_v1() {
        // v2 pattern is more specific and registered first. Even
        // though v1 `classDiagram` matches it too, v2 must win.
        assert_eq!(kind("classDiagram-v2\nclass Foo\n"), DiagramKind::Class);
        assert_eq!(kind("classDiagram\nclass Foo\n"), DiagramKind::Class);
    }

    #[test]
    fn detects_state_v2() {
        assert_eq!(kind("stateDiagram-v2\n[*] --> Idle\n"), DiagramKind::State);
    }

    #[test]
    fn detects_sequence() {
        assert_eq!(kind("sequenceDiagram\nA->>B: hi\n"), DiagramKind::Sequence);
    }

    #[test]
    fn detects_gantt_not_graph() {
        // Starts with `gantt`, so gantt detector wins before flowchart.
        assert_eq!(kind("gantt\n    title A\n"), DiagramKind::Gantt);
    }

    #[test]
    fn detects_pie_packet_radar() {
        assert_eq!(kind("pie\ntitle x\n"), DiagramKind::Pie);
        assert_eq!(kind("packet\n"), DiagramKind::Packet);
        assert_eq!(kind("packet-beta\n"), DiagramKind::Packet);
        assert_eq!(kind("radar-beta\n"), DiagramKind::Radar);
    }

    #[test]
    fn detects_beta_aliases() {
        assert_eq!(kind("sankey-beta\n"), DiagramKind::Sankey);
        assert_eq!(kind("xychart-beta\n"), DiagramKind::Xychart);
        assert_eq!(kind("block-beta\n"), DiagramKind::Block);
        assert_eq!(kind("venn-beta\n"), DiagramKind::Venn);
        assert_eq!(kind("wardley-beta\n"), DiagramKind::Wardley);
    }

    #[test]
    fn detects_ishikawa_case_insensitive() {
        assert_eq!(kind("ISHIKAWA\n"), DiagramKind::Ishikawa);
        assert_eq!(kind("ishikawa-beta\n"), DiagramKind::Ishikawa);
    }

    #[test]
    fn detects_through_frontmatter_and_init_directive() {
        let src = "---\ntitle: demo\nconfig:\n  theme: dark\n---\n%%{init: {theme: \"forest\"}}%%\n%% a comment line\nflowchart TD\nA-->B\n";
        assert_eq!(kind(src), DiagramKind::Flowchart);
    }

    #[test]
    fn detects_error_synthetic() {
        assert_eq!(kind("error"), DiagramKind::Error);
        assert_eq!(kind("  Error  "), DiagramKind::Error);
    }

    #[test]
    fn unknown_on_gibberish() {
        assert_eq!(kind("not-a-mermaid-diagram\n"), DiagramKind::Unknown);
    }

    #[test]
    fn info_detector_ordered_before_pie_per_upstream() {
        assert_eq!(kind("info\n"), DiagramKind::Info);
    }
}
