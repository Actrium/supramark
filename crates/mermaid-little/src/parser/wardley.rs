//! Hand-rolled parser for the `wardley-beta` diagram.
//!
//! Upstream grammar:
//!   /ext/mermaid-official-stable-v11.14.0/packages/parser/src/language/wardley/wardley.langium
//! Upstream populate-DB shim:
//!   /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/wardley/wardleyParser.ts
//!
//! Lines recognised (covers every fixture under
//! tests/ext_fixtures/{cypress,demos}/wardley):
//!
//! ```text
//! wardley-beta
//! title <free text>
//! size [W, H]
//! evolution S1 -> S2 -> ... (optional `@boundary` / `/ secondName`)
//! anchor <name> [<visibility>, <evolution>]
//! component <name> [<vis>, <evo>] label [<dx>, <dy>]? (strategy)? inertia?
//! pipeline <parent> { component <name> [<evo>] label [<dx>, <dy>]? ... }
//! <name> [+<>|+>|+<]? (->|-->|-.->|>|+'label'>|+'label'<|+'label'<>) <name> [+<>|+>|+<]? (;label)?
//! evolve <name> <target-evolution>
//! note "text" [<vis>, <evo>]
//! annotations [<x>, <y>]
//! annotation N, [<x>, <y>] "text"
//! accelerator <name> [<x>, <y>]
//! deaccelerator <name> [<x>, <y>]
//! ```
//!
//! We keep the parser simple and line-oriented. Names can contain
//! spaces — upstream's NAME_WITH_SPACES pattern — so where a name is
//! expected, we capture greedily up to a delimiter (`[`, `->`, `-->`,
//! `-.->`, `>`, `+<`/`+>`/`+<>`, `;`, or EOL).

use crate::error::{MermaidError, Result};
use crate::model::wardley::{
    LinkFlow, SourceStrategy, WardleyAccelerator, WardleyAnnotation, WardleyAxesConfig,
    WardleyDeaccelerator, WardleyDiagram, WardleyLink, WardleyNode, WardleyNote, WardleyPipeline,
    WardleyTrend,
};

/// Parse a mermaid `wardley-beta` source into a [`WardleyDiagram`].
pub fn parse(source: &str) -> Result<WardleyDiagram> {
    let mut diagram = WardleyDiagram::default();

    // Collapse source into logical lines. We also pre-join lines that
    // belong to a `pipeline { ... }` block so the core dispatcher can
    // treat that block as one unit.
    let lines: Vec<&str> = source.lines().collect();

    // State flag: reject any statement before the `wardley-beta` header
    // (mirrors upstream Langium: `entry Wardley: NEWLINE* KW_WARDLEY ...`).
    let mut saw_header = false;

    let mut i = 0usize;
    while i < lines.len() {
        let raw = lines[i];
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            i += 1;
            continue;
        }

        if !saw_header {
            if line == "wardley-beta" || line.starts_with("wardley-beta") {
                saw_header = true;
                i += 1;
                continue;
            }
            return Err(MermaidError::Parse {
                line: i + 1,
                col: 1,
                message: "expected 'wardley-beta' header".into(),
            });
        }

        // Branch on the leading keyword (or interpret as a link).
        if let Some(rest) = strip_kw(line, "title") {
            diagram.meta.title = Some(rest.trim().to_string());
        } else if let Some(rest) = strip_kw(line, "accTitle:") {
            diagram.meta.acc_title = Some(rest.trim().to_string());
        } else if let Some(rest) = strip_kw(line, "accDescr:") {
            diagram.meta.acc_descr = Some(rest.trim().to_string());
        } else if let Some(rest) = strip_kw(line, "size") {
            diagram.size = Some(parse_size(rest)?);
        } else if let Some(rest) = strip_kw(line, "evolution") {
            parse_evolution_line(&mut diagram.axes, rest)?;
        } else if let Some(rest) = strip_kw(line, "anchor") {
            let n = parse_anchor(rest)?;
            upsert_node(&mut diagram.nodes, n);
        } else if let Some(rest) = strip_kw(line, "component") {
            let n = parse_component(rest)?;
            upsert_node(&mut diagram.nodes, n);
        } else if let Some(rest) = strip_kw(line, "evolve") {
            parse_evolve(&mut diagram, rest)?;
        } else if let Some(rest) = strip_kw(line, "note") {
            let note = parse_note(rest)?;
            diagram.notes.push(note);
        } else if let Some(rest) = strip_kw(line, "annotations") {
            let (x, y) = parse_coords(rest)?;
            // Second value is a visibility; convert like upstream's
            // `toCoordinates(x, y)` — evolution first as stored.
            let (x_pct, y_pct) = (to_percent(y), to_percent(x));
            diagram.annotations_box = Some((x_pct, y_pct));
        } else if let Some(rest) = strip_kw(line, "annotation") {
            let ann = parse_annotation(rest)?;
            diagram.annotations.push(ann);
        } else if let Some(rest) = strip_kw(line, "accelerator") {
            let acc = parse_accelerator(rest)?;
            diagram.accelerators.push(acc);
        } else if let Some(rest) = strip_kw(line, "deaccelerator") {
            let d = parse_deaccelerator(rest)?;
            diagram.deaccelerators.push(d);
        } else if let Some(rest) = strip_kw(line, "pipeline") {
            // Collect lines through the matching `}` then parse them.
            let mut block_lines = Vec::new();
            // Find opening brace position — could be on same line.
            if let Some(_brace_idx) = rest.find('{') {
                // Any content after `{` on this line becomes part of
                // the block body, too.
                let after_brace = rest.split_once('{').map(|(_, r)| r).unwrap_or("");
                let inner_first = after_brace.trim();
                if !inner_first.is_empty() && inner_first != "}" {
                    block_lines.push(inner_first.to_string());
                }
            } else {
                // Opening brace on following line(s).
                let mut j = i + 1;
                loop {
                    if j >= lines.len() {
                        return Err(MermaidError::Parse {
                            line: i + 1,
                            col: 1,
                            message: "pipeline: missing '{'".into(),
                        });
                    }
                    let l = lines[j].trim();
                    if l.starts_with('{') {
                        i = j;
                        break;
                    }
                    if !l.is_empty() {
                        return Err(MermaidError::Parse {
                            line: j + 1,
                            col: 1,
                            message: "pipeline: expected '{'".into(),
                        });
                    }
                    j += 1;
                }
            }

            let parent_name = rest
                .split_once('{')
                .map(|(name, _)| name.trim().to_string())
                .unwrap_or_else(|| rest.trim().to_string());

            let mut j = i + 1;
            while j < lines.len() {
                let l = lines[j].trim();
                if l.is_empty() {
                    j += 1;
                    continue;
                }
                if l == "}" || l.starts_with('}') {
                    break;
                }
                block_lines.push(l.to_string());
                j += 1;
            }
            if j >= lines.len() || !lines[j].trim().starts_with('}') {
                return Err(MermaidError::Parse {
                    line: i + 1,
                    col: 1,
                    message: "pipeline: missing '}'".into(),
                });
            }

            parse_pipeline_block(&mut diagram, &parent_name, &block_lines)?;
            i = j;
        } else if looks_like_link(line) {
            let link = parse_link(line)?;
            diagram.links.push(link);
        } else {
            return Err(MermaidError::Parse {
                line: i + 1,
                col: 1,
                message: format!("unrecognised line: {line}"),
            });
        }

        i += 1;
    }

    Ok(diagram)
}

// ─────────────────────────────────────────────────────────────────────────────
// Keyword helpers.
// ─────────────────────────────────────────────────────────────────────────────

/// If `line` begins with `kw` followed by a word-boundary (space, `[`,
/// `:`, or EOL), return the remainder (trimmed of the leading space).
fn strip_kw<'a>(line: &'a str, kw: &str) -> Option<&'a str> {
    if !line.starts_with(kw) {
        return None;
    }
    let rest = &line[kw.len()..];
    // Accept: end-of-line, space, tab, or the `[` / `{` / `,` that
    // appears directly after the keyword in some grammars.
    match rest.chars().next() {
        None => Some(rest),
        Some(c) if c.is_whitespace() => Some(rest.trim_start()),
        Some('[') | Some('{') | Some(',') => Some(rest),
        _ => None,
    }
}

/// Convert a 0-1 (decimal) or 0-100 value to the 0-100 percentage
/// representation used throughout the layout, matching upstream's
/// `toPercent`.
fn to_percent(value: f64) -> f64 {
    if value <= 1.0 {
        value * 100.0
    } else {
        value
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Statement parsers.
// ─────────────────────────────────────────────────────────────────────────────

fn parse_size(rest: &str) -> Result<(i64, i64)> {
    let (a, b) = parse_bracket_pair(rest)?;
    let w = a.parse::<f64>().map_err(|_| parse_err("size width"))?;
    let h = b.parse::<f64>().map_err(|_| parse_err("size height"))?;
    Ok((w as i64, h as i64))
}

fn parse_evolution_line(axes: &mut WardleyAxesConfig, rest: &str) -> Result<()> {
    // `name1 -> name2 -> name3 -> ...` with optional `@0.25` boundary.
    let stages_raw: Vec<&str> = rest.split("->").map(|s| s.trim()).collect();
    let mut stages: Vec<String> = Vec::with_capacity(stages_raw.len());
    let mut boundaries: Vec<f64> = Vec::new();
    for stage in stages_raw {
        let (name_part, boundary) = if let Some(idx) = stage.find('@') {
            let b = stage[idx + 1..]
                .trim()
                .parse::<f64>()
                .map_err(|_| parse_err("evolution @boundary"))?;
            (stage[..idx].trim(), Some(b))
        } else {
            (stage, None)
        };
        // Dual-label `Name1 / Name2` pattern — keep full string.
        let normalised = name_part
            .split('/')
            .map(|p| p.trim())
            .collect::<Vec<_>>()
            .join(" / ");
        stages.push(normalised);
        if let Some(b) = boundary {
            boundaries.push(b);
        }
    }
    axes.stages = stages;
    if !boundaries.is_empty() {
        axes.stage_boundaries = boundaries;
    }
    Ok(())
}

fn parse_anchor(rest: &str) -> Result<WardleyNode> {
    let (name, coords) = split_name_brackets(rest)?;
    let (vis, evo) = parse_coord_pair(coords)?;
    let name = name.trim().to_string();
    Ok(WardleyNode {
        id: name.clone(),
        label: name,
        x: to_percent(evo),
        y: to_percent(vis),
        class_name: Some("anchor".to_string()),
        ..Default::default()
    })
}

fn parse_component(rest: &str) -> Result<WardleyNode> {
    // Format: name [vis, evo] label [dx, dy]? (strategy)? inertia?
    let (name, after_name) = split_name_bracket_region(rest)?;
    // The bracket region itself — after_name begins at `[`.
    let (coords, after_coords) = take_bracket_pair(after_name)?;
    let (vis, evo) = parse_coord_pair(coords)?;

    // Optional `label [dx, dy]`.
    let mut cursor = after_coords.trim_start();
    let (label_ox, label_oy, cursor_next) = if let Some(rem) = cursor.strip_prefix("label") {
        let rem = rem.trim_start();
        let (inside, rest_after) = take_bracket_pair(rem)?;
        let parts: Vec<&str> = inside.split(',').map(|s| s.trim()).collect();
        if parts.len() != 2 {
            return Err(parse_err("label offsets"));
        }
        let dx = parts[0].parse::<i64>().map_err(|_| parse_err("label dx"))?;
        let dy = parts[1].parse::<i64>().map_err(|_| parse_err("label dy"))?;
        (Some(dx), Some(dy), rest_after.trim_start())
    } else {
        (None, None, cursor)
    };
    cursor = cursor_next;

    // Optional `(strategy)` or `(inertia)`.
    let mut strategy: Option<SourceStrategy> = None;
    let mut inertia = false;
    if let Some(rem) = cursor.strip_prefix('(') {
        if let Some(end) = rem.find(')') {
            let inside = rem[..end].trim();
            match inside {
                "build" => strategy = Some(SourceStrategy::Build),
                "buy" => strategy = Some(SourceStrategy::Buy),
                "outsource" => strategy = Some(SourceStrategy::Outsource),
                "market" => strategy = Some(SourceStrategy::Market),
                "inertia" => inertia = true,
                other => return Err(parse_err(&format!("component decorator: {other}"))),
            }
            cursor = rem[end + 1..].trim_start();
        }
    }

    // Bare `inertia` after decorator.
    if cursor == "inertia" {
        inertia = true;
    }

    let name = name.trim().to_string();
    Ok(WardleyNode {
        id: name.clone(),
        label: name,
        x: to_percent(evo),
        y: to_percent(vis),
        class_name: Some("component".to_string()),
        label_offset_x: label_ox,
        label_offset_y: label_oy,
        inertia,
        source_strategy: strategy,
        ..Default::default()
    })
}

fn parse_evolve(diagram: &mut WardleyDiagram, rest: &str) -> Result<()> {
    // `name target`. Walk from the end to split off the numeric target.
    let trimmed = rest.trim();
    let last_space = trimmed
        .rfind(char::is_whitespace)
        .ok_or_else(|| parse_err("evolve: missing target"))?;
    let name = trimmed[..last_space].trim();
    let target = trimmed[last_space + 1..]
        .trim()
        .parse::<f64>()
        .map_err(|_| parse_err("evolve target"))?;
    let node = diagram.get_node(name).ok_or_else(|| MermaidError::Parse {
        line: 0,
        col: 0,
        message: format!("evolve references unknown component '{name}'"),
    })?;
    let node_y = node.y;
    diagram.trends.push(WardleyTrend {
        node_id: name.to_string(),
        target_x: to_percent(target),
        target_y: node_y,
    });
    Ok(())
}

fn parse_note(rest: &str) -> Result<WardleyNote> {
    // `"text" [vis, evo]`
    let rest = rest.trim_start();
    let rest = rest
        .strip_prefix('"')
        .ok_or_else(|| parse_err("note quote"))?;
    let end = rest
        .find('"')
        .ok_or_else(|| parse_err("note close-quote"))?;
    let text = rest[..end].to_string();
    let after = rest[end + 1..].trim();
    let (vis, evo) = parse_coord_pair_brackets(after)?;
    Ok(WardleyNote {
        text,
        x: to_percent(evo),
        y: to_percent(vis),
    })
}

fn parse_annotation(rest: &str) -> Result<WardleyAnnotation> {
    // `N, [x, y] "text"`
    let (num_part, rest1) = rest
        .split_once(',')
        .ok_or_else(|| parse_err("annotation number"))?;
    let number = num_part
        .trim()
        .parse::<i64>()
        .map_err(|_| parse_err("annotation N"))?;
    let rest1 = rest1.trim_start();
    let (coords, after) = take_bracket_pair(rest1)?;
    let parts: Vec<&str> = coords.split(',').map(|s| s.trim()).collect();
    if parts.len() != 2 {
        return Err(parse_err("annotation coords"));
    }
    let x = parts[0]
        .parse::<f64>()
        .map_err(|_| parse_err("annotation x"))?;
    let y = parts[1]
        .parse::<f64>()
        .map_err(|_| parse_err("annotation y"))?;
    // upstream: toCoordinates(x, y) -> {x: evo, y: vis}
    let (xp, yp) = (to_percent(y), to_percent(x));
    let after = after.trim_start();
    let text = if let Some(q) = after.strip_prefix('"') {
        let end = q.find('"').ok_or_else(|| parse_err("annotation text"))?;
        Some(q[..end].to_string())
    } else {
        None
    };
    Ok(WardleyAnnotation {
        number,
        coordinates: vec![(xp, yp)],
        text,
    })
}

fn parse_accelerator(rest: &str) -> Result<WardleyAccelerator> {
    let (name, after) = split_name_bracket_region(rest)?;
    let (coords, _) = take_bracket_pair(after)?;
    let (a, b) = parse_coord_pair(coords)?;
    // upstream: toCoordinates(x, y) where first bracket is `x` and second `y`.
    // However grammar names the first operand `x` and second `y` —
    // consistent with `Accelerator: name '[' x ',' y ']'`. Apply same
    // swap as other coord consumers.
    Ok(WardleyAccelerator {
        name: name.trim().to_string(),
        x: to_percent(b),
        y: to_percent(a),
    })
}

fn parse_deaccelerator(rest: &str) -> Result<WardleyDeaccelerator> {
    let (name, after) = split_name_bracket_region(rest)?;
    let (coords, _) = take_bracket_pair(after)?;
    let (a, b) = parse_coord_pair(coords)?;
    Ok(WardleyDeaccelerator {
        name: name.trim().to_string(),
        x: to_percent(b),
        y: to_percent(a),
    })
}

fn parse_pipeline_block(
    diagram: &mut WardleyDiagram,
    parent_name: &str,
    block_lines: &[String],
) -> Result<()> {
    let parent = diagram
        .get_node(parent_name)
        .ok_or_else(|| MermaidError::Parse {
            line: 0,
            col: 0,
            message: format!("pipeline references unknown parent '{parent_name}'"),
        })?;
    let parent_y = parent.y;

    // Mark parent as pipeline parent.
    if let Some(p) = diagram.get_node_mut(parent_name) {
        p.is_pipeline_parent = true;
    }

    let mut pipeline = WardleyPipeline {
        node_id: parent_name.to_string(),
        component_ids: Vec::new(),
    };

    for line in block_lines {
        let line = line.trim();
        if line.is_empty() || line == "}" {
            continue;
        }
        if let Some(rest) = strip_kw(line, "component") {
            // `name [evo] label [dx, dy]?`
            let (name, after) = split_name_bracket_region(rest)?;
            let (inside, after2) = take_bracket_pair(after)?;
            let evo = inside
                .trim()
                .parse::<f64>()
                .map_err(|_| parse_err("pipeline component evo"))?;

            // Optional label offsets.
            let mut label_ox = None;
            let mut label_oy = None;
            let after2 = after2.trim_start();
            if let Some(rem) = after2.strip_prefix("label") {
                let rem = rem.trim_start();
                let (inside, _) = take_bracket_pair(rem)?;
                let parts: Vec<&str> = inside.split(',').map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    label_ox = parts[0].parse::<i64>().ok();
                    label_oy = parts[1].parse::<i64>().ok();
                }
            }

            let child_name = name.trim().to_string();
            let child_id = format!("{}_{}", parent_name, child_name);

            let mut node = WardleyNode {
                id: child_id.clone(),
                label: child_name,
                x: to_percent(evo),
                y: parent_y,
                class_name: Some("pipeline-component".to_string()),
                label_offset_x: label_ox,
                label_offset_y: label_oy,
                in_pipeline: true,
                ..Default::default()
            };
            // Merge rule: upsert (pipeline components normally don't
            // collide with existing nodes because of the compound id).
            upsert_node_existing(&mut diagram.nodes, &mut node);
            pipeline.component_ids.push(child_id);
        }
    }

    diagram.pipelines.push(pipeline);
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Link parsing.
// ─────────────────────────────────────────────────────────────────────────────

fn looks_like_link(line: &str) -> bool {
    // A link line contains at least one of the arrow literals OR a
    // `+'label'...` arrow, and is not a keyword statement.
    if line.contains("-.->") || line.contains("-->") || line.contains("->") {
        return true;
    }
    // Plain `+> / +< / +<>` alone won't match as a link; same with
    // `>`. Fixtures include `+>`, `+'backup'>`, etc.
    if line.contains("+>") || line.contains("+<") || line.contains("+<>") {
        return true;
    }
    if line.contains("+'") {
        return true;
    }
    // Standalone `>`.
    if line.contains('>') {
        return true;
    }
    false
}

fn parse_link(line: &str) -> Result<WardleyLink> {
    // Split off a trailing `;label` (plain annotation) first.
    let (body, trailing_label) = if let Some(idx) = line.find(';') {
        let label = line[idx + 1..].trim().to_string();
        (&line[..idx], Some(label))
    } else {
        (line, None)
    };
    let body = body.trim();

    // Upstream grammar:
    //   from fromPort? arrow? to toPort? linkLabel?
    //   arrow  : '->' | '-->' | '-.->' | '>' | /\+'[^']*'<>/
    //          | /\+'[^']*'</ | /\+'[^']*'>/
    //   port   : '+<>' | '+>' | '+<'
    //
    // The arrow is optional, so `App +> API` has fromPort='+>', no
    // arrow. We scan the string left-to-right looking for the first
    // separator. Any `+'...'<>/`/`<`/`>` is treated as an arrow;
    // bare `+<>`, `+>`, `+<` become fromPort; remaining plain arrows
    // (`->`, `-->`, `-.->`, `>`) are arrows; a `+...` right after the
    // to-name becomes toPort.

    // Find the separator between from and to.
    let sep = find_link_separator(body)?;

    // Everything to the left of `sep.start` is the from-side; right of
    // `sep.end` is the to-side.
    let left = body[..sep.start].trim();
    let right = body[sep.end..].trim();

    // Right side may still have a leading/trailing port.
    let (to_port, to) = split_leading_port(right);

    // Left side: strip leading whitespace then the name.
    let from = left.to_string();

    // Extract arrow-specific fields.
    let dashed = sep.literal.contains("-.->") || sep.literal.contains(".-.");
    let (flow_from_arrow, label_from_arrow) = extract_flow_from_arrow(sep.literal);

    let from_port_flow = sep.from_port.as_deref().and_then(port_to_flow);
    let to_port_flow = to_port.as_deref().and_then(port_to_flow);

    let flow = from_port_flow.or(to_port_flow).or(flow_from_arrow);
    let label = label_from_arrow.or(trailing_label);

    Ok(WardleyLink {
        source: from,
        target: to.to_string(),
        dashed,
        label,
        flow,
    })
}

/// Result of splitting `from [fromPort] [arrow] to [toPort] [label]`.
///
/// `start/end` delimit the span that separates `from` and `to` — i.e.
/// everything that isn't a name. That span may contain zero or more of:
///   - a port attached to `from` (captured in `from_port`),
///   - an arrow literal (`literal`, empty string when no arrow).
///
/// Note: the to-port is discovered later by [`split_leading_port`] on
/// the remainder after `end`.
struct LinkSeparator<'a> {
    start: usize,
    end: usize,
    literal: &'a str,
    from_port: Option<String>,
}

fn find_link_separator(body: &str) -> Result<LinkSeparator<'_>> {
    // Scan for a separator anchor (one of the recognised tokens) in
    // preference order: longer/more-specific first.
    //
    // We look for these tokens in order:
    //   1. `+'...'<>`, `+'...'>`, `+'...'<`  (full arrow with label)
    //   2. `-.->`, `-->`                       (long dashed/plain arrows)
    //   3. `->`                                (simple arrow)
    //   4. `+<>`, `+>`, `+<`                   (port on source + implicit arrow)
    //   5. `>`                                 (bare arrow)

    // Try labeled arrow first.
    if let Some(pos) = body.find("+'") {
        let after = &body[pos + 2..];
        if let Some(rel) = after.find('\'') {
            let abs_close = pos + 2 + rel;
            let tail = &body[abs_close + 1..];
            let tail_len = if tail.starts_with("<>") {
                2
            } else if tail.starts_with('<') || tail.starts_with('>') {
                1
            } else {
                0
            };
            if tail_len > 0 {
                let end = abs_close + 1 + tail_len;
                return Ok(LinkSeparator {
                    start: pos,
                    end,
                    literal: &body[pos..end],
                    from_port: None,
                });
            }
        }
    }

    // Long plain arrows.
    for tok in &["-.->", "-->"] {
        if let Some(pos) = body.find(tok) {
            return Ok(LinkSeparator {
                start: pos,
                end: pos + tok.len(),
                literal: tok,
                from_port: None,
            });
        }
    }

    // Simple `->`.
    if let Some(pos) = body.find("->") {
        return Ok(LinkSeparator {
            start: pos,
            end: pos + 2,
            literal: "->",
            from_port: None,
        });
    }

    // Source ports followed by a name (no explicit arrow).
    //   `App +> API` → from="App", from_port="+>", literal="", to="API"
    for tok in &["+<>", "+>", "+<"] {
        // Find the port token preceded by whitespace (so "A+>B" would
        // not match, but we don't expect that pattern).
        if let Some(pos) = body.find(tok) {
            let end = pos + tok.len();
            return Ok(LinkSeparator {
                start: pos,
                end,
                literal: "",
                from_port: Some((*tok).to_string()),
            });
        }
    }

    // Bare `>` as arrow (grammar's `LINK_ARROW: ... | '>' | ...`).
    if let Some(pos) = body.find('>') {
        return Ok(LinkSeparator {
            start: pos,
            end: pos + 1,
            literal: ">",
            from_port: None,
        });
    }

    Err(parse_err("link: no arrow token found"))
}

fn split_leading_port(s: &str) -> (Option<String>, &str) {
    for tok in &["+<>", "+>", "+<"] {
        if let Some(stripped) = s.strip_prefix(tok) {
            return (Some((*tok).to_string()), stripped.trim_start());
        }
    }
    (None, s)
}

fn port_to_flow(p: &str) -> Option<LinkFlow> {
    match p {
        "+<>" => Some(LinkFlow::Bidirectional),
        "+<" => Some(LinkFlow::Backward),
        "+>" => Some(LinkFlow::Forward),
        _ => None,
    }
}

fn extract_flow_from_arrow(arrow: &str) -> (Option<LinkFlow>, Option<String>) {
    if !arrow.starts_with('+') {
        return (None, None);
    }
    // `+'label'<>` / `+'label'>` / `+'label'<`
    let after_quote = arrow.trim_start_matches('+');
    let inner = after_quote
        .strip_prefix('\'')
        .and_then(|body| body.find('\'').map(|end| body[..end].to_string()));
    let flow = if arrow.contains("<>") {
        Some(LinkFlow::Bidirectional)
    } else if arrow.contains('<') {
        Some(LinkFlow::Backward)
    } else if arrow.contains('>') {
        Some(LinkFlow::Forward)
    } else {
        None
    };
    (flow, inner)
}

// ─────────────────────────────────────────────────────────────────────────────
// Bracket / coord helpers.
// ─────────────────────────────────────────────────────────────────────────────

fn parse_bracket_pair(rest: &str) -> Result<(&str, &str)> {
    let rest = rest.trim_start();
    let rest = rest
        .strip_prefix('[')
        .ok_or_else(|| parse_err("expected '['"))?;
    let end = rest.find(']').ok_or_else(|| parse_err("expected ']'"))?;
    let inside = rest[..end].trim();
    let (a, b) = inside
        .split_once(',')
        .ok_or_else(|| parse_err("expected comma in bracket pair"))?;
    Ok((a.trim(), b.trim()))
}

fn take_bracket_pair(rest: &str) -> Result<(&str, &str)> {
    let rest = rest.trim_start();
    let rest = rest
        .strip_prefix('[')
        .ok_or_else(|| parse_err("expected '['"))?;
    let end = rest.find(']').ok_or_else(|| parse_err("expected ']'"))?;
    Ok((rest[..end].trim(), &rest[end + 1..]))
}

fn parse_coords(rest: &str) -> Result<(f64, f64)> {
    let (a, b) = parse_bracket_pair(rest)?;
    let a = a.parse::<f64>().map_err(|_| parse_err("coord a"))?;
    let b = b.parse::<f64>().map_err(|_| parse_err("coord b"))?;
    Ok((a, b))
}

fn parse_coord_pair(inside: &str) -> Result<(f64, f64)> {
    let (a, b) = inside
        .split_once(',')
        .ok_or_else(|| parse_err("coord pair comma"))?;
    let a = a.trim().parse::<f64>().map_err(|_| parse_err("coord a"))?;
    let b = b.trim().parse::<f64>().map_err(|_| parse_err("coord b"))?;
    Ok((a, b))
}

fn parse_coord_pair_brackets(rest: &str) -> Result<(f64, f64)> {
    let (a, b) = parse_bracket_pair(rest)?;
    let a = a.parse::<f64>().map_err(|_| parse_err("coord a"))?;
    let b = b.parse::<f64>().map_err(|_| parse_err("coord b"))?;
    Ok((a, b))
}

/// Split `name [...]` where name may contain spaces.
fn split_name_brackets(rest: &str) -> Result<(&str, &str)> {
    let idx = rest.find('[').ok_or_else(|| parse_err("expected '['"))?;
    let end = rest[idx..]
        .find(']')
        .ok_or_else(|| parse_err("expected ']'"))?;
    Ok((&rest[..idx], &rest[idx + 1..idx + end]))
}

/// Like [`split_name_brackets`] but returns the remainder starting at
/// the `[` so the caller can use [`take_bracket_pair`].
fn split_name_bracket_region(rest: &str) -> Result<(&str, &str)> {
    let idx = rest.find('[').ok_or_else(|| parse_err("expected '['"))?;
    Ok((&rest[..idx], &rest[idx..]))
}

fn parse_err(msg: &str) -> MermaidError {
    MermaidError::Parse {
        line: 0,
        col: 0,
        message: msg.to_string(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Node upsert helpers (preserve insertion order).
// ─────────────────────────────────────────────────────────────────────────────

fn upsert_node(vec: &mut Vec<WardleyNode>, node: WardleyNode) {
    if let Some(existing) = vec.iter_mut().find(|n| n.id == node.id) {
        // Merge rule (upstream WardleyBuilder.addNode):
        //   existing overridden by node, but class_name / labelOffsets
        //   preserved when node has None.
        let class_name = node
            .class_name
            .clone()
            .or_else(|| existing.class_name.clone());
        let label_offset_x = node.label_offset_x.or(existing.label_offset_x);
        let label_offset_y = node.label_offset_y.or(existing.label_offset_y);
        // Preserve already-set boolean flags (pipeline markers) across
        // re-insertion.
        let in_pipeline = node.in_pipeline || existing.in_pipeline;
        let is_pipeline_parent = node.is_pipeline_parent || existing.is_pipeline_parent;
        *existing = WardleyNode {
            class_name,
            label_offset_x,
            label_offset_y,
            in_pipeline,
            is_pipeline_parent,
            ..node
        };
    } else {
        vec.push(node);
    }
}

fn upsert_node_existing(vec: &mut Vec<WardleyNode>, node: &mut WardleyNode) {
    if let Some(existing) = vec.iter_mut().find(|n| n.id == node.id) {
        let class_name = node
            .class_name
            .clone()
            .or_else(|| existing.class_name.clone());
        let label_offset_x = node.label_offset_x.or(existing.label_offset_x);
        let label_offset_y = node.label_offset_y.or(existing.label_offset_y);
        let in_pipeline = node.in_pipeline || existing.in_pipeline;
        let is_pipeline_parent = node.is_pipeline_parent || existing.is_pipeline_parent;
        *existing = WardleyNode {
            class_name,
            label_offset_x,
            label_offset_y,
            in_pipeline,
            is_pipeline_parent,
            ..node.clone()
        };
    } else {
        vec.push(node.clone());
    }
}
