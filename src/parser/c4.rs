//! C4 macro parser.
//!
//! Recognises the macro family from upstream `c4Diagram.jison`:
//! `Person`, `Person_Ext`, `System*`, `Container*`, `Component*`,
//! `Enterprise_Boundary`, `System_Boundary`, `Container_Boundary`,
//! `Boundary`, `Deployment_Node` (and `Node`, `Node_L`, `Node_R`),
//! relations `Rel`, `BiRel`, `Rel_U/D/L/R/Back`, `RelIndex`, plus the
//! style updates `UpdateElementStyle`, `UpdateRelStyle`,
//! `UpdateLayoutConfig`.
//!
//! This parser walks the source line-by-line, tokenises each macro
//! invocation by hand, and populates a [`C4Diagram`]. It does NOT
//! attempt to mirror the jison grammar's lexer states verbatim — it
//! exploits the fact that every C4 macro fits the regular shape
//!     `<Name>([ <arg> { , <arg> } ] )`
//! with arguments being either bare identifiers, double-quoted
//! strings, or `$key="value"` key-value pairs.

use crate::error::MermaidError;
use crate::model::c4::{
    C4Boundary, C4Diagram, C4Rel, C4Shape, C4Subtype, C4Text,
};
use crate::preprocess;

/// Parse a C4 source document into a [`C4Diagram`].
pub fn parse(source: &str) -> Result<C4Diagram, MermaidError> {
    let pre = preprocess::preprocess(source).map_err(|e| MermaidError::Parse {
        line: 0,
        col: 0,
        message: format!("c4 preprocess: {e}"),
    })?;
    let body = pre.cleaned_source.as_str();

    let mut diag = C4Diagram {
        meta: DiagramMetaBuilder::from_pre(&pre),
        ..C4Diagram::default()
    };

    // Detect subtype on the first non-empty content line.
    let trimmed_first = body
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("%%"))
        .unwrap_or("");
    diag.subtype = match () {
        _ if trimmed_first.starts_with("C4Context") => C4Subtype::Context,
        _ if trimmed_first.starts_with("C4Container") => C4Subtype::Container,
        _ if trimmed_first.starts_with("C4Component") => C4Subtype::Component,
        _ if trimmed_first.starts_with("C4Dynamic") => C4Subtype::Dynamic,
        _ if trimmed_first.starts_with("C4Deployment") => C4Subtype::Deployment,
        _ => {
            return Err(MermaidError::Parse {
                line: 0,
                col: 0,
                message: "c4: unrecognised header".into(),
            });
        }
    };

    // Concatenate body, then walk character-by-character to handle
    // multi-line macro calls. The jison grammar allows newlines inside
    // `(...)` argument lists, so we cannot tokenise per line.
    let mut p = Parser::new(body);
    p.consume_header()?;

    let mut boundary_stack: Vec<String> = vec!["global".into()];
    let mut current_parent: String = "global".into();

    while !p.at_eof() {
        p.skip_ws_and_comments();
        if p.at_eof() {
            break;
        }
        if p.eat_char('}') {
            // Close the current boundary scope.
            boundary_stack.pop();
            current_parent = boundary_stack
                .last()
                .cloned()
                .unwrap_or_else(|| "global".into());
            continue;
        }

        // Title line — `title Some text`. Up to next newline.
        if p.peek_keyword("title") {
            p.advance(5);
            let line = p.read_to_eol().trim().to_string();
            if !line.is_empty() {
                diag.meta.title = Some(line);
            }
            continue;
        }
        // accTitle:, accDescr:, accDescription
        if p.peek_keyword("accTitle") {
            p.advance(8);
            p.skip_inline_ws();
            if p.eat_char(':') {
                p.skip_inline_ws();
                let v = p.read_to_eol().trim().to_string();
                if !v.is_empty() {
                    diag.meta.acc_title = Some(v);
                }
                continue;
            }
        }
        if p.peek_keyword("accDescription") {
            p.advance(14);
            let v = p.read_to_eol().trim().to_string();
            if !v.is_empty() {
                diag.meta.acc_descr = Some(v);
            }
            continue;
        }
        if p.peek_keyword("accDescr") {
            p.advance(8);
            p.skip_inline_ws();
            if p.eat_char(':') {
                p.skip_inline_ws();
                let v = p.read_to_eol().trim().to_string();
                if !v.is_empty() {
                    diag.meta.acc_descr = Some(v);
                }
                continue;
            }
            if p.eat_char('{') {
                let v = p.read_until_char('}');
                p.eat_char('}');
                if !v.trim().is_empty() {
                    diag.meta.acc_descr = Some(v.trim().to_string());
                }
                continue;
            }
        }

        // Macro name — letters/underscores.
        let name = p.read_ident();
        if name.is_empty() {
            // Unknown char; skip one to avoid infinite loop.
            p.advance(1);
            continue;
        }
        // Top-level kind keyword may appear again (rare); skip rest of line.
        if matches!(
            name.as_str(),
            "C4Context" | "C4Container" | "C4Component" | "C4Dynamic" | "C4Deployment"
        ) {
            let _ = p.read_to_eol();
            continue;
        }

        // Now expect `(`.
        p.skip_ws_and_comments();
        if !p.eat_char('(') {
            // Direction or unrecognised — drop line.
            let _ = p.read_to_eol();
            continue;
        }
        let args = parse_args(&mut p);
        // Attempt to consume optional `{` opening a boundary scope.
        p.skip_ws_and_comments();
        let opens_boundary = p.eat_char('{');

        dispatch_macro(
            &name,
            &args,
            opens_boundary,
            &mut diag,
            &mut boundary_stack,
            &mut current_parent,
        );
    }

    Ok(diag)
}

// ── Helpers ──────────────────────────────────────────────────────────

struct DiagramMetaBuilder;
impl DiagramMetaBuilder {
    fn from_pre(pre: &preprocess::PreprocessOutput) -> crate::model::DiagramMeta {
        let mut meta = crate::model::DiagramMeta::default();
        if let Some(t) = pre.config.title.as_ref() {
            meta.title = Some(t.clone());
        }
        meta
    }
}

#[derive(Debug, Clone)]
enum Arg {
    /// Plain identifier or unquoted string.
    Bare(String),
    /// Double-quoted string.
    Quoted(String),
    /// `$key="value"`.
    Kv(String, String),
}

fn parse_args(p: &mut Parser) -> Vec<Arg> {
    let mut out = Vec::new();
    loop {
        p.skip_ws_and_comments();
        if p.eat_char(')') {
            break;
        }
        if p.at_eof() {
            break;
        }
        if p.eat_char(',') {
            continue;
        }
        // $key="val"
        if p.eat_char('$') {
            let key = p.read_until_set("=");
            p.eat_char('=');
            p.skip_inline_ws();
            if p.eat_char('"') {
                let val = p.read_until_char('"');
                p.eat_char('"');
                out.push(Arg::Kv(key.trim().to_string(), val));
            } else {
                let val = p.read_until_set(",)");
                out.push(Arg::Kv(key.trim().to_string(), val.trim().to_string()));
            }
            continue;
        }
        // Quoted
        if p.eat_char('"') {
            let s = p.read_until_char('"');
            p.eat_char('"');
            out.push(Arg::Quoted(s));
            continue;
        }
        // Bare token up to comma, paren, or whitespace.
        let s = p.read_until_set(",)");
        let s = s.trim().to_string();
        if !s.is_empty() {
            out.push(Arg::Bare(s));
        }
    }
    out
}

fn arg_text(a: &Arg) -> String {
    match a {
        Arg::Bare(s) | Arg::Quoted(s) => s.clone(),
        Arg::Kv(_, v) => v.clone(),
    }
}

fn collect_kv(args: &[Arg]) -> Vec<(String, String)> {
    args.iter()
        .filter_map(|a| match a {
            Arg::Kv(k, v) => Some((k.clone(), v.clone())),
            _ => None,
        })
        .collect()
}

fn pick_pos(args: &[Arg], idx: usize) -> Option<&Arg> {
    let mut k = 0;
    for a in args {
        if !matches!(a, Arg::Kv(_, _)) {
            if k == idx {
                return Some(a);
            }
            k += 1;
        }
    }
    None
}

fn pos_text(args: &[Arg], idx: usize) -> String {
    pick_pos(args, idx).map(arg_text).unwrap_or_default()
}

fn dispatch_macro(
    name: &str,
    args: &[Arg],
    opens_boundary: bool,
    diag: &mut C4Diagram,
    stack: &mut Vec<String>,
    current_parent: &mut String,
) {
    // Map macro name → C4 typeC4Shape discriminator (mirrors
    // c4Diagram.jison rules).
    let shape_kind: Option<&'static str> = match name {
        "Person" => Some("person"),
        "Person_Ext" => Some("external_person"),
        "System" => Some("system"),
        "SystemDb" => Some("system_db"),
        "SystemQueue" => Some("system_queue"),
        "System_Ext" => Some("external_system"),
        "SystemDb_Ext" => Some("external_system_db"),
        "SystemQueue_Ext" => Some("external_system_queue"),
        "Container" => Some("container"),
        "ContainerDb" => Some("container_db"),
        "ContainerQueue" => Some("container_queue"),
        "Container_Ext" => Some("external_container"),
        "ContainerDb_Ext" => Some("external_container_db"),
        "ContainerQueue_Ext" => Some("external_container_queue"),
        "Component" => Some("component"),
        "ComponentDb" => Some("component_db"),
        "ComponentQueue" => Some("component_queue"),
        "Component_Ext" => Some("external_component"),
        "ComponentDb_Ext" => Some("external_component_db"),
        "ComponentQueue_Ext" => Some("external_component_queue"),
        _ => None,
    };

    if let Some(kind) = shape_kind {
        let alias = pos_text(args, 0);
        let label = pos_text(args, 1);
        // Containers/Components have techn at position 2 and descr at 3;
        // Persons/Systems have descr at 2.
        let is_container_or_component = matches!(
            kind,
            "container"
                | "container_db"
                | "container_queue"
                | "external_container"
                | "external_container_db"
                | "external_container_queue"
                | "component"
                | "component_db"
                | "component_queue"
                | "external_component"
                | "external_component_db"
                | "external_component_queue"
        );
        let (techn, descr) = if is_container_or_component {
            (pos_text(args, 2), pos_text(args, 3))
        } else {
            (String::new(), pos_text(args, 2))
        };
        let kvs = collect_kv(args);
        let mut sh = C4Shape {
            type_c4_shape: kind.to_string(),
            alias,
            label: C4Text { text: label },
            descr: C4Text { text: descr },
            techn: C4Text { text: techn },
            sprite: kvs.iter().find(|(k, _)| k == "sprite").map(|(_, v)| v.clone()),
            tags: kvs.iter().find(|(k, _)| k == "tags").map(|(_, v)| v.clone()),
            link: kvs.iter().find(|(k, _)| k == "link").map(|(_, v)| v.clone()),
            parent_boundary: current_parent.clone(),
            bg_color: None,
            font_color: None,
            border_color: None,
            shadowing: None,
            shape: None,
            legend_text: None,
            legend_sprite: None,
            wrap: false,
        };
        // Apply $bgColor / $fontColor / $borderColor inline kvs.
        for (k, v) in kvs.iter() {
            match k.as_str() {
                "bgColor" => sh.bg_color = Some(v.clone()),
                "fontColor" => sh.font_color = Some(v.clone()),
                "borderColor" => sh.border_color = Some(v.clone()),
                "shadowing" => sh.shadowing = Some(v.clone()),
                "shape" => sh.shape = Some(v.clone()),
                "legendText" => sh.legend_text = Some(v.clone()),
                "legendSprite" => sh.legend_sprite = Some(v.clone()),
                _ => {}
            }
        }
        diag.shapes.push(sh);
        return;
    }

    // Boundaries.
    let boundary_kind: Option<(&'static str, Option<&'static str>)> = match name {
        "Enterprise_Boundary" => Some(("ENTERPRISE", None)),
        "System_Boundary" => Some(("SYSTEM", None)),
        "Container_Boundary" => Some(("CONTAINER", None)),
        "Boundary" => Some(("system", None)), // default type
        "Deployment_Node" | "Node" => Some(("node", Some("node"))),
        "Node_L" => Some(("node", Some("nodeL"))),
        "Node_R" => Some(("node", Some("nodeR"))),
        _ => None,
    };
    if let Some((default_type, node_type)) = boundary_kind {
        let alias = pos_text(args, 0);
        let label = pos_text(args, 1);
        // For Boundary / non-deployment: optional `type` at pos 2.
        // For Deployment_Node: pos 2 is type, pos 3 is descr.
        let (b_type_text, descr_text): (String, String) = if node_type.is_some() {
            (
                {
                    let t = pos_text(args, 2);
                    if t.is_empty() { default_type.to_string() } else { t }
                },
                pos_text(args, 3),
            )
        } else {
            // For ENTERPRISE/SYSTEM/CONTAINER, default_type is the
            // injected type (overrides any positional).
            if default_type == "ENTERPRISE"
                || default_type == "SYSTEM"
                || default_type == "CONTAINER"
            {
                (default_type.to_string(), String::new())
            } else {
                let t = pos_text(args, 2);
                (
                    if t.is_empty() { default_type.to_string() } else { t },
                    String::new(),
                )
            }
        };
        let kvs = collect_kv(args);
        let mut b = C4Boundary {
            alias: alias.clone(),
            label: C4Text { text: label },
            b_type: C4Text { text: b_type_text },
            descr: C4Text { text: descr_text },
            tags: kvs.iter().find(|(k, _)| k == "tags").map(|(_, v)| v.clone()),
            link: kvs.iter().find(|(k, _)| k == "link").map(|(_, v)| v.clone()),
            parent_boundary: current_parent.clone(),
            node_type: node_type.map(str::to_string),
            bg_color: None,
            font_color: None,
            border_color: None,
            wrap: false,
        };
        for (k, v) in kvs.iter() {
            match k.as_str() {
                "bgColor" => b.bg_color = Some(v.clone()),
                "fontColor" => b.font_color = Some(v.clone()),
                "borderColor" => b.border_color = Some(v.clone()),
                _ => {}
            }
        }
        diag.boundaries.push(b);
        if opens_boundary {
            stack.push(alias.clone());
            *current_parent = alias;
        } else {
            // C4 grammar requires `{`; if missing we still accept and
            // assume an immediate close (no children).
        }
        return;
    }

    // Relationships.
    let rel_kind: Option<&'static str> = match name {
        "Rel" => Some("rel"),
        "BiRel" => Some("birel"),
        "Rel_U" | "Rel_Up" => Some("rel_u"),
        "Rel_D" | "Rel_Down" => Some("rel_d"),
        "Rel_L" | "Rel_Left" => Some("rel_l"),
        "Rel_R" | "Rel_Right" => Some("rel_r"),
        "Rel_Back" => Some("rel_b"),
        "RelIndex" => Some("rel"),
        _ => None,
    };
    if let Some(kind) = rel_kind {
        // RelIndex: drop first positional (the index number).
        let offset = if name == "RelIndex" { 1 } else { 0 };
        let from = pos_text(args, offset);
        let to = pos_text(args, offset + 1);
        let label = pos_text(args, offset + 2);
        let techn = pos_text(args, offset + 3);
        let descr = pos_text(args, offset + 4);
        let kvs = collect_kv(args);
        diag.rels.push(C4Rel {
            rel_type: kind.to_string(),
            from,
            to,
            label: C4Text { text: label },
            techn: C4Text { text: techn },
            descr: C4Text { text: descr },
            sprite: kvs.iter().find(|(k, _)| k == "sprite").map(|(_, v)| v.clone()),
            tags: kvs.iter().find(|(k, _)| k == "tags").map(|(_, v)| v.clone()),
            link: kvs.iter().find(|(k, _)| k == "link").map(|(_, v)| v.clone()),
            text_color: None,
            line_color: None,
            offset_x: None,
            offset_y: None,
            wrap: false,
        });
        return;
    }

    // Style updates.
    if name == "UpdateElementStyle" {
        let alias = pos_text(args, 0);
        let kvs = collect_kv(args);
        // Find shape or boundary by alias.
        if let Some(sh) = diag.shapes.iter_mut().find(|s| s.alias == alias) {
            for (k, v) in kvs {
                match k.as_str() {
                    "bgColor" => sh.bg_color = Some(v),
                    "fontColor" => sh.font_color = Some(v),
                    "borderColor" => sh.border_color = Some(v),
                    "shadowing" => sh.shadowing = Some(v),
                    "shape" => sh.shape = Some(v),
                    "legendText" => sh.legend_text = Some(v),
                    "legendSprite" => sh.legend_sprite = Some(v),
                    _ => {}
                }
            }
        } else if let Some(bn) = diag.boundaries.iter_mut().find(|b| b.alias == alias) {
            for (k, v) in kvs {
                match k.as_str() {
                    "bgColor" => bn.bg_color = Some(v),
                    "fontColor" => bn.font_color = Some(v),
                    "borderColor" => bn.border_color = Some(v),
                    _ => {}
                }
            }
        }
        return;
    }
    if name == "UpdateRelStyle" {
        let from = pos_text(args, 0);
        let to = pos_text(args, 1);
        let kvs = collect_kv(args);
        if let Some(r) = diag.rels.iter_mut().find(|r| r.from == from && r.to == to) {
            for (k, v) in kvs {
                match k.as_str() {
                    "textColor" => r.text_color = Some(v),
                    "lineColor" => r.line_color = Some(v),
                    "offsetX" => r.offset_x = v.parse().ok(),
                    "offsetY" => r.offset_y = v.parse().ok(),
                    _ => {}
                }
            }
        }
        return;
    }
    if name == "UpdateLayoutConfig" {
        let kvs = collect_kv(args);
        for (k, v) in kvs {
            match k.as_str() {
                "c4ShapeInRow" => {
                    diag.c4_shape_in_row = v.parse().ok();
                }
                "c4BoundaryInRow" => {
                    diag.c4_boundary_in_row = v.parse().ok();
                }
                _ => {}
            }
        }
    }

    // direction TB/BT/LR/RL — silently ignored for now.
}

// ── Tiny char parser ─────────────────────────────────────────────────

struct Parser<'a> {
    s: &'a str,
    i: usize,
}

impl<'a> Parser<'a> {
    fn new(s: &'a str) -> Self {
        Self { s, i: 0 }
    }
    fn at_eof(&self) -> bool {
        self.i >= self.s.len()
    }
    fn peek(&self) -> Option<u8> {
        self.s.as_bytes().get(self.i).copied()
    }
    fn advance(&mut self, n: usize) {
        self.i = (self.i + n).min(self.s.len());
    }
    fn eat_char(&mut self, c: char) -> bool {
        if self.peek() == Some(c as u8) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn skip_inline_ws(&mut self) {
        while let Some(b) = self.peek() {
            if b == b' ' || b == b'\t' {
                self.i += 1;
            } else {
                break;
            }
        }
    }
    fn skip_ws_and_comments(&mut self) {
        loop {
            while let Some(b) = self.peek() {
                if matches!(b, b' ' | b'\t' | b'\r' | b'\n') {
                    self.i += 1;
                } else {
                    break;
                }
            }
            // `%%` to end-of-line comment.
            if self.peek() == Some(b'%') && self.s.as_bytes().get(self.i + 1) == Some(&b'%') {
                while let Some(b) = self.peek() {
                    self.i += 1;
                    if b == b'\n' {
                        break;
                    }
                }
                continue;
            }
            break;
        }
    }
    fn read_to_eol(&mut self) -> String {
        let start = self.i;
        while let Some(b) = self.peek() {
            if b == b'\n' {
                break;
            }
            self.i += 1;
        }
        let out = self.s[start..self.i].to_string();
        if self.peek() == Some(b'\n') {
            self.i += 1;
        }
        out
    }
    fn read_until_char(&mut self, c: char) -> String {
        let start = self.i;
        while let Some(b) = self.peek() {
            if b == c as u8 {
                break;
            }
            self.i += 1;
        }
        self.s[start..self.i].to_string()
    }
    fn read_until_set(&mut self, set: &str) -> String {
        let start = self.i;
        while let Some(b) = self.peek() {
            if set.as_bytes().contains(&b) || b == b'\n' {
                break;
            }
            self.i += 1;
        }
        self.s[start..self.i].to_string()
    }
    fn read_ident(&mut self) -> String {
        let start = self.i;
        while let Some(b) = self.peek() {
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.i += 1;
            } else {
                break;
            }
        }
        self.s[start..self.i].to_string()
    }
    fn peek_keyword(&self, kw: &str) -> bool {
        let bytes = self.s.as_bytes();
        let kb = kw.as_bytes();
        if self.i + kb.len() > bytes.len() {
            return false;
        }
        if &bytes[self.i..self.i + kb.len()] != kb {
            return false;
        }
        // Followed by non-ident char (or EOF).
        match bytes.get(self.i + kb.len()) {
            None => true,
            Some(b) => !(b.is_ascii_alphanumeric() || *b == b'_'),
        }
    }
    fn consume_header(&mut self) -> Result<(), MermaidError> {
        // Skip any leading whitespace/comments, then the first
        // C4Context/Container/Component/Dynamic/Deployment word.
        self.skip_ws_and_comments();
        let _ = self.read_ident();
        let _ = self.read_to_eol();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_context_with_nested_boundaries() {
        let src = r#"C4Context
title System Context diagram
Enterprise_Boundary(b0, "BankBoundary0") {
    Person(customerA, "Banking Customer A", "A customer.")
    System(SystemAA, "Internet Banking System", "Allows customers to view info.")
    Enterprise_Boundary(b1, "BankBoundary") {
      System_Ext(SystemC, "E-mail system", "Internal Microsoft Exchange.")
    }
}
BiRel(customerA, SystemAA, "Uses")
Rel(SystemAA, SystemC, "Sends e-mails", "SMTP")
UpdateRelStyle(customerA, SystemAA, $textColor="blue", $offsetX="5")
"#;
        let d = parse(src).expect("parse ok");
        assert_eq!(d.subtype, C4Subtype::Context);
        assert_eq!(d.shapes.len(), 3);
        assert_eq!(d.boundaries.len(), 3); // global + b0 + b1
        assert_eq!(d.rels.len(), 2);
        // Nested boundary chain: SystemC.parent == b1, b1.parent == b0,
        // b0.parent == global.
        let sc = d.shapes.iter().find(|s| s.alias == "SystemC").unwrap();
        assert_eq!(sc.parent_boundary, "b1");
        let b1 = d.boundaries.iter().find(|b| b.alias == "b1").unwrap();
        assert_eq!(b1.parent_boundary, "b0");
        let b0 = d.boundaries.iter().find(|b| b.alias == "b0").unwrap();
        assert_eq!(b0.parent_boundary, "global");
        // UpdateRelStyle reflected back into the rel.
        let r = d
            .rels
            .iter()
            .find(|r| r.from == "customerA" && r.to == "SystemAA")
            .unwrap();
        assert_eq!(r.text_color.as_deref(), Some("blue"));
        assert_eq!(r.offset_x, Some(5));
        assert_eq!(d.meta.title.as_deref(), Some("System Context diagram"));
    }

    #[test]
    fn parse_deployment_node_chain() {
        let src = r#"C4Deployment
Deployment_Node(plc, "Big Bank plc", "Big Bank plc data center"){
    Deployment_Node(dn, "bigbank-api*** x8", "Ubuntu 16.04 LTS"){
        Container(api, "API Application", "Java", "JSON/HTTPS API.")
    }
}
"#;
        let d = parse(src).unwrap();
        assert_eq!(d.subtype, C4Subtype::Deployment);
        assert_eq!(d.shapes.len(), 1);
        // global + plc + dn
        assert_eq!(d.boundaries.len(), 3);
        let api = &d.shapes[0];
        assert_eq!(api.parent_boundary, "dn");
        assert_eq!(api.techn.text, "Java");
        assert_eq!(api.descr.text, "JSON/HTTPS API.");
        let dn = d.boundaries.iter().find(|b| b.alias == "dn").unwrap();
        assert_eq!(dn.node_type.as_deref(), Some("node"));
        assert_eq!(dn.b_type.text, "Ubuntu 16.04 LTS");
    }
}
