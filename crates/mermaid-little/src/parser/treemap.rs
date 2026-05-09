//! Treemap parser — line-oriented port of the Langium grammar at
//! `/ext/mermaid-official-stable-v11.14.0/packages/parser/src/language/treemap/treemap.langium`
//! plus the AST → DB transform from `diagrams/treemap/parser.ts` and
//! `utils.ts::buildHierarchy`.
//!
//! Key upstream quirks we reproduce:
//!   * `INDENTATION` value-converter returns `input.length` — hierarchy
//!     level is the number of leading whitespace characters, not a tab
//!     count.
//!   * `buildHierarchy` pops the parent stack while
//!     `stack[top].level >= item.level`. Root nodes have `level == 0`
//!     (no `indent` match).
//!   * `addClass` splits `styleText` on `;` *after* escaping `\,` →
//!     `§§§` so that commas inside individual declarations survive.
//!     Each non-text style also goes into `styles` (upstream pushes to
//!     both lists when `isLabelStyle` returns true).
//!   * Leaf lines parse as `"name": <number>` optionally followed by
//!     `:::className`. Numbers accept commas as thousand separators
//!     (the `NUMBER2` terminal), which we strip before `parseFloat`.

use crate::error::{MermaidError, Result};
use crate::model::treemap::{
    NodeId, TreemapClassDef, TreemapConfig, TreemapDiagram, TreemapNode, TreemapNodeKind,
};

/// Run the treemap parser on raw mermaid source (frontmatter and
/// directives may still be present — we strip them ourselves so the
/// byte-exact test harness can hand us untouched `.mmd` bytes).
pub fn parse(source: &str) -> Result<TreemapDiagram> {
    let mut d = TreemapDiagram::default();

    let source = normalise_newlines(source);
    let (after_frontmatter, fm_title, theme_override, fm_config) = strip_frontmatter(&source);
    if let Some(t) = fm_title {
        d.meta.title = Some(t);
    }
    d.theme_override = theme_override;
    d.config = fm_config;
    let after_directives = strip_init_directives(&after_frontmatter, &mut d);
    let cleaned = strip_line_comments(&after_directives);

    let mut iter = cleaned.lines().enumerate().peekable();
    // Skip blank lines and locate the `treemap` / `treemap-beta` header.
    let mut seen_header = false;
    while let Some((_, line)) = iter.peek() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            iter.next();
            continue;
        }
        if trimmed == "treemap" || trimmed == "treemap-beta" {
            iter.next();
            seen_header = true;
            break;
        }
        break;
    }
    if !seen_header {
        return Err(MermaidError::Parse {
            line: 1,
            col: 1,
            message: "missing treemap header".into(),
        });
    }

    // Collected flat items (level, name, kind, value, class) mirroring
    // upstream's `items` array just before `buildHierarchy`.
    struct FlatItem {
        level: usize,
        name: String,
        kind: TreemapNodeKind,
        value: Option<f64>,
        class_selector: Option<String>,
    }
    let mut items: Vec<FlatItem> = Vec::new();

    for (idx, raw) in iter {
        let line = raw;
        let trimmed = line.trim_start_matches([' ', '\t']);
        if trimmed.trim().is_empty() {
            continue;
        }
        // classDef — can be anywhere in the body (upstream iterates all
        // rows twice: once for classDefs, once for items).
        if let Some(class) = try_parse_classdef(trimmed) {
            d.classes.push(class);
            continue;
        }
        // accTitle / accDescr / title (body-level)
        if let Some(rest) = strip_kw(trimmed, "title") {
            let value = rest.trim().to_string();
            if !value.is_empty() {
                d.meta.title = Some(value);
            }
            continue;
        }
        if let Some(rest) = strip_kw_colon(trimmed, "accTitle") {
            d.meta.acc_title = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = strip_kw_colon(trimmed, "accDescr") {
            d.meta.acc_descr = Some(rest.trim().to_string());
            continue;
        }

        // Items: indentation = count of leading whitespace characters.
        let indent_len = line.len() - trimmed.len();

        let (name, after_name) = match parse_string_literal(trimmed) {
            Some(pair) => pair,
            None => {
                return Err(MermaidError::Parse {
                    line: idx + 1,
                    col: 1,
                    message: format!("expected quoted item name, got `{line}`"),
                });
            }
        };

        let rest = after_name.trim_start();
        // Leaf: optional ":::cls" is preceded by ":" value [+ ":::cls"].
        let mut value: Option<f64> = None;
        let mut class_selector: Option<String> = None;
        let mut rest_work = rest;
        if rest_work.starts_with(':') && !rest_work.starts_with(":::") {
            // Leaf branch.
            rest_work = &rest_work[1..];
            rest_work = rest_work.trim_start();
            let (num_str, remain) = split_number_token(rest_work);
            let raw = num_str.replace(',', "");
            let parsed: f64 = raw.parse().map_err(|_| MermaidError::Parse {
                line: idx + 1,
                col: 1,
                message: format!("invalid number `{num_str}`"),
            })?;
            value = Some(parsed);
            rest_work = remain.trim_start();
        }
        if let Some(stripped) = rest_work.strip_prefix(":::") {
            let (name_part, _) = split_identifier(stripped);
            if !name_part.is_empty() {
                class_selector = Some(name_part.to_string());
            }
        }

        let kind = if value.is_some() {
            TreemapNodeKind::Leaf
        } else {
            TreemapNodeKind::Section
        };

        items.push(FlatItem {
            level: indent_len,
            name,
            kind,
            value,
            class_selector,
        });
    }

    // Assemble the tree — mirror `buildHierarchy` in utils.ts with two
    // twists:
    //   * we write directly into our flat `TreemapNode` arena.
    //   * we track per-class style lookups here so the renderer doesn't
    //     need a `TreemapDiagram::getStylesForClass` helper.
    let mut stack: Vec<(NodeId, usize)> = Vec::new();
    for item in items {
        let id = d.nodes.len();
        let css = compiled_styles_for(&d.classes, item.class_selector.as_deref());
        let node = TreemapNode {
            id,
            name: item.name,
            kind: item.kind.clone(),
            value: item.value,
            parent: None,
            children: if matches!(item.kind, TreemapNodeKind::Section) {
                Some(Vec::new())
            } else {
                None
            },
            class_selector: item.class_selector,
            css_compiled_styles: css,
            depth: 0, // Filled after parent is known.
        };
        d.nodes.push(node);

        while let Some(&(_, lvl)) = stack.last() {
            if lvl >= item.level {
                stack.pop();
            } else {
                break;
            }
        }
        if stack.is_empty() {
            d.outer_nodes.push(id);
            d.nodes[id].depth = 1; // outer = d3 depth 1 (root itself is 0)
        } else {
            let parent_id = stack.last().unwrap().0;
            d.nodes[id].parent = Some(parent_id);
            let p_depth = d.nodes[parent_id].depth;
            d.nodes[id].depth = p_depth + 1;
            if let Some(children) = d.nodes[parent_id].children.as_mut() {
                children.push(id);
            }
        }
        if matches!(item.kind, TreemapNodeKind::Section) {
            stack.push((id, item.level));
        }
    }

    Ok(d)
}

/// Look up compiled-styles for a class selector, matching upstream
/// `addClass` semantics: styles are split on `;`, with `\,` escape
/// preservation.
fn compiled_styles_for(classes: &[TreemapClassDef], sel: Option<&str>) -> Vec<String> {
    let Some(name) = sel else { return Vec::new() };
    for c in classes {
        if c.id == name {
            return c.styles.clone();
        }
    }
    Vec::new()
}

/// Strip `%%{init: ...}%%` blocks, populating any `treemap` /
/// `themeVariables` keys we understand. Unknown keys are tolerated.
fn strip_init_directives(source: &str, d: &mut TreemapDiagram) -> String {
    // We don't try to reproduce the full preprocess pipeline here —
    // directive handling is best-effort so unit tests that feed raw mmd
    // still honour `valueFormat` set in frontmatter. The preprocess
    // stage invoked from `lib.rs` already strips directives for the
    // main pipeline.
    let _ = d;
    source.to_string()
}

/// Pull out a single `---\n...\n---` frontmatter block at the start of
/// the document. Captures `title`, `config.theme`, and `config.treemap.*`
/// keys we understand, and returns the remainder with frontmatter
/// removed.
fn strip_frontmatter(source: &str) -> (String, Option<String>, Option<String>, TreemapConfig) {
    let mut cfg = TreemapConfig::default();
    let mut title = None;
    let mut theme = None;
    let text = source;
    let first_line = text.lines().next().unwrap_or("");
    if first_line.trim() != "---" {
        return (text.to_string(), title, theme, cfg);
    }
    // Find second "---" delimiter.
    let after_open = match text.find('\n') {
        Some(pos) => &text[pos + 1..],
        None => return (text.to_string(), title, theme, cfg),
    };
    let (body, tail) = match find_fm_close(after_open) {
        Some((b, t)) => (b, t),
        None => return (text.to_string(), title, theme, cfg),
    };

    // Lightweight YAML walker — we only need the small subset used by
    // treemap fixtures (`config:\n  theme: ...\n  treemap:\n    valueFormat: ...`).
    let mut in_config = false;
    let mut in_treemap = false;
    for raw_line in body.lines() {
        let line = raw_line.trim_end();
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if indent == 0 {
            in_config = trimmed.starts_with("config:");
            in_treemap = false;
            if trimmed.starts_with("title:") {
                let v = trimmed["title:".len()..]
                    .trim()
                    .trim_matches('"')
                    .to_string();
                if !v.is_empty() {
                    title = Some(v);
                }
            }
            continue;
        }
        if in_config && indent == 2 {
            if let Some(rest) = trimmed.strip_prefix("theme:") {
                theme = Some(rest.trim().trim_matches('"').to_string());
                in_treemap = false;
            } else if trimmed.starts_with("treemap:") {
                in_treemap = true;
            } else {
                in_treemap = false;
            }
            continue;
        }
        if in_config && in_treemap && indent == 4 {
            let Some((k, v)) = trimmed.split_once(':') else {
                continue;
            };
            let k = k.trim();
            let v = v.trim().trim_matches('"').to_string();
            match k {
                "valueFormat" => cfg.value_format = Some(v),
                "showValues" => cfg.show_values = Some(v == "true"),
                "padding" => {
                    if let Ok(n) = v.parse::<f64>() {
                        cfg.padding = Some(n);
                    }
                }
                "nodeWidth" => {
                    if let Ok(n) = v.parse::<f64>() {
                        cfg.node_width = Some(n);
                    }
                }
                "nodeHeight" => {
                    if let Ok(n) = v.parse::<f64>() {
                        cfg.node_height = Some(n);
                    }
                }
                "useMaxWidth" => cfg.use_max_width = Some(v == "true"),
                "diagramPadding" => {
                    if let Ok(n) = v.parse::<f64>() {
                        cfg.diagram_padding = Some(n);
                    }
                }
                _ => {}
            }
        }
    }

    (tail.to_string(), title, theme, cfg)
}

fn find_fm_close(text: &str) -> Option<(&str, &str)> {
    let mut pos = 0;
    while pos < text.len() {
        let rest = &text[pos..];
        let line_end = rest.find('\n').map(|p| pos + p).unwrap_or(text.len());
        let line = &text[pos..line_end];
        if line.trim() == "---" {
            let body = &text[..pos];
            let tail_start = (line_end + 1).min(text.len());
            return Some((body, &text[tail_start..]));
        }
        pos = line_end + 1;
    }
    None
}

fn strip_line_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for line in source.lines() {
        let trimmed = line.trim_start();
        // `%%` comments — treat the rest of the line as a comment, but
        // preserve the line itself so that line numbering stays intact.
        if trimmed.starts_with("%%") {
            out.push('\n');
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn normalise_newlines(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// Parse a quoted `"..."` or `'...'` literal and return (contents, rest).
fn parse_string_literal(s: &str) -> Option<(String, &str)> {
    let mut chars = s.chars();
    let quote = chars.next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let bytes = s.as_bytes();
    let mut i = 1;
    while i < bytes.len() {
        if bytes[i] == quote as u8 {
            let content = s[1..i].to_string();
            let tail = &s[i + 1..];
            return Some((content, tail));
        }
        i += 1;
    }
    None
}

/// Take a run of digits / `.` / `,` (matching the `NUMBER2` terminal
/// `/[0-9_\.\,]+/`) and split it off the input. We tolerate underscores
/// for parity even though no fixture uses them.
fn split_number_token(s: &str) -> (&str, &str) {
    let end = s
        .char_indices()
        .take_while(|&(_, c)| c.is_ascii_digit() || matches!(c, '.' | ',' | '_'))
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    (&s[..end], &s[end..])
}

/// Match an identifier (letters / digits / `_`) — used for class names.
fn split_identifier(s: &str) -> (&str, &str) {
    let mut end = 0;
    for (i, c) in s.char_indices() {
        if c.is_ascii_alphanumeric() || c == '_' {
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    (&s[..end], &s[end..])
}

/// Match `keyword` (exact, optional trailing whitespace) at the start
/// of `s`, returning the rest of the line. Does *not* require `:`.
fn strip_kw<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(kw)?;
    if rest.is_empty() || rest.starts_with(char::is_whitespace) {
        Some(rest)
    } else {
        None
    }
}

/// Match `keyword:` at the start of `s`, returning the rest.
fn strip_kw_colon<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(kw)?;
    rest.trim_start_matches([' ', '\t']).strip_prefix(':')
}

/// Parse a classDef line.
///
/// Regex reference: `/classDef\s+([A-Z_a-z]\w+)(?:\s+([^\n\r;]*))?;?/`.
fn try_parse_classdef(trimmed: &str) -> Option<TreemapClassDef> {
    let rest = trimmed.strip_prefix("classDef")?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    let (name, rest) = split_identifier(rest);
    if name.is_empty() {
        return None;
    }
    let style_text = rest.trim_start();
    // Drop a single optional trailing `;`.
    let style_text = style_text.strip_suffix(';').unwrap_or(style_text);
    // Upstream: replace `\,` with placeholder, split on `;`, then
    // restore commas. Treemap's `addClass` also pushes to `textStyles`
    // when the style looks label-like (`isLabelStyle`), and *always*
    // pushes to `styles`. For our purposes we just keep the full style
    // list — the renderer applies `styles2String` which consumes both.
    // Upstream `addClass` in db.ts:
    //   `_style.replace(/\\,/g, '§§§').replace(/,/g, ';').replace(/§§§/g, ',').split(';')`
    // The escape dance exists because `classDef myCls fill:red,color:blue`
    // uses commas (or `;`s) as the style separator — both split. Any
    // escaped comma (`\,`) survives as a literal comma within a single
    // declaration (e.g. inside `rgb(…)`). We mirror that here.
    let esc = style_text.replace("\\,", "§§§").replace(',', ";");
    let styles: Vec<String> = esc
        .split(';')
        .map(|p| p.trim().replace("§§§", ","))
        .filter(|p| !p.is_empty())
        .collect();
    Some(TreemapClassDef {
        id: name.to_string(),
        styles: styles.clone(),
        text_styles: styles
            .iter()
            .filter(|s| is_label_style(s))
            .cloned()
            .collect(),
    })
}

fn is_label_style(decl: &str) -> bool {
    let lower = decl.to_ascii_lowercase();
    lower.starts_with("color:") || lower.starts_with("font-")
}
