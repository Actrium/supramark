//! Hand-rolled parser for the `ishikawa-beta` (fishbone) diagram.
//!
//! Upstream grammar: /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/ishikawa/parser/ishikawa.jison
//! Upstream DB (post-AST reshape): /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/ishikawa/ishikawaDb.ts
//!
//! Grammar (jison, 56 LoC):
//!
//! ```text
//! ishikawa[-beta]                             # header (both spellings accepted)
//! [<leading-whitespace>] <text until EOL>     # each remaining line = addNode(len(ws), text.trim())
//! ```
//!
//! Each indented line becomes an [`IshikawaNode`]. The first line is the
//! root (effect). Subsequent lines are nested by indent:
//!   - `baseLevel` is the indent of the FIRST cause (not the root).
//!   - Every cause's `level = rawLevel - baseLevel + 1`, floored at 1.
//!   - The stack pops until the top has a strictly lower level — that
//!     becomes the parent.
//!
//! Frontmatter (`---\nconfig:\n  ishikawa:\n    diagramPadding: N\n---`)
//! is consumed to extract diagramPadding. `%%{init:...}%%` directives
//! are ignored by this parser — only diagramPadding is relevant to
//! our byte-exact targets (forest theme comes from the global config,
//! already resolved before render).

use crate::error::{MermaidError, Result};
use crate::model::ishikawa::{IshikawaDiagram, IshikawaNode};

/// Parse an ishikawa source document.
pub fn parse(source: &str) -> Result<IshikawaDiagram> {
    // Normalise CRLF → LF so downstream indent measurement is byte-accurate.
    let normalised = source.replace("\r\n", "\n").replace('\r', "\n");

    // Strip and mine frontmatter.
    let (diagram_padding, body) = extract_frontmatter(&normalised);

    // Skip leading blank/comment lines before the header.
    let mut lines = body.lines().peekable();

    // Consume header line — must be "ishikawa" or "ishikawa-beta"
    // (trimmed, case-insensitive per the jison lexer).
    let mut saw_header = false;
    for line in lines.by_ref() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("%%") {
            continue;
        }
        let low = trimmed.to_ascii_lowercase();
        if low == "ishikawa-beta" || low == "ishikawa" {
            saw_header = true;
            break;
        }
        return Err(MermaidError::Parse {
            line: 0,
            col: 0,
            message: format!("expected 'ishikawa-beta', got: {trimmed}"),
        });
    }
    if !saw_header {
        return Err(MermaidError::Parse {
            line: 0,
            col: 0,
            message: "empty ishikawa source".into(),
        });
    }

    let mut root: Option<IshikawaNode> = None;
    // Stack holds indices into the path from the root to the current
    // tail. We keep raw indices as chains of child-list indices so we
    // can re-materialise a `&mut IshikawaNode` on every `addNode`.
    let mut stack_levels: Vec<i64> = Vec::new();
    let mut stack_path: Vec<Vec<usize>> = Vec::new(); // [path-to-node-on-stack]
    let mut base_level: Option<i64> = None;

    for raw in lines {
        // Measure leading whitespace run in BYTES to match jison's
        // `[\s]+` + $1.length — spaces, tabs, and %%{directive}%% lines
        // are handled separately. Pure blank lines are skipped (SPACELINE
        // in jison yields no addNode call).
        if raw.trim().is_empty() {
            continue;
        }
        // Skip comment lines `%% ...`.
        let trimmed_for_comment = raw.trim_start();
        if trimmed_for_comment.starts_with("%%") {
            continue;
        }
        // Indent count: number of leading ws chars.
        let indent: i64 = raw.chars().take_while(|c| *c == ' ' || *c == '\t').count() as i64;
        let text = raw.trim().to_string();

        // sanitizeText at `securityLevel: strict` with default config
        // escapes `<` `>` `=` — our fixtures never contain them so we
        // pass text through unchanged. Future work: plumb a real
        // sanitiser when a fixture needs it.

        if root.is_none() {
            root = Some(IshikawaNode {
                text: text.clone(),
                children: Vec::new(),
            });
            stack_levels.push(0);
            stack_path.push(Vec::new()); // root has empty path
            continue;
        }

        // First cause sets the baseline indent.
        let raw_level = indent;
        if base_level.is_none() {
            base_level = Some(raw_level);
        }
        let mut level = raw_level - base_level.unwrap() + 1;
        if level <= 0 {
            level = 1;
        }

        // Pop stack while top.level >= level (keep at least one entry).
        while stack_levels.len() > 1 && *stack_levels.last().unwrap() >= level {
            stack_levels.pop();
            stack_path.pop();
        }

        // Append the new node as a child of the parent (top-of-stack).
        let parent_path = stack_path.last().cloned().unwrap_or_default();
        let parent = walk_mut(root.as_mut().unwrap(), &parent_path);
        let new_child_idx = parent.children.len();
        parent.children.push(IshikawaNode {
            text,
            children: Vec::new(),
        });

        let mut new_path = parent_path;
        new_path.push(new_child_idx);
        stack_levels.push(level);
        stack_path.push(new_path);
    }

    Ok(IshikawaDiagram {
        meta: Default::default(),
        root,
        diagram_padding,
    })
}

/// Walk into `root` along `path` (each entry = index into `children`).
fn walk_mut<'a>(root: &'a mut IshikawaNode, path: &[usize]) -> &'a mut IshikawaNode {
    let mut cur = root;
    for &idx in path {
        cur = &mut cur.children[idx];
    }
    cur
}

/// Strip a YAML-ish frontmatter block (`---\n...\n---`) from the head
/// of `src`, returning (diagramPadding, rest_source). Only the single
/// field `config.ishikawa.diagramPadding` is mined — every other key
/// is silently ignored. Default padding is 20 (upstream default).
fn extract_frontmatter(src: &str) -> (f64, String) {
    const DEFAULT_PAD: f64 = 20.0;
    let trimmed = src.trim_start_matches(['\n', ' ', '\t']);
    if !trimmed.starts_with("---") {
        return (DEFAULT_PAD, src.to_string());
    }
    // Find the closing `---` line.
    let mut lines = trimmed.lines();
    // consume opening ---
    let _ = lines.next();
    let mut fm_body = String::new();
    let mut end_found = false;
    let mut consumed_bytes: usize = 4; // "---\n"
    for line in lines.by_ref() {
        consumed_bytes += line.len() + 1;
        if line.trim() == "---" {
            end_found = true;
            break;
        }
        fm_body.push_str(line);
        fm_body.push('\n');
    }
    if !end_found {
        return (DEFAULT_PAD, src.to_string());
    }
    // Determine offset of the rest relative to the ORIGINAL src. We
    // replayed via `.lines()` on the post-trim string, so adjust.
    let trim_off = src.len() - trimmed.len();
    let rest_start = (trim_off + consumed_bytes).min(src.len());
    let rest = src[rest_start..].to_string();

    // Parse the frontmatter body for diagramPadding.
    // We look for a line matching `\s*diagramPadding\s*:\s*<number>` AND
    // that it is nested under `ishikawa:`. A full YAML parser is
    // overkill — use a tiny indent-aware state machine.
    let mut pad = DEFAULT_PAD;
    let mut in_config = false;
    let mut in_ishikawa = false;
    let mut config_indent: Option<usize> = None;
    let mut ishikawa_indent: Option<usize> = None;
    for line in fm_body.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.chars().take_while(|c| *c == ' ').count();
        let content = &line[indent..];
        // Exit nested sections when dedent reached.
        if let Some(ci) = config_indent {
            if indent <= ci && !content.starts_with('-') {
                in_config = false;
                in_ishikawa = false;
                config_indent = None;
                ishikawa_indent = None;
            }
        }
        if let Some(ii) = ishikawa_indent {
            if indent <= ii {
                in_ishikawa = false;
                ishikawa_indent = None;
            }
        }
        if !in_config && content.starts_with("config:") {
            in_config = true;
            config_indent = Some(indent);
            continue;
        }
        if in_config && !in_ishikawa && content.starts_with("ishikawa:") {
            in_ishikawa = true;
            ishikawa_indent = Some(indent);
            continue;
        }
        if in_ishikawa {
            // Match `diagramPadding: NUMBER` with optional surrounding ws.
            if let Some(rest) = content.strip_prefix("diagramPadding:") {
                if let Ok(n) = rest.trim().parse::<f64>() {
                    pad = n;
                }
            }
        }
    }

    (pad, rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_hierarchy() {
        let src = "ishikawa-beta\n    Blurry Photo\n        Process\n            Out of focus\n        User\n            Shaky hands\n";
        let d = parse(src).expect("parse");
        let root = d.root.expect("root");
        assert_eq!(root.text, "Blurry Photo");
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].text, "Process");
        assert_eq!(root.children[0].children[0].text, "Out of focus");
        assert_eq!(root.children[1].text, "User");
        assert_eq!(root.children[1].children[0].text, "Shaky hands");
    }

    #[test]
    fn parse_unindented_root() {
        let src = "ishikawa-beta\nProblem\nCause A\n  Subcause A1\nCause B\n";
        let d = parse(src).expect("parse");
        let root = d.root.expect("root");
        assert_eq!(root.text, "Problem");
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].text, "Cause A");
        assert_eq!(root.children[0].children[0].text, "Subcause A1");
    }

    #[test]
    fn parse_effect_indented_more_than_causes() {
        let src = "ishikawa-beta\n    Problem\nCause A\n  Subcause A1\nCause B\n";
        let d = parse(src).expect("parse");
        let root = d.root.expect("root");
        assert_eq!(root.text, "Problem");
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].children.len(), 1);
    }

    #[test]
    fn parse_frontmatter_padding() {
        let src =
            "---\nconfig:\n  ishikawa:\n    diagramPadding: 100\n---\nishikawa-beta\n  E\n  C\n";
        let d = parse(src).expect("parse");
        assert_eq!(d.diagram_padding, 100.0);
    }

    #[test]
    fn parse_no_frontmatter_defaults() {
        let d = parse("ishikawa-beta\nProblem\n").expect("parse");
        assert_eq!(d.diagram_padding, 20.0);
    }
}
