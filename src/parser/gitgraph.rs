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
//! `merge` is supported (commit kind `Merge` with two parents and an
//! auto-generated `{seq}-{hex7}` id, mirroring upstream's `getID()`).
//! `cherry-pick` is recognised but still bails out as `Unsupported`.

use crate::error::{MermaidError, Result};
use crate::model::gitgraph::{
    Branch, Commit, CommitKind, GitGraphConfig, GitGraphDiagram, Orientation,
};
use crate::model::DiagramMeta;

/// Mulberry32 PRNG — exact port of the deterministic PRNG used by the
/// reference test harness (`tests/support/generate_ref.mjs`). It seeds
/// `Math.random` so that `random({length:7})` (used by upstream's
/// `gitGraphAst.merge`) produces stable hex ids across runs. We mirror
/// the same sequence here so our parser produces byte-identical merge
/// commit ids.
pub(crate) struct GitGraphPrng {
    state: u32,
}

impl GitGraphPrng {
    pub fn new() -> Self {
        Self { state: 0x12345678 }
    }
    /// Returns a value in `[0.0, 1.0)` — same arithmetic as the JS
    /// shim: `((t ^ (t >>> 14)) >>> 0) / 4294967296`.
    fn next_f64(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x6d2b79f5);
        let mut t: u32 = self.state;
        // t = Math.imul(t ^ (t >>> 15), 1 | t)
        let a = (t ^ (t >> 15)) as i32;
        let b = (1u32 | t) as i32;
        t = (a.wrapping_mul(b)) as u32;
        // t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
        let a2 = (t ^ (t >> 7)) as i32;
        let b2 = (61u32 | t) as i32;
        let m2 = (a2.wrapping_mul(b2)) as u32;
        t = t.wrapping_add(m2) ^ t;
        // ((t ^ (t >>> 14)) >>> 0) / 4294967296
        ((t ^ (t >> 14)) as f64) / 4294967296.0
    }
    /// Mirrors `random({length})` → 7 hex chars by default.
    pub fn make_hex(&mut self, length: usize) -> String {
        const CHARS: &[u8] = b"0123456789abcdef";
        let mut out = String::with_capacity(length);
        for _ in 0..length {
            // Math.floor(rand * 16) — JS `floor` on positive [0,16) is plain truncate.
            let idx = (self.next_f64() * 16.0).floor() as usize;
            out.push(CHARS[idx.min(15)] as char);
        }
        out
    }
}

pub fn parse(source: &str) -> Result<GitGraphDiagram> {
    let FrontmatterData {
        title,
        theme: theme_name_fm,
        rotate_commit_label: fm_rotate,
        show_branches: fm_show_branches,
        show_commit_label: fm_show_commit_label,
        main_branch_name,
        main_branch_order,
        body,
    } = strip_frontmatter(source);
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

    // Frontmatter is the lower-priority source; init directive overrides it.
    if let Some(r) = fm_rotate {
        diagram.config.rotate_commit_label = r;
    }
    if let Some(r) = rotate_override {
        diagram.config.rotate_commit_label = r;
    }
    if let Some(b) = fm_show_branches {
        diagram.config.show_branches = b;
    }
    if let Some(b) = fm_show_commit_label {
        diagram.config.show_commit_label = b;
    }

    let main_name = main_branch_name.unwrap_or_else(|| "main".to_string());
    diagram.branches.push(Branch {
        name: main_name.clone(),
        order: main_branch_order,
    });

    let mut current_branch = main_name.clone();
    let mut branch_heads: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();
    branch_heads.insert(main_name.clone(), None);
    let mut head: Option<String> = None;
    let mut seq: usize = 0;
    let mut prng = GitGraphPrng::new();

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
            // Mirrors upstream `id: id ? id : state.records.seq + '-' + getID()`.
            // Same JS short-circuit as merge: PRNG is consumed only when
            // no explicit id was supplied.
            let id = match id {
                Some(s) => s,
                None => format!("{seq}-{}", prng.make_hex(7)),
            };
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
        } else if let Some(rest) = strip_keyword(line, "checkout")
            .or_else(|| strip_keyword(line, "switch"))
        {
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
        } else if let Some(rest) = strip_keyword(line, "merge") {
            // Syntax: `merge <branchName> [id: "..."] [tag: "..."] [type: REVERSE|HIGHLIGHT]`
            let (other_branch, args) = take_word(rest.trim_start());
            if other_branch.is_empty() {
                return Err(MermaidError::Parse {
                    line: 0,
                    col: 0,
                    message: "merge requires a branch name".into(),
                });
            }
            let other_head = branch_heads.get(&other_branch).cloned().flatten();
            let other_head = match other_head {
                Some(h) => h,
                None => {
                    return Err(MermaidError::Parse {
                        line: 0,
                        col: 0,
                        message: format!("merge: branch '{other_branch}' has no commits"),
                    });
                }
            };
            let (custom_id, custom_type, tags) = parse_merge_args(args)?;
            let has_custom_id = custom_id.is_some();
            // ID generation mirrors upstream: `customId || \`${seq}-${getID()}\``
            // — JS short-circuits, so the PRNG is consumed only when no
            // custom id is supplied. Mirroring this ordering is required
            // so subsequent cherry-pick / merge commits land on the
            // exact hex sequence produced by the reference shim.
            let id = match custom_id {
                Some(c) => c,
                None => format!("{seq}-{}", prng.make_hex(7)),
            };
            let mut parents: Vec<String> = head.iter().cloned().collect();
            parents.push(other_head);
            let commit = Commit {
                id: id.clone(),
                seq,
                kind: CommitKind::Merge,
                custom_type,
                custom_id: has_custom_id,
                tags,
                parents,
                branch: current_branch.clone(),
                message: format!("merged branch {other_branch} into {current_branch}"),
            };
            seq += 1;
            head = Some(id.clone());
            branch_heads.insert(current_branch.clone(), Some(id));
            diagram.commits.push(commit);
        } else if let Some(rest) = strip_keyword(line, "cherry-pick") {
            let (source_id, parent, tags, tag_was_set) = parse_cherrypick_args(rest)?;
            // Mirrors upstream's gitGraphAst.cherryPick: emits a new
            // CHERRY_PICK commit on the current branch with parents
            // `[currentHead, sourceId]`. The auto-id consumes 7 PRNG
            // draws even though we always end up using the auto value.
            let auto_id = format!("{seq}-{}", prng.make_hex(7));
            let id = auto_id;
            let mut parents: Vec<String> = head.iter().cloned().collect();
            // The source commit must already exist; if not, fall back
            // to no extra parent (we still emit a commit so the rest of
            // the diagram parses, mirroring upstream's softer fallback
            // when running outside the strict lint mode).
            if let Some(src) = diagram.commits.iter().find(|c| c.id == source_id) {
                parents.push(src.id.clone());
            } else {
                return Err(MermaidError::Parse {
                    line: 0,
                    col: 0,
                    message: format!("cherry-pick: unknown source commit '{source_id}'"),
                });
            }
            // Default tag mirrors upstream: when no `tag:` was passed
            // at all, use `cherry-pick:<id>` (or
            // `cherry-pick:<id>|parent:<parent>` for merge sources).
            // When `tag:` was set (even to ""), upstream's
            // `tags.filter(Boolean)` collapses to an empty list, which
            // we mirror by skipping the default entirely.
            let resolved_tags = if !tag_was_set {
                let src = diagram.commits.iter().find(|c| c.id == source_id);
                let suffix = if let (Some(s), Some(p)) = (src, parent.as_ref()) {
                    if matches!(s.kind, CommitKind::Merge) {
                        format!("|parent:{}", p)
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };
                vec![format!("cherry-pick:{}{}", source_id, suffix)]
            } else {
                tags
            };
            let commit = Commit {
                id: id.clone(),
                seq,
                kind: CommitKind::CherryPick,
                custom_type: None,
                custom_id: false,
                tags: resolved_tags,
                parents,
                branch: current_branch.clone(),
                message: format!("cherry-picked into {current_branch}"),
            };
            seq += 1;
            head = Some(id.clone());
            branch_heads.insert(current_branch.clone(), Some(id));
            diagram.commits.push(commit);
        } else {
            // Unknown statement — skip for now.
        }
    }

    Ok(diagram)
}

struct FrontmatterData {
    title: Option<String>,
    theme: Option<String>,
    rotate_commit_label: Option<bool>,
    show_branches: Option<bool>,
    show_commit_label: Option<bool>,
    main_branch_name: Option<String>,
    main_branch_order: Option<i64>,
    body: String,
}

/// Parse the optional `---` frontmatter block. Recognises:
///   - `title: ...`
///   - `config: theme: ...`
///   - `config: gitGraph: rotateCommitLabel: bool`
///   - `config: gitGraph: mainBranchName: ...`
///   - `config: gitGraph: mainBranchOrder: N`
fn strip_frontmatter(source: &str) -> FrontmatterData {
    let trimmed = source.trim_start_matches('\u{feff}');
    let trimmed = trimmed.trim_start();
    let empty = FrontmatterData {
        title: None,
        theme: None,
        rotate_commit_label: None,
        show_branches: None,
        show_commit_label: None,
        main_branch_name: None,
        main_branch_order: None,
        body: source.to_string(),
    };
    if !trimmed.starts_with("---") {
        return empty;
    }
    let after_open = match trimmed.strip_prefix("---") {
        Some(s) => s,
        None => return empty,
    };
    let after_open = after_open.trim_start_matches('\n');
    let close_idx = match after_open.find("\n---") {
        Some(i) => i,
        None => return empty,
    };
    let yaml = &after_open[..close_idx];
    let after_close = &after_open[close_idx + 4..];
    let after_close = after_close.trim_start_matches('\n');

    let mut title = None;
    let mut theme = None;
    let mut rotate: Option<bool> = None;
    let mut show_branches: Option<bool> = None;
    let mut show_commit_label: Option<bool> = None;
    let mut main_branch_name: Option<String> = None;
    let mut main_branch_order: Option<i64> = None;
    let mut config_indent: Option<usize> = None;
    let mut gitgraph_indent: Option<usize> = None;
    for raw in yaml.lines() {
        if raw.trim().is_empty() {
            continue;
        }
        let indent = raw.chars().take_while(|c| *c == ' ').count();
        let trimmed_line = raw.trim_end().trim_start();
        if let Some(gi) = gitgraph_indent {
            if indent <= gi {
                gitgraph_indent = None;
            }
        }
        if let Some(ci) = config_indent {
            if indent <= ci {
                config_indent = None;
            }
        }
        if trimmed_line.starts_with("title:") {
            title = Some(trimmed_line["title:".len()..].trim().trim_matches('"').to_string());
        } else if trimmed_line.starts_with("config:") {
            config_indent = Some(indent);
        } else if config_indent.is_some() && trimmed_line.starts_with("theme:") {
            theme = Some(
                trimmed_line["theme:".len()..]
                    .trim()
                    .trim_matches('"')
                    .to_string(),
            );
        } else if config_indent.is_some() && trimmed_line.starts_with("gitGraph:") {
            gitgraph_indent = Some(indent);
        } else if gitgraph_indent.is_some() && trimmed_line.starts_with("rotateCommitLabel:") {
            let v = trimmed_line["rotateCommitLabel:".len()..]
                .trim()
                .trim_matches('"');
            if v == "true" {
                rotate = Some(true);
            } else if v == "false" {
                rotate = Some(false);
            }
        } else if gitgraph_indent.is_some() && trimmed_line.starts_with("showBranches:") {
            let v = trimmed_line["showBranches:".len()..].trim().trim_matches('"');
            if v == "true" {
                show_branches = Some(true);
            } else if v == "false" {
                show_branches = Some(false);
            }
        } else if gitgraph_indent.is_some() && trimmed_line.starts_with("showCommitLabel:") {
            let v = trimmed_line["showCommitLabel:".len()..].trim().trim_matches('"');
            if v == "true" {
                show_commit_label = Some(true);
            } else if v == "false" {
                show_commit_label = Some(false);
            }
        } else if gitgraph_indent.is_some() && trimmed_line.starts_with("mainBranchName:") {
            let v = trimmed_line["mainBranchName:".len()..]
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            if !v.is_empty() {
                main_branch_name = Some(v);
            }
        } else if gitgraph_indent.is_some() && trimmed_line.starts_with("mainBranchOrder:") {
            let v = trimmed_line["mainBranchOrder:".len()..].trim().trim_matches('"');
            if let Ok(n) = v.parse::<i64>() {
                main_branch_order = Some(n);
            }
        }
    }

    FrontmatterData {
        title,
        theme,
        rotate_commit_label: rotate,
        show_branches,
        show_commit_label,
        main_branch_name,
        main_branch_order,
        body: after_close.to_string(),
    }
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

/// Parse the trailing arg list of a `cherry-pick` statement.
/// Returns `(source_id, parent_id, tags, tag_was_set)`. The boolean
/// distinguishes "user wrote `tag:`" from "user did not", because
/// upstream suppresses the default `cherry-pick:<id>` tag whenever any
/// `tag:` was supplied (even when its value is empty).
fn parse_cherrypick_args(
    s: &str,
) -> Result<(String, Option<String>, Vec<String>, bool)> {
    let mut id: Option<String> = None;
    let mut parent: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();
    let mut tag_was_set = false;
    let mut rem = s.trim();
    while !rem.is_empty() {
        if let Some(after) = rem.strip_prefix("id:") {
            let (val, next) = take_quoted_or_word(after.trim_start());
            id = Some(val);
            rem = next.trim_start();
        } else if let Some(after) = rem.strip_prefix("parent:") {
            let (val, next) = take_quoted_or_word(after.trim_start());
            parent = Some(val);
            rem = next.trim_start();
        } else if let Some(after) = rem.strip_prefix("tag:") {
            let (val, next) = take_quoted_or_word(after.trim_start());
            tag_was_set = true;
            if !val.is_empty() {
                tags.push(val);
            }
            rem = next.trim_start();
        } else {
            let mut chars = rem.chars();
            chars.next();
            rem = chars.as_str().trim_start();
        }
    }
    let id = id.ok_or_else(|| MermaidError::Parse {
        line: 0,
        col: 0,
        message: "cherry-pick requires id:".into(),
    })?;
    Ok((id, parent, tags, tag_was_set))
}

/// Strip the leading word (whitespace-delimited) and return it +
/// remainder. Quotes are not handled — branch names in `merge X` are
/// always bare identifiers per upstream's grammar.
fn take_word(s: &str) -> (String, &str) {
    let s = s.trim_start();
    let end = s
        .find(|c: char| c.is_whitespace())
        .unwrap_or(s.len());
    (s[..end].to_string(), &s[end..])
}

/// Parse the trailing arg list of a `merge` statement. Recognises
/// `id:`, `tag:`, `type:` in any order. Return `(custom_id, custom_type, tags)`.
fn parse_merge_args(s: &str) -> Result<(Option<String>, Option<CommitKind>, Vec<String>)> {
    let mut id: Option<String> = None;
    let mut custom_type: Option<CommitKind> = None;
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
            custom_type = match token {
                "REVERSE" => Some(CommitKind::Reverse),
                "HIGHLIGHT" => Some(CommitKind::Highlight),
                _ => None,
            };
            rem = after[token.len()..].trim_start();
        } else if let Some(after) = rem.strip_prefix("tag:") {
            let (val, next) = take_quoted_or_word(after.trim_start());
            tags.push(val);
            rem = next.trim_start();
        } else {
            // Skip a single char to make progress on unknown content.
            let mut chars = rem.chars();
            chars.next();
            rem = chars.as_str().trim_start();
        }
    }
    Ok((id, custom_type, tags))
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
