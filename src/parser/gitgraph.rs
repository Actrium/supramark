//! gitGraph parser. Hand-rolled tokenizer over the line-oriented
//! mermaid `gitGraph` source. Mirrors the subset of upstream
//! `gitGraphAst.ts` we need for byte-exact rendering of the simple
//! linear/single-branch fixtures.
//!
//! Scope of this initial port:
//! - frontmatter (`title`)
//! - `%%{init:...}%%` directive (best-effort: theme + rotateCommitLabel
//!   are surfaced; rest of `themeVariables` is consumed by the global
//!   preprocess layer.)
//! - `gitGraph` header with optional `LR|TB|BT` orientation.
//! - `commit id: "X" type: NORMAL|REVERSE|HIGHLIGHT tag: "v"`
//! - `branch <name>`, `checkout <name>`
//!
//! `merge` and `cherry-pick` are recognised but bail out as
//! `Unsupported` — those fixtures sit in `tests/known_ignored.txt`.

use crate::error::{MermaidError, Result};
use crate::model::gitgraph::{
    Branch, Commit, CommitKind, GitGraphConfig, GitGraphDiagram, Orientation,
};
use crate::model::DiagramMeta;

pub fn parse(source: &str) -> Result<GitGraphDiagram> {
    let (title, theme_name_fm, body) = strip_frontmatter(source);
    let (theme_name_dir, rotate_override, body, has_init) = strip_init_directives(&body);

    let mut diagram = GitGraphDiagram {
        meta: DiagramMeta {
            title,
            acc_title: None,
            acc_descr: None,
        },
        orientation: Orientation::LR,
        config: GitGraphConfig::defaults(),
        branches: Vec::new(),
        commits: Vec::new(),
        theme_name: theme_name_dir.or(theme_name_fm),
        has_init_directive: has_init,
    };

    if let Some(r) = rotate_override {
        diagram.config.rotate_commit_label = r;
    }

    diagram.branches.push(Branch {
        name: "main".to_string(),
        order: None,
    });

    let mut current_branch = "main".to_string();
    let mut branch_heads: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();
    branch_heads.insert("main".into(), None);
    let mut head: Option<String> = None;
    let mut seq: usize = 0;

    let mut header_seen = false;
    for raw_line in body.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("%%") {
            continue;
        }
        if !header_seen {
            if let Some(rest) = strip_keyword(line, "gitGraph") {
                let rest = rest.trim().trim_start_matches(':').trim();
                if rest.starts_with("LR") {
                    diagram.orientation = Orientation::LR;
                } else if rest.starts_with("TB") {
                    diagram.orientation = Orientation::TB;
                } else if rest.starts_with("BT") {
                    diagram.orientation = Orientation::BT;
                }
                header_seen = true;
                continue;
            }
            continue;
        }
        if let Some(rest) = strip_keyword(line, "commit") {
            let (id, kind, tags) = parse_commit_args(rest)?;
            let id = id.unwrap_or_else(|| format!("{}-noid", seq));
            let parents: Vec<String> = head.iter().cloned().collect();
            let commit = Commit {
                id: id.clone(),
                seq,
                kind,
                custom_type: None,
                custom_id: false,
                tags,
                parents,
                branch: current_branch.clone(),
                message: String::new(),
            };
            seq += 1;
            head = Some(id.clone());
            branch_heads.insert(current_branch.clone(), Some(id.clone()));
            diagram.commits.push(commit);
        } else if let Some(rest) = strip_keyword(line, "branch") {
            let name = parse_ident(rest);
            let order = parse_order_after(rest);
            if !diagram.branches.iter().any(|b| b.name == name) {
                diagram.branches.push(Branch {
                    name: name.clone(),
                    order,
                });
            }
            branch_heads.entry(name.clone()).or_insert(head.clone());
            current_branch = name;
        } else if let Some(rest) = strip_keyword(line, "checkout") {
            let name = parse_ident(rest);
            if branch_heads.contains_key(&name) {
                head = branch_heads.get(&name).cloned().flatten();
                current_branch = name;
            } else {
                return Err(MermaidError::Parse {
                    line: 0,
                    col: 0,
                    message: format!("checkout to unknown branch '{name}'"),
                });
            }
        } else if strip_keyword(line, "merge").is_some() {
            return Err(MermaidError::Unsupported(
                "gitGraph: 'merge' not yet supported in minimal port".into(),
            ));
        } else if strip_keyword(line, "cherry-pick").is_some() {
            return Err(MermaidError::Unsupported(
                "gitGraph: 'cherry-pick' not yet supported in minimal port".into(),
            ));
        } else {
            // Unknown statement — skip for now.
        }
    }

    Ok(diagram)
}

/// Hand-rolled frontmatter strip. Returns (title, theme, body).
/// Only recognises `title:` and `config: { theme: ... }` at the top
/// level — enough for the gitGraph fixtures that opt into frontmatter.
fn strip_frontmatter(source: &str) -> (Option<String>, Option<String>, String) {
    let trimmed = source.trim_start_matches('\u{feff}');
    let trimmed = trimmed.trim_start();
    if !trimmed.starts_with("---") {
        return (None, None, source.to_string());
    }
    let after_open = match trimmed.strip_prefix("---") {
        Some(s) => s,
        None => return (None, None, source.to_string()),
    };
    let after_open = after_open.trim_start_matches('\n');
    let close_idx = match after_open.find("\n---") {
        Some(i) => i,
        None => return (None, None, source.to_string()),
    };
    let yaml = &after_open[..close_idx];
    let after_close = &after_open[close_idx + 4..];
    let after_close = after_close.trim_start_matches('\n');

    let mut title = None;
    let mut theme = None;
    let mut in_config = false;
    for raw in yaml.lines() {
        let line = raw.trim_end();
        if line.starts_with("title:") {
            title = Some(line["title:".len()..].trim().trim_matches('"').to_string());
        } else if line.starts_with("config:") {
            in_config = true;
        } else if in_config && line.starts_with("  theme:") {
            theme = Some(
                line["  theme:".len()..]
                    .trim()
                    .trim_matches('"')
                    .to_string(),
            );
        } else if !line.starts_with(' ') && !line.is_empty() {
            in_config = false;
        }
    }

    (title, theme, after_close.to_string())
}

/// Strip `%%{init: {...}}%%` blocks. Returns (theme override, rotate
/// override, body, had-any-init). We don't need a real JSON parser here
/// for the byte-exact subset; a simple key-search is enough.
fn strip_init_directives(source: &str) -> (Option<String>, Option<bool>, String, bool) {
    let mut theme: Option<String> = None;
    let mut rotate: Option<bool> = None;
    let mut had_any = false;
    let mut out = String::with_capacity(source.len());
    let mut s = source;
    while let Some(idx) = s.find("%%{") {
        out.push_str(&s[..idx]);
        if let Some(end) = s[idx..].find("}%%") {
            had_any = true;
            let block = &s[idx..idx + end + 3];
            // Inspect the directive payload.
            if let Some(t) = scan_value(block, "'theme'").or_else(|| scan_value(block, "\"theme\"")) {
                theme = Some(t);
            }
            if scan_value(block, "'rotateCommitLabel'")
                .or_else(|| scan_value(block, "\"rotateCommitLabel\""))
                .as_deref()
                == Some("true")
            {
                rotate = Some(true);
            } else if scan_value(block, "'rotateCommitLabel'")
                .or_else(|| scan_value(block, "\"rotateCommitLabel\""))
                .as_deref()
                == Some("false")
            {
                rotate = Some(false);
            }
            s = &s[idx + end + 3..];
        } else {
            out.push_str(&s[idx..]);
            s = "";
            break;
        }
    }
    out.push_str(s);
    (theme, rotate, out, had_any)
}

fn scan_value(block: &str, key: &str) -> Option<String> {
    let i = block.find(key)?;
    let rest = &block[i + key.len()..];
    let after_colon = rest.find(':')?;
    let mut value_part = rest[after_colon + 1..].trim_start().to_string();
    // Trim trailing comma/brace/whitespace.
    let end = value_part
        .find(|c: char| c == ',' || c == '}' || c == '\n')
        .unwrap_or(value_part.len());
    value_part.truncate(end);
    let v = value_part
        .trim()
        .trim_matches('\'')
        .trim_matches('"')
        .to_string();
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

fn strip_keyword<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    if let Some(rest) = s.strip_prefix(kw) {
        if rest.is_empty() || rest.starts_with(char::is_whitespace) || rest.starts_with(':') {
            return Some(rest);
        }
    }
    None
}

fn parse_ident(s: &str) -> String {
    s.trim()
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches(|c: char| c == '"' || c == '\'')
        .to_string()
}

fn parse_order_after(s: &str) -> Option<i64> {
    if let Some(idx) = s.find("order:") {
        let after = &s[idx + 6..];
        let token = after.trim().split_whitespace().next().unwrap_or("");
        token.parse::<i64>().ok()
    } else {
        None
    }
}

fn parse_commit_args(s: &str) -> Result<(Option<String>, CommitKind, Vec<String>)> {
    let mut id: Option<String> = None;
    let mut kind = CommitKind::Normal;
    let mut tags: Vec<String> = Vec::new();

    let mut rem = s.trim();
    while !rem.is_empty() {
        if let Some(after) = rem.strip_prefix("id:") {
            let (val, next) = take_quoted_or_word(after.trim_start());
            id = Some(val);
            rem = next.trim_start();
        } else if let Some(after) = rem.strip_prefix("type:") {
            let after = after.trim_start();
            let token = after.split_whitespace().next().unwrap_or("");
            kind = match token {
                "REVERSE" => CommitKind::Reverse,
                "HIGHLIGHT" => CommitKind::Highlight,
                _ => CommitKind::Normal,
            };
            rem = after[token.len()..].trim_start();
        } else if let Some(after) = rem.strip_prefix("tag:") {
            let (val, next) = take_quoted_or_word(after.trim_start());
            tags.push(val);
            rem = next.trim_start();
        } else if let Some(after) = rem.strip_prefix("msg:") {
            let (_val, next) = take_quoted_or_word(after.trim_start());
            rem = next.trim_start();
        } else {
            let mut chars = rem.chars();
            chars.next();
            rem = chars.as_str().trim_start();
        }
    }
    Ok((id, kind, tags))
}

fn take_quoted_or_word(s: &str) -> (String, &str) {
    let s = s.trim_start();
    if let Some(rest) = s.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            return (rest[..end].to_string(), &rest[end + 1..]);
        }
    }
    let token: String = s
        .chars()
        .take_while(|c| !c.is_whitespace())
        .collect::<String>();
    let n = token.len();
    (token, &s[n..])
}
