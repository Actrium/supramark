//! Flowchart parser — hand-rolled recursive descent that mirrors the
//! upstream `flow.jison` grammar closely enough for the common
//! subset of flowchart syntax.
//!
//! Upstream reference:
//! `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/flowchart/parser/flow.jison`
//! `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/flowchart/flowDb.ts`
//!
//! The parser operates line-by-line, but understands multi-line
//! subgraph blocks. Within a line it uses a small char-cursor that
//! can peek/pop tokens, matching upstream's lexer states loosely.

use crate::config::directive;
use crate::config::frontmatter;
use crate::error::{MermaidError, Result};
use crate::model::flowchart::{
    ArrowType, ClassDef, Direction, Edge, EdgeStroke, FlowchartDiagram, Label, LinkStyle, Subgraph,
    Vertex,
};

/// Parse a flowchart / graph source.
pub fn parse(source: &str) -> Result<FlowchartDiagram> {
    // --- preprocess ---------------------------------------------------
    let (fm, body) = frontmatter::parse_frontmatter(source);
    // Capture init directives BEFORE stripping them out of the body, so
    // we can lift `flowchart.htmlLabels` and other CSS-affecting flags.
    let init_cfgs = directive::parse_directives(body);
    let body = directive::remove_directives(body);

    let mut diag = FlowchartDiagram::default();
    if let Some(fm) = fm {
        diag.meta.title = fm.title.clone();
        if let Some(cfg) = fm.config.as_ref() {
            if let Some(t) = cfg.theme.as_ref() {
                diag.theme_override = Some(t.clone());
            }
            if let Some(b) = cfg.html_labels {
                diag.html_labels = Some(b);
            }
            if let Some(fcc) = cfg.flowchart.as_ref() {
                if let Some(b) = fcc.html_labels {
                    diag.html_labels = Some(b);
                }
                if let Some(n) = fcc.node_spacing {
                    diag.node_spacing = Some(n);
                }
                if let Some(n) = fcc.rank_spacing {
                    diag.rank_spacing = Some(n);
                }
                if let Some(c) = &fcc.curve {
                    diag.curve = Some(c.clone());
                }
                if let Some(b) = fcc.inherit_dir {
                    diag.inherit_dir = b;
                }
            }
        }
    }
    // `%%{init: {flowchart: {htmlLabels: false}}}%%` overrides frontmatter.
    for cfg in &init_cfgs {
        if let Some(b) = cfg.html_labels {
            diag.html_labels = Some(b);
        }
        if let Some(fcc) = cfg.flowchart.as_ref() {
            if let Some(b) = fcc.html_labels {
                diag.html_labels = Some(b);
            }
            if let Some(n) = fcc.node_spacing {
                diag.node_spacing = Some(n);
            }
            if let Some(n) = fcc.rank_spacing {
                diag.rank_spacing = Some(n);
            }
            if let Some(c) = &fcc.curve {
                diag.curve = Some(c.clone());
            }
            if let Some(b) = fcc.inherit_dir {
                diag.inherit_dir = b;
            }
        }
    }

    // --- normalise body lines ----------------------------------------
    // Strip whole-line `%%` comments, keep everything else (we DO want
    // to preserve `%%{init:...}%%` directives content, but they were
    // already removed above).
    // Also expand inline `;` separators (upstream treats `;` like `\n`),
    // splitting outside quoted strings to preserve e.g. subgraph titles.
    let mut lines: Vec<String> = Vec::new();
    for raw in body.split('\n') {
        let t = raw.trim_end_matches('\r');
        // Skip whole-line `%%` comments.
        if t.trim_start().starts_with("%%") && !t.trim_start().starts_with("%%{") {
            continue;
        }
        // Expand `;` as statement separator (outside of `"..."` quotes).
        let segments = split_semicolons_outside_quotes(t);
        for seg in segments {
            lines.push(seg);
        }
    }

    // --- header: `flowchart TD` / `graph LR` / `flowchart-elk RL` ----
    let mut i = 0usize;
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }
    if i >= lines.len() {
        return Err(MermaidError::Parse {
            line: 0,
            col: 0,
            message: "flowchart: empty source".into(),
        });
    }
    let header = lines[i].trim();
    let is_v2;
    let mut dir = Direction::TB;
    let header_keyword;
    if let Some(rest) = header.strip_prefix("flowchart-elk") {
        is_v2 = true;
        header_keyword = "flowchart-elk";
        let rest = rest.trim();
        if !rest.is_empty() {
            if let Some(d) = Direction::parse(rest) {
                dir = d;
            }
        }
    } else if let Some(rest) = header.strip_prefix("flowchart") {
        is_v2 = true;
        header_keyword = "flowchart";
        let rest = rest.trim();
        if !rest.is_empty() {
            if let Some(d) = Direction::parse(rest) {
                dir = d;
            }
        }
    } else if let Some(rest) = header.strip_prefix("graph") {
        is_v2 = false;
        header_keyword = "graph";
        let rest = rest.trim();
        if !rest.is_empty() {
            if let Some(d) = Direction::parse(rest) {
                dir = d;
            }
        }
    } else {
        return Err(MermaidError::Parse {
            line: 0,
            col: 0,
            message: format!(
                "flowchart: expected `flowchart` or `graph` header, got `{}`",
                header
            ),
        });
    }
    diag.is_v2 = is_v2;
    diag.direction = dir;
    diag.header_keyword = header_keyword.to_string();
    i += 1;

    // --- body --------------------------------------------------------
    // We join continuation lines: the flowchart grammar treats `;` and
    // `\n` as statement separators, and a statement can stretch over
    // multiple lines when it ends mid-edge or inside a bracketed label.
    // We preserve newlines here and parse one logical statement at a
    // time, allowing the cursor to peek across lines when inside a
    // subgraph block.
    let mut parser = LineParser {
        lines,
        i,
        current_subgraph: Vec::new(),
        diag: &mut diag,
        acc_title: None,
        acc_descr: None,
        vertex_counter: 0,
    };
    parser.parse_body()?;

    // Post-process: renumber auto-generated subgraph IDs to match upstream's
    // bottom-up (post-order) numbering. Upstream's LR parser reduces inner
    // subgraphs before outer ones, so inner subgraphs get lower subCount values.
    // Our top-down parser encounters outer subgraphs first, giving them lower
    // order numbers. We fix this by doing a post-order traversal of the subgraph
    // tree and reassigning counters in that order.
    renumber_auto_subgraph_ids(&mut diag);

    Ok(diag)
}

struct LineParser<'a> {
    lines: Vec<String>,
    i: usize,
    /// Stack of subgraph ids currently being parsed (innermost last).
    current_subgraph: Vec<String>,
    diag: &'a mut FlowchartDiagram,
    acc_title: Option<String>,
    acc_descr: Option<String>,
    /// Global vertex counter — increments on every `ensure_vertex` call,
    /// even when the vertex already exists. Matches upstream's
    /// `flowDb.vertexCounter` behaviour so that node dom-ids like
    /// `flowchart-C-3` are produced correctly.
    vertex_counter: usize,
}

impl<'a> LineParser<'a> {
    fn parse_body(&mut self) -> Result<()> {
        while self.i < self.lines.len() {
            self.parse_line()?;
        }
        if let Some(t) = self.acc_title.take() {
            self.diag.meta.acc_title = Some(t);
        }
        if let Some(d) = self.acc_descr.take() {
            self.diag.meta.acc_descr = Some(d);
        }
        // Note: upstream keeps frontmatter `title:` and `accTitle:` as
        // separate fields. accTitle becomes the SVG `<title>` element
        // (a11y), while only the frontmatter title becomes the
        // `flowchartTitleText` rendered above the diagram. Do not
        // promote acc_title into meta.title here.
        Ok(())
    }

    fn current_line(&self) -> &str {
        &self.lines[self.i]
    }

    fn advance(&mut self) {
        self.i += 1;
    }

    fn parse_line(&mut self) -> Result<()> {
        let raw = self.current_line().to_string();
        let line = raw.trim();
        if line.is_empty() {
            self.advance();
            return Ok(());
        }

        // accTitle / accDescr
        if let Some(rest) = line.strip_prefix("accTitle") {
            let rest = rest.trim_start();
            if let Some(v) = rest.strip_prefix(':') {
                self.acc_title = Some(v.trim().to_string());
                self.advance();
                return Ok(());
            }
        }
        if let Some(rest) = line.strip_prefix("accDescr") {
            let rest = rest.trim_start();
            if let Some(v) = rest.strip_prefix(':') {
                self.acc_descr = Some(v.trim().to_string());
                self.advance();
                return Ok(());
            }
            // Multi-line form `accDescr { ... }`
            if rest.starts_with('{') {
                let mut collected = String::new();
                let mut s = rest.strip_prefix('{').unwrap().to_string();
                loop {
                    if let Some(end) = s.find('}') {
                        collected.push_str(&s[..end]);
                        self.advance();
                        break;
                    } else {
                        collected.push_str(&s);
                        collected.push('\n');
                        self.advance();
                        if self.i >= self.lines.len() {
                            break;
                        }
                        s = self.current_line().to_string();
                    }
                }
                self.acc_descr = Some(collected.trim().to_string());
                return Ok(());
            }
        }

        // `direction X` line.
        //
        // Inside a subgraph this overrides that subgraph's `dir`. At the
        // top level upstream's flowDb treats it as a no-op: the parser
        // emits a `{stmt:'dir', value:X}` AST node that only `addSubGraph`
        // consumes (via the `uniq` reducer in flowDb.ts). Since the root
        // document is never wrapped in a subgraph, top-level direction
        // tokens are silently dropped — the root `rankdir` stays whatever
        // the `flowchart <DIR>` header declared. Mirror that here so we
        // don't accidentally swap the outer dagre direction (cypress
        // fixture 248).
        if let Some(rest) = line.strip_prefix("direction") {
            let rest = rest.trim();
            if let Some(d) = Direction::parse(rest) {
                if let Some(sid) = self.current_subgraph.last().cloned() {
                    if let Some(sg) = self.diag.subgraphs.iter_mut().find(|s| s.id == sid) {
                        sg.dir = Some(d);
                    }
                }
                self.advance();
                return Ok(());
            }
        }

        // `subgraph`
        if line == "subgraph" || line.starts_with("subgraph ") || line.starts_with("subgraph\t") {
            return self.parse_subgraph_open(&raw);
        }
        if line == "end" || line.starts_with("end ") || line == "end;" {
            // Close current subgraph
            self.current_subgraph.pop();
            self.advance();
            return Ok(());
        }

        // classDef
        if let Some(rest) = line.strip_prefix("classDef ") {
            self.parse_class_def(rest.trim());
            self.advance();
            return Ok(());
        }

        // class  (`class id1,id2 className`)
        if let Some(rest) = line.strip_prefix("class ") {
            self.parse_class_stmt(rest.trim());
            self.advance();
            return Ok(());
        }

        // style
        if let Some(rest) = line.strip_prefix("style ") {
            self.parse_style_stmt(rest.trim());
            self.advance();
            return Ok(());
        }

        // linkStyle
        if let Some(rest) = line.strip_prefix("linkStyle ") {
            self.parse_link_style(rest.trim());
            self.advance();
            return Ok(());
        }

        // click — may span multiple lines when a tooltip/link string
        // contains a literal newline. Upstream's jison grammar treats
        // unclosed `"…` strings as errors, but in practice users embed
        // multi-line tooltips (e.g. demos/flowchart/18). To match the
        // forgiving behavior, fold subsequent raw lines into the click
        // statement until the quote count is balanced.
        if let Some(rest) = line.strip_prefix("click ") {
            let mut combined = rest.to_string();
            let mut q = combined.chars().filter(|&c| c == '"').count();
            while q % 2 == 1 {
                self.advance();
                if self.i >= self.lines.len() {
                    break;
                }
                let nxt = self.current_line().to_string();
                combined.push('\n');
                combined.push_str(&nxt);
                q += nxt.chars().filter(|&c| c == '"').count();
            }
            self.parse_click(combined.trim());
            self.advance();
            return Ok(());
        }

        // vertex statement / edge statement — may span multiple lines
        // (e.g. bracketed labels split over lines). We'll gather a
        // full "statement" by concatenating until we've closed all
        // brackets or hit a newline at balanced depth.
        let stmt = self.collect_statement();
        if stmt.trim().is_empty() {
            return Ok(());
        }
        // Split on `;` (statement separator) at depth 0.
        for piece in split_semis(&stmt) {
            let piece = piece.trim();
            if piece.is_empty() {
                continue;
            }
            // Edge-metadata binding: `<edge-id>@{ key: value, ... }` where
            // the id matches a previously declared `link_id` (e.g.
            // `L_A_B_0@{ animation: slow }`). Upstream's `addEdge` consumes
            // these as edge property bags — they MUST NOT spawn synthetic
            // nodes. Without this guard the bare-id form falls through to
            // `parse_vertex_statement` and gets registered as a rectangle
            // node (cypress fixtures 113, 196, 237).
            if self.try_consume_edge_meta(piece) {
                continue;
            }
            self.parse_vertex_statement(piece)?;
        }
        Ok(())
    }

    /// Detect `<edge-id>@{ ... }` lines that bind metadata to an existing
    /// edge and consume them silently. Returns `true` when the line was
    /// claimed (caller should skip the regular vertex parser).
    fn try_consume_edge_meta(&mut self, piece: &str) -> bool {
        // Quick reject: must contain `@{` and no link arrows after the
        // identifier portion. Anything that also has `-->` / `==>` / `--`
        // etc. is a real edge declaration and must keep flowing through
        // `parse_vertex_statement`.
        let at = match piece.find("@{") {
            Some(i) => i,
            None => return false,
        };
        let id = piece[..at].trim();
        if id.is_empty() {
            return false;
        }
        // Bare id only — reject anything with whitespace inside the id
        // portion (would suggest something like `A B@{...}` which is not
        // an edge-meta line).
        if id
            .chars()
            .any(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        {
            return false;
        }
        // The remainder after `@{` must close with `}` and have nothing
        // (other than whitespace) after the closing brace, otherwise this
        // is a more complex statement (e.g. `id@{...} --> other`).
        let rest = &piece[at + 2..];
        let close = match rest.find('}') {
            Some(i) => i,
            None => return false,
        };
        if !rest[close + 1..].trim().is_empty() {
            return false;
        }
        // Only claim the line if `id` matches a known edge id. Two forms
        // are recognised:
        //   1. an explicit `link_id` captured via `<id>@-->` syntax
        //      (stored on `Edge::id`).
        //   2. an auto-generated `L_<start>_<end>_<n>` form that the
        //      layout will assign — recompute it here from the parser's
        //      per-pair edge ordering so we can spot the binding before
        //      layout has run.
        let is_edge_id = self.diag.edges.iter().any(|e| e.id.as_deref() == Some(id))
            || edge_id_matches_synthetic(id, &self.diag.edges);
        if !is_edge_id {
            return false;
        }
        // Parse metadata body and extract `curve:` / `animation:` if present.
        let body = &rest[..close];
        let curve = parse_edge_meta_curve(body);
        let animation = parse_edge_meta_animation(body);
        if let Some(curve_type) = curve {
            let curve_type = curve_type.to_string();
            for e in &mut self.diag.edges {
                if e.id.as_deref() == Some(id) {
                    e.curve = Some(curve_type.clone());
                    break;
                }
            }
            if self.diag.edges.iter().all(|e| e.id.as_deref() != Some(id)) {
                let curve_type_clone = curve_type.clone();
                let idx = edge_id_matches_synthetic_index(id, &self.diag.edges);
                if let Some(i) = idx {
                    self.diag.edges[i].curve = Some(curve_type_clone);
                }
            }
        }
        if let Some(anim) = animation {
            let anim = anim.to_string();
            // First try matching by explicit edge id (assigned via `@-->`).
            let mut applied = false;
            for e in &mut self.diag.edges {
                if e.id.as_deref() == Some(id) {
                    e.animation = Some(anim.clone());
                    applied = true;
                    break;
                }
            }
            // Fall back to the synthetic `L_<start>_<end>_<n>` form.
            if !applied {
                if let Some(i) = edge_id_matches_synthetic_index(id, &self.diag.edges) {
                    self.diag.edges[i].animation = Some(anim);
                }
            }
        }
        true
    }

    /// Collect a statement that may span multiple physical lines.
    /// Consumes lines until bracket depth is balanced *and* the current
    /// line ends at depth 0. Returns the concatenated text.
    fn collect_statement(&mut self) -> String {
        let mut out = String::new();
        let mut depth_paren = 0i32;
        let mut depth_sq = 0i32;
        let mut depth_cu = 0i32;
        let mut in_str = false;
        let mut in_md = false;
        while self.i < self.lines.len() {
            let line = self.lines[self.i].clone();
            let mut chars = line.chars().peekable();
            while let Some(c) = chars.next() {
                out.push(c);
                if in_md {
                    // closed by `"
                    if c == '`' && chars.peek() == Some(&'"') {
                        in_md = false;
                        if let Some(n) = chars.next() {
                            out.push(n);
                        }
                    }
                    continue;
                }
                if in_str {
                    if c == '"' {
                        in_str = false;
                    }
                    continue;
                }
                match c {
                    '"' => {
                        // Check for `"` opener
                        if chars.peek() == Some(&'`') {
                            in_md = true;
                            if let Some(n) = chars.next() {
                                out.push(n);
                            }
                        } else {
                            in_str = true;
                        }
                    }
                    '(' => depth_paren += 1,
                    ')' => depth_paren -= 1,
                    '[' => depth_sq += 1,
                    ']' => depth_sq -= 1,
                    '{' => depth_cu += 1,
                    '}' => depth_cu -= 1,
                    _ => {}
                }
            }
            self.advance();
            if depth_paren <= 0 && depth_sq <= 0 && depth_cu <= 0 && !in_str && !in_md {
                // Continuation: when the next non-empty physical line begins
                // with an arrow operator (e.g. `--> b`), treat it as a
                // continuation of the current statement. Mirrors upstream's
                // jison grammar which accepts whitespace (incl. \n) between
                // `vertex` and the following `LINK vertex` segment, so:
                //   a{{...}}
                //   --> b{{...}}
                // parses as a single edge `a --> b` rather than a stray
                // edge with no source. Cypress fixture 160 relies on this.
                let mut peek_i = self.i;
                while peek_i < self.lines.len() && self.lines[peek_i].trim().is_empty() {
                    peek_i += 1;
                }
                if peek_i < self.lines.len()
                    && line_starts_with_arrow(self.lines[peek_i].trim_start())
                {
                    out.push('\n');
                    // Absorb any blank lines between us and the arrow line.
                    while self.i < peek_i {
                        out.push('\n');
                        self.i += 1;
                    }
                    continue;
                }
                break;
            }
            out.push('\n');
        }
        out
    }

    fn parse_subgraph_open(&mut self, raw: &str) -> Result<()> {
        // Formats:
        //   subgraph
        //   subgraph TITLE
        //   subgraph id
        //   subgraph id[Title]
        //   subgraph id ["Title"]
        // `after_kw` is the raw suffix after stripping "subgraph" (spaces preserved).
        // Used to detect upstream's "id contains whitespace → use auto-id" rule.
        let after_kw = raw.trim().trim_start_matches("subgraph");
        let line = after_kw.trim();
        let order = self.diag.subgraphs.len();
        let (id, title_opt) = if line.is_empty() {
            (format!("subGraph{}", order), None)
        } else {
            // Try to pick out `id[Title]` or `id [Title]`.
            if let Some(br) = line.find('[') {
                let id = line[..br].trim().to_string();
                let rest = &line[br..];
                // Title is inside balanced `[...]`
                if let Some(close) = rest.rfind(']') {
                    let title_text = &rest[1..close];
                    let label = parse_label_text(title_text);
                    if id.is_empty() {
                        (format!("subGraph{}", order), Some(label))
                    } else {
                        (id, Some(label))
                    }
                } else {
                    (line.to_string(), None)
                }
            } else if line.starts_with('"') && line.ends_with('"') && line.len() >= 2 {
                // Quoted string — upstream's grammar rule
                //   `subgraph SPACE textNoTags ...` matches a STR token whose
                // text is the unquoted body. The `addSubGraph` helper uses that
                // text for BOTH `_id` and `_title`. If the text contains
                // whitespace it is then forced to an auto-id; otherwise the
                // body becomes the cluster id (e.g. `subgraph "subbe"` → id "subbe").
                //
                // Markdown labels (`"`...`"`) carry the inner content WITHOUT
                // surrounding backticks as both the id and the rendered title.
                let inner = &line[1..line.len() - 1];
                let is_markdown =
                    inner.starts_with('`') && inner.ends_with('`') && inner.len() >= 2;
                let id_text = if is_markdown {
                    &inner[1..inner.len() - 1]
                } else {
                    inner
                };
                let label = if is_markdown {
                    Label::markdown(id_text)
                } else {
                    Label::string(inner)
                };
                if id_text.is_empty() || id_text.contains(|c: char| c.is_whitespace()) {
                    (format!("subGraph{}", order), Some(label))
                } else {
                    (id_text.to_string(), Some(label))
                }
            } else {
                // Maybe just a title without brackets — use as id+title.
                // Upstream jison grammar rule: `subgraph SPACE textNoTags NEWLINE...`.
                // One leading space is consumed as the SPACE separator token; any extra
                // leading whitespace becomes part of the textNoTags text. The upstream
                // check is `_id === _title && /\s/.exec(_title.text)` — if the token
                // text contains whitespace, the id is auto-generated instead.
                //
                // `subgraph main`  → one space separator, textNoTags.text = "main"  → no \s → id = "main"
                // `subgraph  main` → two spaces, textNoTags.text = " main" → has \s → auto-id
                let label = parse_label_text(line);
                // after_kw starts with the space(s) between "subgraph" and the title.
                // The first space is consumed as separator token; remaining chars form the
                // textNoTags token text. If that remainder contains whitespace, use auto-id.
                let after_sep: String = after_kw.chars().skip(1).collect();
                if after_sep.contains(|c: char| c.is_whitespace()) {
                    // Extra leading whitespace → upstream auto-generates the id
                    (format!("subGraph{}", order), Some(label))
                } else {
                    (line.to_string(), Some(label))
                }
            }
        };
        // Register subgraph
        let sg = Subgraph {
            id: id.clone(),
            title: title_opt,
            members: Vec::new(),
            children: Vec::new(),
            dir: None,
            order,
            classes: Vec::new(),
        };
        // If nested, register parent->child
        if let Some(parent) = self.current_subgraph.last().cloned() {
            if let Some(p) = self.diag.subgraphs.iter_mut().find(|s| s.id == parent) {
                p.children.push(id.clone());
            }
        }
        self.diag.subgraphs.push(sg);
        self.current_subgraph.push(id);
        self.advance();
        Ok(())
    }

    fn parse_class_def(&mut self, rest: &str) {
        // `classDef name1,name2 style1,style2`
        // Upstream allows comma-separated class names sharing the same styles.
        let (names, styles) = split_once_ws(rest);
        let styles: Vec<String> = styles
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        for name in names.split(',') {
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            let def = ClassDef {
                name: name.to_string(),
                styles: styles.clone(),
                text_styles: Vec::new(),
            };
            self.diag.class_defs.push(def);
        }
    }

    fn parse_class_stmt(&mut self, rest: &str) {
        // `class id1,id2 cls`
        //
        // Upstream mermaid only applies `class id cls` to existing vertices;
        // when the id refers to a custom edge id (`A name@-->B`) the class
        // is attached to the edge instead and surfaces as inline edge style
        // at render time. Creating a phantom vertex would otherwise render
        // an extra orphan default node (see cypress fixture 197).
        let (ids, cls) = split_once_ws(rest);
        for id in ids.split(',') {
            let id = id.trim();
            if id.is_empty() {
                continue;
            }
            // Edge-id match: attach the class to all matching edges.
            let mut applied_to_edge = false;
            for e in self.diag.edges.iter_mut() {
                if e.id.as_deref() == Some(id) {
                    e.classes.push(cls.to_string());
                    applied_to_edge = true;
                }
            }
            if applied_to_edge {
                continue;
            }
            // Upstream `setClass` does NOT auto-create vertices and does NOT
            // bump `vertexCounter`. Only attach the class when the vertex
            // already exists; mirrors upstream byte-exactly and keeps the
            // domId suffix counter in sync.
            if let Some(v) = self.diag.find_vertex_mut(id) {
                v.classes.push(cls.to_string());
                continue;
            }
            // Subgraph match: `class <subgraph-id> <className>` attaches
            // the class to the cluster `<g>` so the rendered DOM gains
            // `class="cluster <className>"` and any matching `classDef`
            // resolves to inline `style="…"` on the cluster rect.
            // See cypress fixture 143 (`class T Test`, `class T TestSub`).
            if let Some(sg) = self.diag.find_subgraph_mut(id) {
                sg.classes.push(cls.to_string());
            }
        }
    }

    fn parse_style_stmt(&mut self, rest: &str) {
        // `style id a:b,c:d`
        let (id, styles) = split_once_ws(rest);
        self.ensure_vertex(id);
        if let Some(v) = self.diag.find_vertex_mut(id) {
            for s in styles.split(',') {
                let s = s.trim();
                if !s.is_empty() {
                    v.styles.push(s.to_string());
                }
            }
        }
    }

    fn parse_link_style(&mut self, rest: &str) {
        // `linkStyle N,M stroke:...` or `linkStyle default ...`
        let (head, styles) = split_once_ws(rest);
        let is_default = head == "default";
        let indices: Vec<usize> = if is_default {
            Vec::new()
        } else {
            head.split(',')
                .filter_map(|s| s.trim().parse::<usize>().ok())
                .collect()
        };
        // Check for `interpolate X` sub-keyword
        let mut interpolate: Option<String> = None;
        let mut styles_str = styles.to_string();
        if let Some(idx) = styles_str.find("interpolate ") {
            let after = &styles_str[idx + "interpolate ".len()..];
            let (name, remainder) = split_once_ws(after.trim());
            interpolate = Some(name.to_string());
            styles_str = remainder.trim().to_string();
        }
        // Upstream's `flow.jison` builds each style entry character-by-character
        // from `styleComponent` rules that include SPACE, so leading whitespace
        // is preserved (e.g. `color:orange, stroke: orange` keeps the leading
        // space on ` stroke: orange`). The terminating `;` and any trailing
        // whitespace are stripped. We mirror that here so the edge `style="…"`
        // attribute reproduces upstream byte-for-byte.
        let mut styles: Vec<String> = styles_str
            .split(',')
            .map(|s| {
                // Strip trailing `;` and trailing whitespace, but keep leading
                // whitespace intact.
                let mut t = s.trim_end();
                while let Some(stripped) = t.strip_suffix(';') {
                    t = stripped.trim_end();
                }
                t.to_string()
            })
            .filter(|s| !s.is_empty() && !s.chars().all(|c| c.is_whitespace()))
            .collect();
        // Upstream `flowDb.updateLink` appends `fill:none` to the styles
        // ONLY for index-targeted updates, never for `linkStyle default`.
        // For default style: `this.edges.defaultStyle = style;` (no push).
        // For specific index: pushes `fill:none` when style is non-empty
        // and contains no `fill` property. Mirroring that keeps the
        // rendered `style="..."` attribute byte-exact.
        if !is_default && !styles.is_empty() && !styles.iter().any(|s| s.starts_with("fill")) {
            styles.push("fill:none".to_string());
        }
        self.diag.link_styles.push(LinkStyle {
            indices,
            is_default,
            styles,
            interpolate,
        });
    }

    fn parse_click(&mut self, rest: &str) {
        // `click id callback(args) "tooltip"` OR `click id href "url" "tooltip"`
        // OR `click id "url" "tooltip"`
        // We'll do a simplified parse.
        // Upstream `flowDb.setLink` / `setClickEvent` only mutate already-known
        // vertices (`this.vertices.get(id)` early-returns when undefined), so
        // a `click ID …` line referring to an unknown id is a no-op; do NOT
        // synthesize a phantom vertex here. Cypress fixture 218 references
        // `click B testClick` where `B` is never declared — upstream renders
        // 8 vertices, our previous behaviour rendered 9 (extra orphan `B`).
        let (id, tail) = split_once_ws(rest);
        let tail = tail.trim();
        let v = match self.diag.find_vertex_mut(id) {
            Some(v) => v,
            None => return,
        };
        if let Some(rest) = tail.strip_prefix("href") {
            let rest = rest.trim_start();
            let (url, after_url) = pop_quoted(rest);
            v.link = Some(url);
            let (tooltip_or_target, after_t) = pop_quoted(after_url.trim_start());
            if !tooltip_or_target.is_empty() {
                // Could be tooltip. Then target might follow.
                v.tooltip = Some(tooltip_or_target);
                let target = after_t.trim_start();
                if is_link_target(target) {
                    v.link_target = Some(target.to_string());
                }
            } else {
                // Maybe a link target was directly after url.
                let target = after_url.trim_start();
                if is_link_target(target) {
                    v.link_target = Some(target.to_string());
                }
            }
        } else if tail.starts_with('"') {
            let (url, after) = pop_quoted(tail);
            v.link = Some(url);
            let (tooltip, after_t) = pop_quoted(after.trim_start());
            if !tooltip.is_empty() {
                v.tooltip = Some(tooltip);
            }
            let target = after_t.trim_start();
            if is_link_target(target) {
                v.link_target = Some(target.to_string());
            }
        } else {
            // Callback form: click id functionName [(args)] ["tooltip"]
            let mut name = String::new();
            let mut args: Option<String> = None;
            let mut rest = tail;
            // Extract name up to '(' or whitespace.
            let mut chars = rest.char_indices();
            let mut end = rest.len();
            while let Some((idx, c)) = chars.next() {
                if c == '(' || c.is_whitespace() {
                    end = idx;
                    break;
                }
            }
            name.push_str(&rest[..end]);
            rest = rest[end..].trim_start();
            // Optional (args)
            if let Some(after_paren) = rest.strip_prefix('(') {
                if let Some(close) = after_paren.find(')') {
                    args = Some(after_paren[..close].to_string());
                    rest = after_paren[close + 1..].trim_start();
                }
            }
            let (tooltip, _) = pop_quoted(rest);
            v.callback = Some(name);
            v.callback_args = args;
            if !tooltip.is_empty() {
                v.tooltip = Some(tooltip);
            }
        }
        // Upstream `setLink` / `setClickEvent` both call
        // `this.setClass(ids, 'clickable')` after recording the URL or
        // callback, which pushes `'clickable'` into `vertex.classes`.
        // Mirror that here so the class string is built in the order
        // upstream expects (e.g. `default clickable someclass` when a
        // later `class A someclass` directive appends `someclass`).
        v.classes.push("clickable".to_string());
    }

    fn ensure_vertex(&mut self, id: &str) {
        // Upstream increments vertexCounter on EVERY call to addVertex,
        // even when the vertex already exists. The domId is only set
        // once (on first encounter), but the counter always advances.
        let is_new = self.diag.find_vertex(id).is_none();
        if is_new {
            let order = self.vertex_counter;
            self.diag.vertices.push(Vertex {
                id: id.to_string(),
                order,
                ..Vertex::default()
            });
            // Register into current subgraph if any.
            if let Some(sid) = self.current_subgraph.last().cloned() {
                if let Some(s) = self.diag.subgraphs.iter_mut().find(|s| s.id == sid) {
                    if !s.members.contains(&id.to_string()) {
                        s.members.push(id.to_string());
                    }
                }
            }
        } else {
            // Still register membership if not yet present.
            if let Some(sid) = self.current_subgraph.last().cloned() {
                if let Some(s) = self.diag.subgraphs.iter_mut().find(|s| s.id == sid) {
                    if !s.members.contains(&id.to_string()) {
                        s.members.push(id.to_string());
                    }
                }
            }
        }
        // Always increment — even for existing vertices.
        self.vertex_counter += 1;
    }

    fn parse_vertex_statement(&mut self, stmt: &str) -> Result<()> {
        // Walk along the string, producing a chain:
        //   nodeGroup LINK nodeGroup LINK nodeGroup ...
        // `nodeGroup` is one or more vertices joined by `&`.
        let mut cursor: String = stmt.to_string();
        let mut prev_group: Option<Vec<String>> = None;
        let mut prev_link: Option<ParsedLink> = None;
        loop {
            let trimmed = cursor.trim_start().to_string();
            if trimmed.is_empty() {
                break;
            }
            // Parse a node group
            let (group, consumed) = match self.parse_node_group(&trimmed) {
                Some(x) => x,
                None => break,
            };
            if let Some(link) = prev_link.take() {
                if let Some(prev) = prev_group.take() {
                    for s in &prev {
                        for e in &group {
                            let idx = self.diag.edges.len();
                            let edge = Edge {
                                id: link.id.clone(),
                                start: s.clone(),
                                end: e.clone(),
                                stroke: link.stroke,
                                length: link.length,
                                arrow_end: link.arrow_end,
                                arrow_start: link.arrow_start,
                                label: link.label.clone(),
                                index: idx,
                                classes: Vec::new(),
                                curve: None,
                                animation: None,
                                scope: self.current_subgraph.last().cloned(),
                            };
                            self.diag.edges.push(edge);
                        }
                    }
                }
            }
            prev_group = Some(group);
            let rest = trimmed[consumed..].trim_start().to_string();
            if rest.is_empty() {
                break;
            }
            // Try to parse a link.
            match parse_link(&rest) {
                Some((link, rem_len)) => {
                    prev_link = Some(link);
                    cursor = rest[rem_len..].to_string();
                }
                None => {
                    // No link — we're done (or it's just spaces).
                    break;
                }
            }
        }
        Ok(())
    }

    /// Parse a `node [& node]*` group. Returns (ids, consumed-byte-count)
    /// or `None` if nothing parseable at the head.
    fn parse_node_group(&mut self, s: &str) -> Option<(Vec<String>, usize)> {
        let mut ids: Vec<String> = Vec::new();
        // Defer the extra `vertexCounter++` that upstream's jison parser
        // performs for each `id@{...}` shapeData binding (cases 49/52 in
        // `flow.jison`). Upstream order matters: when shapeData appears
        // mid-chain (e.g. `B & C@{x} & E@{y}`), the counter bumps only
        // happen AFTER the next styledVertex on the right has been pushed
        // through the bare `vertex: idString` rule (case 72). We model that
        // by collecting the count here and applying the bumps once the
        // group has finished — that keeps domIds aligned (`E-3`, `D-6`).
        let mut shape_data_bumps: usize = 0;
        let mut pos: usize = 0;
        loop {
            // Skip leading spaces
            let tail = &s[pos..];
            let lead = tail
                .chars()
                .take_while(|c| c.is_whitespace())
                .map(|c| c.len_utf8())
                .sum::<usize>();
            pos += lead;
            let cur_tail = &s[pos..];
            if cur_tail.is_empty() {
                break;
            }
            let (id, shape_opt, label_opt, class_suffix, consumed, had_shape_data, shape_data_val) =
                parse_one_vertex(cur_tail)?;
            pos += consumed;
            self.ensure_vertex(&id);
            if shape_opt.is_some() || label_opt.is_some() || shape_data_val.is_some() {
                if let Some(v) = self.diag.find_vertex_mut(&id) {
                    if let Some(sh) = shape_opt {
                        v.shape = Some(sh);
                    }
                    if let Some(lbl) = label_opt {
                        v.label = Some(lbl);
                    }
                    if let Some(sd) = shape_data_val {
                        v.shape_data = Some(sd);
                    }
                }
            }
            if let Some(cls) = class_suffix {
                if let Some(v) = self.diag.find_vertex_mut(&id) {
                    v.classes.push(cls);
                }
            }
            if had_shape_data {
                // Defer the extra counter bump until the surrounding `&`
                // group has finished — see comment at top of this function.
                shape_data_bumps += 1;
            }
            ids.push(id);
            // Look for `&` joiner
            let lead2 = s[pos..]
                .chars()
                .take_while(|c| c.is_whitespace())
                .map(|c| c.len_utf8())
                .sum::<usize>();
            pos += lead2;
            if s[pos..].starts_with('&') {
                pos += 1;
                continue;
            }
            break;
        }
        if ids.is_empty() {
            None
        } else {
            // Apply deferred shapeData counter bumps now that the chain has
            // closed. See comment at top of `parse_node_group`.
            self.vertex_counter += shape_data_bumps;
            Some((ids, pos))
        }
    }
}

// ─── helpers ────────────────────────────────────────────────────────

/// Reconstruct the synthetic edge id format the layout will assign
/// (`L_<start>_<end>_<index>`) and check whether `id` matches one of the
/// already-recorded edges. The per-pair index follows the order in which
/// duplicate `(start, end)` edges appear in the source, mirroring
/// upstream `flowDb.addSingleLink` (existing=0 → 0, otherwise → existing+1).
fn edge_id_matches_synthetic(id: &str, edges: &[crate::model::flowchart::Edge]) -> bool {
    edge_id_matches_synthetic_index(id, edges).is_some()
}

fn edge_id_matches_synthetic_index(
    id: &str,
    edges: &[crate::model::flowchart::Edge],
) -> Option<usize> {
    use std::collections::HashMap;
    let mut pair_count: HashMap<(String, String), usize> = HashMap::new();
    for (i, e) in edges.iter().enumerate() {
        let n = *pair_count
            .entry((e.start.clone(), e.end.clone()))
            .and_modify(|c| *c += 1)
            .or_insert(0);
        let counter = if n == 0 { 0 } else { n + 1 };
        let synth = format!("L_{}_{}_{}", e.start, e.end, counter);
        if synth == id {
            return Some(i);
        }
    }
    None
}

fn parse_edge_meta_curve(body: &str) -> Option<&str> {
    for part in body.split(',') {
        let part = part.trim();
        if let Some(val) = part.strip_prefix("curve:") {
            let val = val.trim().trim_matches('"').trim_matches('\'');
            if !val.is_empty() {
                return Some(val);
            }
        }
    }
    None
}

/// Extract the `animation:` value from an `@{ ... }` body. Recognises the
/// upstream-supported tokens `slow` / `fast` (any other value is dropped to
/// match upstream `edgeMetaSchema`).
fn parse_edge_meta_animation(body: &str) -> Option<&str> {
    for part in body.split(',') {
        let part = part.trim();
        if let Some(val) = part.strip_prefix("animation:") {
            let val = val.trim().trim_matches('"').trim_matches('\'');
            if val == "slow" || val == "fast" {
                return Some(val);
            }
        }
    }
    None
}

fn split_once_ws(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    if let Some(idx) = s.find(char::is_whitespace) {
        (&s[..idx], s[idx..].trim_start())
    } else {
        (s, "")
    }
}

/// Split `s` on `;` characters that occur outside double-quoted strings.
/// Returns the resulting segments (empty segments from trailing `;` are kept
/// as empty strings so the caller can skip them).
fn split_semicolons_outside_quotes(s: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_quote = !in_quote;
                current.push(c);
            }
            ';' if !in_quote => {
                result.push(current.clone());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    result.push(current);
    result
}

fn pop_quoted(s: &str) -> (String, &str) {
    let s = s.trim_start();
    if let Some(rest) = s.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            return (rest[..end].to_string(), &rest[end + 1..]);
        }
    }
    (String::new(), s)
}

fn is_link_target(s: &str) -> bool {
    matches!(s.trim(), "_self" | "_blank" | "_parent" | "_top")
}

fn parse_label_text(raw: &str) -> Label {
    let trimmed = raw.trim();
    // Markdown label: starts with "`" inside quotes — check BEFORE plain string.
    if trimmed.starts_with("\"`") && trimmed.ends_with("`\"") {
        let inner = &trimmed[2..trimmed.len() - 2];
        // Marked.js paragraph normalisation: leading whitespace on each
        // continuation line of a multi-line paragraph is stripped before
        // the lexer emits text tokens. Mermaid's `markdownToHTML` runs
        // marked directly on the raw source, so the same stripping applies.
        // We pre-bake it here so both measurement and rendering observe
        // the post-marked text (`<br>` lines are unaffected — marked only
        // touches `\n`-separated lines, not inline HTML breaks).
        let cleaned = strip_md_continuation_indents(inner);
        return Label::markdown(cleaned);
    }
    if let Some(inner) = trimmed.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        // Upstream mermaid trims leading/trailing whitespace (incl. newlines)
        // from the quoted string body before HTML-conversion. Otherwise a
        // node like ["\n\nfoo\nbar\n"] would emit `<br/><br/>foo<br/>bar<br/>`
        // instead of `foo<br/>bar`. See ext_fixtures/demos/flowchart/48.
        return Label::string(decode_pseudo_entities(inner.trim()));
    }
    Label::text(decode_pseudo_entities(trimmed))
}

/// Decode upstream's `#NAME;` pseudo-entities into the equivalent character.
///
/// Mermaid's parser emits `#NAME;` (with a literal `#` instead of `&`) as a
/// safe pre-DOM-purify marker for HTML named entities embedded in user
/// strings — e.g. `Lorem #quot;ipsum#quot;` becomes `Lorem "ipsum"` once the
/// label is rendered. We emit the SVG without going through a real HTML
/// parser, so we have to perform the decode here to match the reference
/// output byte-for-byte (cypress fixture 160 relies on it).
///
/// The named-entity table is intentionally narrow — only the entities that
/// appear in the upstream fixtures are decoded. Unknown names are passed
/// through verbatim so mermaid features that intentionally use `#NAME;` as
/// literal text remain untouched.
fn decode_pseudo_entities(s: &str) -> String {
    if !s.contains('#') {
        return s.to_owned();
    }
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' {
            // Look for `#<name>;` where <name> is alphanumeric.
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_alphanumeric() {
                j += 1;
            }
            if j > i + 1 && j < bytes.len() && bytes[j] == b';' {
                let name = &s[i + 1..j];
                if let Some(replacement) = named_entity_to_str(name) {
                    out.push_str(replacement);
                    i = j + 1;
                    continue;
                }
            }
        }
        // Copy a UTF-8 char.
        let b = bytes[i];
        let ch_len = if b < 0x80 {
            1
        } else if b & 0xE0 == 0xC0 {
            2
        } else if b & 0xF0 == 0xE0 {
            3
        } else {
            4
        };
        out.push_str(&s[i..i + ch_len]);
        i += ch_len;
    }
    out
}

fn named_entity_to_str(name: &str) -> Option<&'static str> {
    Some(match name {
        "quot" => "\"",
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "apos" => "'",
        "nbsp" => "\u{00A0}",
        _ => return None,
    })
}

/// Mirror marked.js paragraph normalisation for backtick-markdown labels:
/// every line after the first has its leading whitespace removed before the
/// text reaches `markdownToHTML`. Empty / whitespace-only lines are kept
/// (subsequent newline collapsing handles those for measurement).
///
/// Note: this only operates on `\n`-separated continuation lines. Inline
/// `<br>` tags are left intact — marked does not touch them, and stripping
/// the space after `<br>` would clobber labels like `**a**:<br> 5...`.
fn strip_md_continuation_indents(src: &str) -> String {
    if !src.contains('\n') {
        return src.to_string();
    }
    let mut out = String::with_capacity(src.len());
    let mut first = true;
    for line in src.split('\n') {
        if first {
            out.push_str(line);
            first = false;
        } else {
            out.push('\n');
            // Strip leading ASCII space/tab characters on this line.
            let trimmed = line.trim_start_matches(|c: char| c == ' ' || c == '\t');
            out.push_str(trimmed);
        }
    }
    out
}

fn split_semis(s: &str) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut depth_paren = 0i32;
    let mut depth_sq = 0i32;
    let mut depth_cu = 0i32;
    let mut in_str = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if in_str {
            current.push(c);
            if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                current.push(c);
            }
            '(' => {
                depth_paren += 1;
                current.push(c);
            }
            ')' => {
                depth_paren -= 1;
                current.push(c);
            }
            '[' => {
                depth_sq += 1;
                current.push(c);
            }
            ']' => {
                depth_sq -= 1;
                current.push(c);
            }
            '{' => {
                depth_cu += 1;
                current.push(c);
            }
            '}' => {
                depth_cu -= 1;
                current.push(c);
            }
            ';' if depth_paren == 0 && depth_sq == 0 && depth_cu == 0 => {
                pieces.push(std::mem::take(&mut current));
            }
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        pieces.push(current);
    }
    pieces
}

#[derive(Debug, Clone)]
struct ParsedLink {
    id: Option<String>,
    stroke: EdgeStroke,
    length: usize,
    arrow_end: ArrowType,
    arrow_start: ArrowType,
    label: Option<Label>,
}

/// Parse a link at the head of `s`, return (link, consumed-byte-count).
fn parse_link(s: &str) -> Option<(ParsedLink, usize)> {
    let head = s.trim_start();
    // Optional LINK_ID: `id@` prefix when followed by arrow chars.
    let (link_id, after_id) = if let Some(pos) = head.find('@') {
        // Only treat as link-id if preceded by non-arrow ident chars and
        // followed by non-`{` (brace = shape data).
        let before = &head[..pos];
        let after = &head[pos + 1..];
        let valid_ident = !before.is_empty()
            && before
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        // `after` must begin with an arrow char.
        let starts_arrow = after
            .chars()
            .next()
            .map(|c| "-=.<>xo".contains(c) || c.is_whitespace())
            .unwrap_or(false);
        if valid_ident && starts_arrow && !after.starts_with('{') {
            (Some(before.to_string()), after.trim_start())
        } else {
            (None, head)
        }
    } else {
        (None, head)
    };

    // Scan the arrow. Recognised patterns:
    //   -->           solid  arrow
    //   ---           solid  none (line only, length 1)
    //   ----          solid  none (line only, length 2)
    //   -->           arrow end
    //   ==>           thick  arrow
    //   -.->          dotted arrow
    //   <-->          both-ends arrow
    //   ~~~           invisible
    //   -- text -->
    //   == text ==>
    //   -. text .->
    let first_non_ws = after_id
        .find(|c: char| !c.is_whitespace())
        .unwrap_or(after_id.len());
    let arrow_start = &after_id[first_non_ws..];
    if arrow_start.is_empty() {
        return None;
    }

    // Identify arrow body chars.
    let first = arrow_start.chars().next()?;
    // Check for `|label|` attached arrow-text (after arrow). We first
    // find the arrow span, then see if `|...|` follows.
    let (arrow_body, rem_after_arrow) = scan_arrow(arrow_start)?;

    // Some forms embed the label inside the arrow itself:
    //   `-- text -->`, `== text ==>`, `-. text .->`
    // scan_arrow handles those by returning the whole span.

    let (stroke, length, arrow_end, arrow_start_ty, inner_label) = classify_arrow(arrow_body)?;

    // Consume optional `|label|` immediately after
    let mut label = inner_label;
    let mut rem = rem_after_arrow;
    let rem_tr = rem.trim_start();
    if let Some(bar_rest) = rem_tr.strip_prefix('|') {
        if let Some(end) = bar_rest.find('|') {
            let text = &bar_rest[..end];
            label = Some(parse_label_text(text));
            rem = &bar_rest[end + 1..];
        }
    }

    let _ = first;
    // Compute how many bytes of `s` we consumed by measuring the
    // remaining tail's byte length.
    let consumed = s.len() - rem.len();
    Some((
        ParsedLink {
            id: link_id,
            stroke,
            length,
            arrow_end,
            arrow_start: arrow_start_ty,
            label,
        },
        consumed,
    ))
}

/// Scan the arrow span starting at `s`. Returns `(arrow_body, remainder)`.
/// The arrow body is the full span of arrow characters + any embedded
/// label text (between two arrow-body segments separated by spaces).
///
/// Handles:
///   `-->`, `---`, `----`, `<-->`, `x--x`, `o--o`
///   `==>`, `===`, `<==>`, `x==x`
///   `-.->`, `-..->`, `<-.->`, `-.- ` (no arrow)
///   `-- text -->` / `== text ==>` / `-. text .->`
///   `~~~` (invisible)
fn scan_arrow(s: &str) -> Option<(&str, &str)> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let b0 = bytes[0];
    if !is_arrow_boundary_byte(b0) {
        return None;
    }
    // `~~~[~]*` invisible.
    if b0 == b'~' {
        let mut i = 0;
        while i < bytes.len() && bytes[i] == b'~' {
            i += 1;
        }
        if i >= 3 {
            return Some((&s[..i], &s[i..]));
        } else {
            return None;
        }
    }
    let mut i = 0;
    // Optional start-arrow char.
    if matches!(b0, b'<' | b'x' | b'o') {
        i += 1;
    }
    // Determine body type.
    if i >= bytes.len() || !matches!(bytes[i], b'-' | b'=' | b'.') {
        return None;
    }
    // Dotted is `-.` or `<-.` etc. — after an optional `-`, comes `.`.
    let mut is_dotted = false;
    if bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'.' {
        is_dotted = true;
    } else if bytes[i] == b'.' {
        is_dotted = true;
    }
    let body_ch = if is_dotted { b'.' } else { bytes[i] };

    // Consume body. For dotted, the body is a mix of `.` and `-`.
    let body_pred = |b: u8| -> bool {
        if is_dotted {
            b == b'-' || b == b'.'
        } else {
            b == body_ch
        }
    };
    while i < bytes.len() && body_pred(bytes[i]) {
        i += 1;
    }
    let after_first_body = i;

    // Possibly absorb embedded label `space TEXT space BODY-RUN`.
    if i < bytes.len() && bytes[i] == b' ' {
        let rest = &s[i..];
        if let Some(pos) = find_next_arrow_segment(rest, if is_dotted { b'.' } else { body_ch }) {
            let next_seg_start = i + pos;
            i = next_seg_start;
            while i < bytes.len() && body_pred(bytes[i]) {
                i += 1;
            }
        } else {
            i = after_first_body;
        }
    } else if i < bytes.len() && !matches!(bytes[i], b'>' | b'x' | b'o' | b'|' | b'\n' | b'\r') {
        // No-space embedded label (`--lb1-->`, `==lb2==>`, `-.lb.->`,
        // `--lb -->` etc.). Mirror upstream mermaid's jison START_LINK
        // rule which accepts a label that may either be flush against
        // the body run or include internal spaces, so long as it is
        // terminated by another body run.
        let body_ch_eff = if is_dotted { b'.' } else { body_ch };
        if let Some(end) = find_inline_arrow_terminator(&bytes[i..], body_ch_eff) {
            i += end;
        }
    }
    // Optional arrow-end: `>`, `x`, `o`.
    if i < bytes.len() && matches!(bytes[i], b'>' | b'x' | b'o') {
        i += 1;
    }
    Some((&s[..i], &s[i..]))
}

/// Locate the end of an inline label + trailing body-run inside a no-space
/// arrow such as `--lb-->`, `==lb2==>`, or `--lb -->`.
///
/// `bytes` starts AT the first label character (already past the leading
/// body run). `body_ch_eff` is the body character (`-`, `=`, or `.`).
///
/// The label may contain internal whitespace; the terminator is the FIRST
/// body-run that is followed by `>`/`x`/`o` (or whitespace at end-of-arrow
/// for the no-arrow `--label--` form).  Stops at `|` and newline characters.
///
/// Returns the byte offset of the trailing body run's END (i.e. where the
/// arrow-end char `>`/`x`/`o` would sit, or end-of-body for no-arrow forms).
/// Returns None when no terminator is found.
fn find_inline_arrow_terminator(bytes: &[u8], body_ch_eff: u8) -> Option<usize> {
    let is_body = |b: u8| b == body_ch_eff || (body_ch_eff == b'-' && b == b'.');
    // Walk forwards. For each non-body, non-`|`, non-newline position, see
    // if it sits inside a label run. Track the FIRST body-run that is
    // followed by an arrow-end terminator.
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'|' || b == b'\n' || b == b'\r' {
            return None;
        }
        if is_body(b) {
            // Start of a body run.
            let run_start = i;
            while i < bytes.len() && is_body(bytes[i]) {
                i += 1;
            }
            let run_len = i - run_start;
            // Arrow-end form: body run ≥ 2 chars + `>`/`x`/`o`.
            if run_len >= 2 && i < bytes.len() && matches!(bytes[i], b'>' | b'x' | b'o') {
                return Some(i);
            }
            // No-arrow form: body run ≥ 2 chars at end-of-input or before
            // whitespace that ends the link span.
            if run_len >= 2 && (i == bytes.len() || bytes[i] == b' ' || bytes[i] == b'\t') {
                return Some(i);
            }
            // Otherwise this body run was part of the label — continue.
        } else {
            i += 1;
        }
    }
    None
}

fn find_next_arrow_segment(s: &str, body_ch: u8) -> Option<usize> {
    // Look for whitespace + body_ch after at least 1 non-body char.
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b' ' {
            // Lookahead: if next non-space is the body-char run, we've found it.
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] == b' ' {
                j += 1;
            }
            if j < bytes.len() && (bytes[j] == body_ch || (body_ch == b'-' && bytes[j] == b'.')) {
                return Some(j);
            }
        }
        // Must not hit `|` — that breaks the arrow.
        if bytes[i] == b'|' {
            return None;
        }
        i += 1;
    }
    None
}

fn is_arrow_boundary_byte(b: u8) -> bool {
    matches!(b, b'-' | b'=' | b'.' | b'<' | b'x' | b'o' | b'~')
}

/// True when `line` (already left-trimmed) begins with what could be the
/// LINK token of a flowchart edge (e.g. `--`, `==`, `-.`, `<--`, `~~~`).
/// Used by `collect_statement` to detect statement continuations like
///   ```text
///   a{{...}}
///   --> b{{...}}
///   ```
/// where the arrow is the first non-whitespace token on a fresh line.
///
/// Excludes lines that begin with `x`/`o` (those have to be followed by a
/// `-`/`=`/`.` arrow body but `x` and `o` are also valid identifiers, so
/// admitting them at column 0 of a line would mis-classify nodes named
/// `x` / `o`).
fn line_starts_with_arrow(line: &str) -> bool {
    let bytes = line.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    match bytes[0] {
        b'-' | b'=' | b'~' => {
            // Need at least two body chars (or `~~~`) to be a real arrow.
            // Single `-` could be from a list item etc., so require the
            // second char also be arrow-body.
            bytes.len() >= 2 && matches!(bytes[1], b'-' | b'=' | b'.' | b'>' | b'~')
        }
        b'<' => {
            // `<-->`, `<==>`, `<-.->` — second char must be `-`, `=`, or `.`.
            bytes.len() >= 2 && matches!(bytes[1], b'-' | b'=' | b'.')
        }
        b'.' => {
            // `.- ` would not be a valid arrow start; only `-.` is.
            false
        }
        _ => false,
    }
}

/// Classify an arrow span: returns (stroke, length, arrow_end, arrow_start, embedded_label).
fn classify_arrow(arrow: &str) -> Option<(EdgeStroke, usize, ArrowType, ArrowType, Option<Label>)> {
    if arrow.chars().all(|c| c == '~') {
        return Some((
            EdgeStroke::Invisible,
            1,
            ArrowType::None,
            ArrowType::None,
            None,
        ));
    }
    // Extract optional start-arrow.
    let mut span = arrow;
    let arrow_start = match span.chars().next()? {
        '<' => {
            span = &span[1..];
            ArrowType::Arrow
        }
        'x' => {
            span = &span[1..];
            ArrowType::Cross
        }
        'o' => {
            span = &span[1..];
            ArrowType::Circle
        }
        _ => ArrowType::None,
    };

    // Determine stroke from body pattern.
    let span_bytes = span.as_bytes();
    if span_bytes.is_empty() {
        return None;
    }
    let stroke = if span_bytes[0] == b'=' {
        EdgeStroke::Thick
    } else if span_bytes[0] == b'-' && span_bytes.len() > 1 && span_bytes[1] == b'.' {
        EdgeStroke::Dotted
    } else if span_bytes[0] == b'.' {
        EdgeStroke::Dotted
    } else if span_bytes[0] == b'-' {
        let mut is_dotted = false;
        for &b in span_bytes {
            if b == b'.' {
                is_dotted = true;
                break;
            }
            if b != b'-' && b != b'.' {
                break;
            }
        }
        if is_dotted {
            EdgeStroke::Dotted
        } else {
            EdgeStroke::Normal
        }
    } else {
        return None;
    };

    // Strip trailing arrow-end char.
    let mut span_rest = span;
    let tail_char = span_rest.chars().last()?;
    let arrow_end = match tail_char {
        '>' => {
            let cut = span_rest.len() - 1;
            span_rest = &span_rest[..cut];
            ArrowType::Arrow
        }
        'x' => {
            let cut = span_rest.len() - 1;
            span_rest = &span_rest[..cut];
            ArrowType::Cross
        }
        'o' => {
            let cut = span_rest.len() - 1;
            span_rest = &span_rest[..cut];
            ArrowType::Circle
        }
        _ => ArrowType::None,
    };

    // Detect embedded label: a space somewhere inside span_rest, OR a
    // non-body / non-space character that breaks the leading body run
    // (the `--lb1-->` no-space form).
    let body_predicate = |c: char| match stroke {
        EdgeStroke::Thick => c == '=',
        EdgeStroke::Dotted => c == '-' || c == '.',
        EdgeStroke::Normal => c == '-',
        _ => false,
    };
    let break_idx = span_rest
        .char_indices()
        .find(|(_, c)| !body_predicate(*c))
        .map(|(i, _)| i);
    let (body_chars, dot_chars, embedded_label) = if let Some(brk) = break_idx {
        // Everything between the first and last body-run.
        let after = &span_rest[brk..];
        let mut run_start = after.len();
        // Walk backwards to find start of trailing body run.
        let chars: Vec<(usize, char)> = after.char_indices().collect();
        for (idx, c) in chars.iter().rev() {
            if body_predicate(*c) {
                run_start = *idx;
            } else if run_start != after.len() {
                break;
            }
        }
        let label_text = after[..run_start].trim();
        let trail = &after[run_start..];
        // Upstream mermaid uses only the TRAILING body (LINK token) for length computation.
        // The leading body (START_LINK token) determines stroke/start-arrow type only.
        // See destructLink: length comes from destructEndLink(_str) where _str = trailing arrow.
        let body_len = trail.chars().filter(|c| body_predicate(*c)).count();
        let dots = trail.chars().filter(|c| *c == '.').count();
        let label = if label_text.is_empty() {
            None
        } else {
            Some(parse_label_text(label_text))
        };
        (body_len, dots, label)
    } else {
        let bc = span_rest.chars().filter(|c| body_predicate(*c)).count();
        let dc = span_rest.chars().filter(|c| *c == '.').count();
        (bc, dc, None)
    };

    // Mirror upstream mermaid's destructEndLink length formula:
    //   line = str.slice(0, -1)  -- always removes last char
    //   length = dots_in_line (dotted) OR line.length - 1 (normal/thick)
    // Since we've already removed the arrow-end char when arrow_end != None,
    // we need to apply one more subtraction when there's no arrow end
    // (the trailing '-' or '=' plays the role of the "end char").
    let extra = if arrow_end == ArrowType::None {
        1usize
    } else {
        0usize
    };
    let length = if stroke == EdgeStroke::Dotted {
        // For dotted: length = dot_count - extra (where extra accounts for no arrow)
        dot_chars.saturating_sub(extra)
    } else {
        // For normal/thick: length = body_chars - extra - 1
        body_chars.saturating_sub(extra).saturating_sub(1)
    };
    Some((stroke, length, arrow_end, arrow_start, embedded_label))
}

/// Parse one vertex expression at the head of `s`:
///   idString                              — bare
///   idString[label]                       — rect
///   idString(label)                       — round
///   idString((label))                     — circle
///   idString(((label)))                   — doublecircle
///   idString([label])                     — stadium
///   idString[[label]]                     — subroutine
///   idString[(label)]                     — cylinder
///   idString{label}                       — diamond
///   idString{{label}}                     — hexagon
///   idString>label]                       — odd (rect_left_inv_arrow)
///   idString[/label/]                     — parallelogram (lean_right)
///   idString[\label\]                     — lean_left
///   idString[/label\]                     — trapezoid
///   idString[\label/]                     — inv_trapezoid
///   idString(-label-)                     — ellipse
///   idString:::className                  — class suffix
///
/// Returns (id, shape, label, class_suffix, bytes_consumed).
fn parse_one_vertex(
    s: &str,
) -> Option<(
    String,
    Option<String>,
    Option<Label>,
    Option<String>,
    usize,
    bool,
    Option<String>,
)> {
    // Read id up to a shape-starter / whitespace / `&` / link-starter.
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'[' || c == b'(' || c == b'{' || c == b'>' {
            break;
        }
        // `@{...}` is node shape-data syntax (mermaid v11+): end id parsing
        // here so the post-shape branch can recognise the `@{...}` block.
        if c == b'@' && bytes.get(i + 1) == Some(&b'{') {
            break;
        }
        // End of id if whitespace or `&` or link-start `-`/`=`/`.`/`<`/`x`/`o`/`~` or `:`.
        if c == b' ' || c == b'\t' || c == b'\n' || c == b'&' {
            break;
        }
        if c == b'-' || c == b'=' || c == b'.' || c == b'~' {
            // Could be part of id (e.g. `foo-bar`). Only treat as
            // link start if followed by arrow-body char.
            // For simplicity: allow `-` / `.` / `=` to continue id if
            // next char is alphanumeric.
            let next = bytes.get(i + 1).copied();
            if matches!(
                next,
                Some(b'-' | b'=' | b'.' | b'>' | b'<' | b'o' | b'x' | b'~')
            ) {
                break;
            }
            if c == b'=' && matches!(next, Some(b'=')) {
                break;
            }
            if c == b'-' && matches!(next, Some(b'-') | Some(b'>')) {
                break;
            }
            if c == b'.' && matches!(next, Some(b'-')) {
                break;
            }
        }
        if c == b':' && bytes.get(i + 1) == Some(&b':') && bytes.get(i + 2) == Some(&b':') {
            break;
        }
        i += 1;
    }
    if i == 0 {
        return None;
    }
    let id = s[..i].trim().to_string();
    if id.is_empty() {
        return None;
    }
    let rest = &s[i..];
    let (shape, label, remainder) = if let Some(after) = rest.strip_prefix("[[") {
        // subroutine
        let (text, r) = take_until_quote_aware(after, "]]")?;
        (
            Some("subroutine".to_string()),
            Some(parse_label_text(text)),
            r,
        )
    } else if let Some(after) = rest.strip_prefix("[(") {
        let (text, r) = take_until_quote_aware(after, ")]")?;
        (
            Some("cylinder".to_string()),
            Some(parse_label_text(text)),
            r,
        )
    } else if let Some(after) = rest.strip_prefix("([") {
        let (text, r) = take_until_quote_aware(after, "])")?;
        (Some("stadium".to_string()), Some(parse_label_text(text)), r)
    } else if let Some(after) = rest.strip_prefix("(((") {
        let (text, r) = take_until_quote_aware(after, ")))")?;
        (
            Some("doublecircle".to_string()),
            Some(parse_label_text(text)),
            r,
        )
    } else if let Some(after) = rest.strip_prefix("((") {
        let (text, r) = take_until_quote_aware(after, "))")?;
        (Some("circle".to_string()), Some(parse_label_text(text)), r)
    } else if let Some(after) = rest.strip_prefix("(-") {
        let (text, r) = take_until_quote_aware(after, "-)")?;
        (Some("ellipse".to_string()), Some(parse_label_text(text)), r)
    } else if let Some(after) = rest.strip_prefix("(") {
        let (text, r) = take_until_quote_aware(after, ")")?;
        (Some("round".to_string()), Some(parse_label_text(text)), r)
    } else if let Some(after) = rest.strip_prefix("{{") {
        let (text, r) = take_until_quote_aware(after, "}}")?;
        (Some("hexagon".to_string()), Some(parse_label_text(text)), r)
    } else if let Some(after) = rest.strip_prefix("{") {
        let (text, r) = take_until_quote_aware(after, "}")?;
        (Some("diamond".to_string()), Some(parse_label_text(text)), r)
    } else if let Some(after) = rest.strip_prefix(">") {
        let (text, r) = take_until_quote_aware(after, "]")?;
        (Some("odd".to_string()), Some(parse_label_text(text)), r)
    } else if let Some(after) = rest.strip_prefix("[/") {
        // Try trapezoid `[/...\]`, lean_right `[/.../]`, inv_trapezoid `[/...\]` (dup), or lean_right.
        // Prefer to look for `\]` first (trapezoid), else `/]` (lean_right).
        let has_trap = after.contains("\\]");
        let has_lean = after.contains("/]");
        let (text, r, shape) = if has_trap && (!has_lean || after.find("\\]") < after.find("/]")) {
            let (t, r) = take_until_quote_aware(after, "\\]")?;
            (t, r, "trapezoid")
        } else {
            let (t, r) = take_until_quote_aware(after, "/]")?;
            (t, r, "lean_right")
        };
        (Some(shape.to_string()), Some(parse_label_text(text)), r)
    } else if let Some(after) = rest.strip_prefix("[\\") {
        let has_inv = after.contains("/]");
        let has_left = after.contains("\\]");
        let (text, r, shape) = if has_inv && (!has_left || after.find("/]") < after.find("\\]")) {
            let (t, r) = take_until_quote_aware(after, "/]")?;
            (t, r, "inv_trapezoid")
        } else {
            let (t, r) = take_until_quote_aware(after, "\\]")?;
            (t, r, "lean_left")
        };
        (Some(shape.to_string()), Some(parse_label_text(text)), r)
    } else if let Some(after) = rest.strip_prefix("[") {
        let (text, r) = take_until_quote_aware(after, "]")?;
        // Handle [|key:value|label] syntax — strip the `|key:value|` prefix.
        // Upstream grammar rule 62: addVertex(id, label, "rect", ..., {key: value})
        let label_text = if let Some(pipe_rest) = text.strip_prefix('|') {
            if let Some(close_pipe) = pipe_rest.find('|') {
                pipe_rest[close_pipe + 1..].trim()
            } else {
                text
            }
        } else {
            text
        };
        (
            Some("rect".to_string()),
            Some(parse_label_text(label_text)),
            r,
        )
    } else {
        (None, None, rest)
    };

    // Optional `:::className` suffix
    let (class_suffix, remainder) = if let Some(after) = remainder.strip_prefix(":::") {
        let end = after
            .find(|c: char| {
                c.is_whitespace() || c == '&' || c == '-' || c == '=' || c == '.' || c == ';'
            })
            .unwrap_or(after.len());
        (Some(after[..end].to_string()), &after[end..])
    } else {
        (None, remainder)
    };

    // Also handle `@{...}` shape-data immediately after id/shape.
    // Format: `id@{ key: value, key: "value", ... }`. We currently only
    // extract `label` and `shape` fields; everything else is dropped.
    // Labels supplied via `@{ label: ... }` are treated as markdown by
    // upstream — the rendered span gains the `markdown-node-label` class.
    let mut shape = shape;
    let mut label = label;
    let mut had_shape_data = false;
    let mut shape_data_val: Option<String> = None;
    let remainder = if let Some(after) = remainder.strip_prefix("@{") {
        if let Some(end) = after.find('}') {
            had_shape_data = true;
            let data = &after[..end];
            let (lbl, shp, ico) = parse_node_shape_data(data);
            if let Some(l) = lbl {
                label = Some(Label::markdown(l));
            }
            if let Some(s_kind) = shp {
                shape = Some(s_kind);
            }
            if let Some(icon_val) = ico {
                shape_data_val = Some(icon_val);
                if shape.is_none() {
                    shape = Some("icon".to_string());
                }
            }
            if shape.is_none() {
                shape = Some("rect".to_string());
            }
            &after[end + 1..]
        } else {
            remainder
        }
    } else {
        remainder
    };

    let consumed = s.len() - remainder.len();
    Some((id, shape, label, class_suffix, consumed, had_shape_data, shape_data_val))
}

fn take_until<'a>(s: &'a str, tok: &str) -> Option<(&'a str, &'a str)> {
    let end = s.find(tok)?;
    Some((&s[..end], &s[end + tok.len()..]))
}

/// Like `take_until`, but skip over `"..."` and `` `...` `` quoted regions when
/// scanning for `tok`. Used for shape body parsers (round/stadium/cylinder/...)
/// so a `)` or `]` inside a quoted markdown label such as
/// `("`The dog (1)`")` doesn't terminate the body prematurely.
fn take_until_quote_aware<'a>(s: &'a str, tok: &str) -> Option<(&'a str, &'a str)> {
    let bytes = s.as_bytes();
    let tok_bytes = tok.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        // Skip over quoted region (double-quote). Backslash escapes the next byte.
        if b == b'"' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        // Skip over backtick-quoted region (markdown label delimiter).
        if b == b'`' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'`' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        // Match `tok` at the current position.
        if i + tok_bytes.len() <= bytes.len() && &bytes[i..i + tok_bytes.len()] == tok_bytes {
            // SAFETY: i is at a valid char boundary (we've only stepped over
            // ASCII delimiters or skipped within quoted regions byte-by-byte;
            // `s` is a valid UTF-8 str so any non-ASCII bytes inside quotes
            // remain part of the same str slice).
            return Some((&s[..i], &s[i + tok_bytes.len()..]));
        }
        i += 1;
    }
    None
}

/// Parse the body of `@{ key: value, ... }` shape-data blocks. Only the
/// `label` and `shape` keys are honoured; values may be quoted (`"..."`)
/// or bare. Returns `(label, shape)`.
fn parse_node_shape_data(body: &str) -> (Option<String>, Option<String>, Option<String>) {
    let mut label: Option<String> = None;
    let mut shape: Option<String> = None;
    let mut icon: Option<String> = None;
    let mut rest = body.trim();
    while !rest.is_empty() {
        let colon = match rest.find(':') {
            Some(i) => i,
            None => break,
        };
        let key = rest[..colon].trim().to_string();
        rest = rest[colon + 1..].trim_start();
        let (value, after) = if let Some(stripped) = rest.strip_prefix('"') {
            let bytes = stripped.as_bytes();
            let mut j = 0;
            while j < bytes.len() {
                if bytes[j] == b'\\' && j + 1 < bytes.len() {
                    j += 2;
                    continue;
                }
                if bytes[j] == b'"' {
                    break;
                }
                j += 1;
            }
            let v = stripped[..j].to_string();
            let r = if j < bytes.len() {
                &stripped[j + 1..]
            } else {
                ""
            };
            (v, r)
        } else {
            let end = rest.find(',').unwrap_or(rest.len());
            (rest[..end].trim().to_string(), &rest[end..])
        };
        match key.as_str() {
            "label" => label = Some(value),
            "shape" => shape = Some(value),
            "icon" => icon = Some(value),
            _ => {}
        }
        let after = after.trim_start();
        rest = after.strip_prefix(',').unwrap_or(after).trim_start();
    }
    (label, shape, icon)
}

/// Renumber auto-generated subgraph IDs (matching `subGraph\d+`) to match
/// upstream's bottom-up (post-order) processing order.
///
/// Upstream's LR jison parser reduces inner subgraph blocks before outer ones,
/// so the `subCount` counter (used for auto-ids) increments for inner subgraphs
/// first. Our top-down parser assigns lower numbers to outer subgraphs.
///
/// This function performs a post-order traversal of the subgraph tree and
/// reassigns auto-ids in that order, matching upstream numbering exactly.
fn renumber_auto_subgraph_ids(diag: &mut FlowchartDiagram) {
    use std::collections::HashMap;

    // Determine which subgraphs have auto-generated IDs.
    // Auto-ids match the pattern `subGraph\d+`.
    let auto_re = |s: &str| -> bool {
        s.starts_with("subGraph") && s[8..].chars().all(|c| c.is_ascii_digit())
    };

    // Build a map from id → index for quick lookup.
    let id_to_idx: HashMap<String, usize> = diag
        .subgraphs
        .iter()
        .enumerate()
        .map(|(i, sg)| (sg.id.clone(), i))
        .collect();

    // Find roots (subgraphs not listed as children of any other subgraph).
    let all_children: std::collections::HashSet<String> = diag
        .subgraphs
        .iter()
        .flat_map(|sg| sg.children.iter().cloned())
        .collect();
    let roots: Vec<usize> = diag
        .subgraphs
        .iter()
        .enumerate()
        .filter(|(_, sg)| !all_children.contains(&sg.id))
        .map(|(i, _)| i)
        .collect();

    // Post-order traversal: collect auto-id subgraph indices in post-order.
    let mut post_order_auto: Vec<usize> = Vec::new();
    let mut stack: Vec<(usize, bool)> = roots.iter().map(|&i| (i, false)).collect();
    // Reverse to maintain original order when popping.
    stack.reverse();
    while let Some((idx, visited)) = stack.pop() {
        if visited {
            if auto_re(&diag.subgraphs[idx].id) {
                post_order_auto.push(idx);
            }
        } else {
            stack.push((idx, true));
            // Push children in reverse order so first child is processed first.
            let children: Vec<usize> = diag.subgraphs[idx]
                .children
                .iter()
                .filter_map(|cid| id_to_idx.get(cid).copied())
                .collect();
            for &ci in children.iter().rev() {
                stack.push((ci, false));
            }
        }
    }

    if post_order_auto.is_empty() {
        return;
    }

    // Determine what new counter values to assign.
    // In upstream, subCount increments for EVERY subgraph (auto or not).
    // Post-order traversal of the entire tree gives the upstream call order.
    // We need to compute, for each auto-id subgraph, what its upstream subCount
    // would have been when addSubGraph was called for it.
    //
    // Simpler approach: just number the auto-id subgraphs in post-order,
    // while non-auto subgraphs also consume counter slots.
    let mut full_post_order: Vec<usize> = Vec::new();
    let mut stack2: Vec<(usize, bool)> = roots.iter().map(|&i| (i, false)).collect();
    stack2.reverse();
    while let Some((idx, visited)) = stack2.pop() {
        if visited {
            full_post_order.push(idx);
        } else {
            stack2.push((idx, true));
            let children: Vec<usize> = diag.subgraphs[idx]
                .children
                .iter()
                .filter_map(|cid| id_to_idx.get(cid).copied())
                .collect();
            for &ci in children.iter().rev() {
                stack2.push((ci, false));
            }
        }
    }

    // Assign counter values: counter increments for every subgraph in post-order;
    // auto-id subgraphs get their new name from the counter at that point.
    let mut counter = 0usize;
    let mut new_ids: HashMap<usize, String> = HashMap::new();
    for &idx in &full_post_order {
        if auto_re(&diag.subgraphs[idx].id) {
            new_ids.insert(idx, format!("subGraph{}", counter));
        }
        counter += 1;
    }

    if new_ids.is_empty() {
        return;
    }

    // Build old→new id mapping.
    let id_remap: HashMap<String, String> = new_ids
        .iter()
        .map(|(&idx, new_id)| (diag.subgraphs[idx].id.clone(), new_id.clone()))
        .collect();

    // Apply remap to subgraph ids, children references, parent references in vertices, etc.
    for sg in &mut diag.subgraphs {
        if let Some(new_id) = id_remap.get(&sg.id) {
            sg.id = new_id.clone();
        }
        for child in &mut sg.children {
            if let Some(new_id) = id_remap.get(child.as_str()) {
                *child = new_id.clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_td() {
        let src = "flowchart TD\nA --> B\n";
        let d = parse(src).unwrap();
        assert_eq!(d.direction, Direction::TB);
        assert_eq!(d.vertices.len(), 2);
        assert_eq!(d.edges.len(), 1);
        assert_eq!(d.edges[0].start, "A");
        assert_eq!(d.edges[0].end, "B");
    }

    #[test]
    fn parses_graph_lr() {
        let src = "graph LR\nid1(Start)-->id2(Stop)\n";
        let d = parse(src).unwrap();
        assert_eq!(d.direction, Direction::LR);
        assert!(!d.is_v2);
        assert_eq!(d.vertices.len(), 2);
        let v = d.find_vertex("id1").unwrap();
        assert_eq!(v.shape.as_deref(), Some("round"));
        assert_eq!(v.label.as_ref().unwrap().text, "Start");
    }

    #[test]
    fn parses_shapes_diamond_hexagon_stadium() {
        let src = "flowchart TD\nA[rect]-->B(round)-->C{diamond}-->D{{hex}}-->E([pill])\n";
        let d = parse(src).unwrap();
        let names: Vec<_> = d
            .vertices
            .iter()
            .map(|v| v.shape.clone().unwrap_or_default())
            .collect();
        assert!(names.contains(&"rect".to_string()));
        assert!(names.contains(&"round".to_string()));
        assert!(names.contains(&"diamond".to_string()));
        assert!(names.contains(&"hexagon".to_string()));
        assert!(names.contains(&"stadium".to_string()));
    }

    #[test]
    fn parses_subgraph_with_title() {
        let src = "flowchart TD\nsubgraph s1 [Title]\n  A-->B\nend\nA-->C\n";
        let d = parse(src).unwrap();
        assert_eq!(d.subgraphs.len(), 1);
        assert_eq!(d.subgraphs[0].id, "s1");
        assert_eq!(d.subgraphs[0].title.as_ref().unwrap().text, "Title");
        assert!(d.subgraphs[0].members.contains(&"A".to_string()));
        assert!(d.subgraphs[0].members.contains(&"B".to_string()));
    }

    #[test]
    fn parses_class_def_and_class_stmt() {
        let src = "flowchart TD\nclassDef red fill:#f00,stroke:#000\nA-->B\nclass A red\n";
        let d = parse(src).unwrap();
        assert_eq!(d.class_defs.len(), 1);
        assert_eq!(d.class_defs[0].name, "red");
        let a = d.find_vertex("A").unwrap();
        assert_eq!(a.classes, vec!["red".to_string()]);
    }

    #[test]
    fn parses_link_with_label() {
        let src = "flowchart TD\nA-->|go|B\nA --> |via| C\nA-- walk -->D\n";
        let d = parse(src).unwrap();
        assert_eq!(d.edges.len(), 3);
        assert_eq!(d.edges[0].label.as_ref().unwrap().text, "go");
        assert_eq!(d.edges[1].label.as_ref().unwrap().text, "via");
        assert_eq!(d.edges[2].label.as_ref().unwrap().text, "walk");
    }

    #[test]
    fn parses_amp_expansion() {
        let src = "flowchart TD\nA & B --> C\n";
        let d = parse(src).unwrap();
        assert_eq!(d.edges.len(), 2);
        assert_eq!(d.edges[0].start, "A");
        assert_eq!(d.edges[1].start, "B");
        assert_eq!(d.edges[0].end, "C");
    }

    #[test]
    fn parses_thick_and_dotted_edges() {
        let src = "flowchart LR\nA==>B\nA-.->C\n";
        let d = parse(src).unwrap();
        assert_eq!(d.edges[0].stroke, EdgeStroke::Thick);
        assert_eq!(d.edges[1].stroke, EdgeStroke::Dotted);
    }

    #[test]
    fn parses_class_suffix_triple_colon() {
        let src = "flowchart TD\nclassDef red fill:#f00\nA:::red --> B\n";
        let d = parse(src).unwrap();
        let a = d.find_vertex("A").unwrap();
        assert!(a.classes.contains(&"red".to_string()));
    }

    #[test]
    fn parses_linkstyle() {
        let src =
            "flowchart LR\nA-->B\nA-->C\nlinkStyle 0 stroke:red\nlinkStyle default stroke:blue\n";
        let d = parse(src).unwrap();
        assert_eq!(d.link_styles.len(), 2);
        assert_eq!(d.link_styles[0].indices, vec![0]);
        assert!(d.link_styles[1].is_default);
    }

    #[test]
    fn parses_style_stmt() {
        let src = "flowchart LR\nA-->B\nstyle A fill:#f9f\n";
        let d = parse(src).unwrap();
        let a = d.find_vertex("A").unwrap();
        assert_eq!(a.styles, vec!["fill:#f9f".to_string()]);
    }

    #[test]
    fn parses_click_callback_and_href() {
        let src = "flowchart LR\nA-->B\nclick A callback \"tip\"\nclick B href \"https://example.com\" \"link tip\"\n";
        let d = parse(src).unwrap();
        let a = d.find_vertex("A").unwrap();
        assert_eq!(a.callback.as_deref(), Some("callback"));
        assert_eq!(a.tooltip.as_deref(), Some("tip"));
        let b = d.find_vertex("B").unwrap();
        assert_eq!(b.link.as_deref(), Some("https://example.com"));
        assert_eq!(b.tooltip.as_deref(), Some("link tip"));
    }

    #[test]
    fn class_def_comma_separated_names() {
        let src = "flowchart TD\nA-->B\nclassDef red,blue fill:#f00,stroke:#000\nclass A red\nclass B blue\n";
        let d = parse(src).unwrap();
        let a = d.find_vertex("A").unwrap();
        assert!(a.classes.contains(&"red".to_string()));
        let b = d.find_vertex("B").unwrap();
        assert!(b.classes.contains(&"blue".to_string()));
        let red_cd = d.class_defs.iter().find(|cd| cd.name == "red").unwrap();
        assert_eq!(red_cd.styles, vec!["fill:#f00", "stroke:#000"]);
        let blue_cd = d.class_defs.iter().find(|cd| cd.name == "blue").unwrap();
        assert_eq!(blue_cd.styles, vec!["fill:#f00", "stroke:#000"]);
    }

    #[test]
    fn thick_arrow_with_embedded_label() {
        let src = "flowchart TD\nC == Choice 1.2 ==> E\n";
        let d = parse(src).unwrap();
        assert_eq!(d.edges.len(), 1);
        let e = &d.edges[0];
        assert_eq!(e.start, "C");
        assert_eq!(e.end, "E");
        let label = e.label.as_ref().map(|l| l.text.as_str()).unwrap_or("");
        assert_eq!(label, "Choice 1.2");
        assert_eq!(e.stroke, EdgeStroke::Thick);
    }

    #[test]
    fn rejects_non_flowchart() {
        let err = parse("pie\n").unwrap_err();
        assert!(matches!(err, MermaidError::Parse { .. }));
    }
}
