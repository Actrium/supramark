//! Sequence-diagram SVG render.
//!
//! Upstream reference:
//!   `packages/mermaid/src/diagrams/sequence/sequenceRenderer.ts`
//!   `packages/mermaid/src/diagrams/sequence/svgDraw.js`
//!
//! Byte-exact target — covers the most basic 2-actor `->>` `participant`
//! case (fixtures 78, 79). More feature-rich fixtures stay in
//! `tests/known_ignored.txt` until the full svgDraw port lands.

use crate::error::Result;
use crate::layout::sequence::SequenceLayout;
use crate::model::sequence::{
    ActorType, ArrowType, CentralConnection, DiagramItem, NotePlacement, SequenceDiagram,
};
use crate::render::svg_sequence_consts as consts;
use crate::theme::ThemeVariables;

type Theme = ThemeVariables;

/// Information collected per-actor for the render pass.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ActorRender {
    id: String,
    description: String,
    actor_type: ActorType,
    x: f64,
    width: f64,
    height: f64,
    starty: f64,
    /// 1-based actor counter as upstream's `actorCnt`. Kept for
    /// future fixtures that need self-message / activation IDs.
    cnt: usize,
    /// Popup-menu entries from `link`/`links` directives.
    links: Vec<(String, String)>,
    /// Optional custom CSS class from a `properties` directive — when
    /// `Some`, the main actor rect uses `<class> actor-{top,bottom}` and
    /// a `#EDF2AE` fill in place of the default `actor` / `#eaeaea`.
    class_name: Option<String>,
    /// Y offset for actors created mid-diagram via `create participant`.
    /// `None` for normal actors (y=0 or y=box_y); `Some(y)` for created
    /// actors whose top group starts at `y`.
    create_y: Option<f64>,
    /// Y position where the lifeline stops for `destroy`ed actors.
    /// `None` means the lifeline extends to `bottom_y`; `Some(y)` means
    /// the lifeline `<line y2>` ends at `y`.
    destroy_y: Option<f64>,
}

/// Information collected per-message for the render pass.
///
/// Multi-line messages (split on `<br>`) are rendered as ONE
/// `<text>` per line, each with its own `y`. Per upstream `drawText`
/// in `valign='center'` mode, the y of line `n` is
/// `round(starty + 10 + n * line_height_unrounded + textMargin/2)`.
#[derive(Debug, Clone)]
struct MsgRender {
    is_self: bool,
    from: String,
    to: String,
    /// One entry per `<br>`-split line.
    lines: Vec<String>,
    /// Raw message text — populated when the message carries `$$..$$`
    /// KaTeX, drives the `<foreignObject>` rendering path.
    raw_text: String,
    /// Pre-computed `min(startx, stopx)` for the KaTeX foreignObject
    /// horizontal anchor (`x = round(startx + |startx-stopx|/2 - w/2)`).
    msg_min_startx: f64,
    /// Pre-computed `|startx - stopx|` for the KaTeX foreignObject
    /// horizontal anchor.
    msg_dx_abs: f64,
    arrow: ArrowType,
    line_start_y: f64,
    text_x: f64,
    /// Y of the FIRST text line. Subsequent lines step by `line_step`.
    text_y_first: f64,
    line_step: f64,
    /// Raw starty for message text (before rounding) — upstream computes
    /// each line independently as `round(starty + 10 + n*lh + 5)`.
    starty_raw: f64,
    /// `10 + 5 = 15` offset for per-line rounding.
    y_offset: f64,
    line_x1: f64,
    line_x2: f64,
    /// For self-ref: the original startx (before autonumber shift).
    self_startx: f64,
    /// For self-ref: lineStartX for path d attribute
    /// (= startx + (autonumber && (isReverse || isBidir) ? 10 : 0)).
    self_line_start_x: f64,
    /// 0-based message index — upstream uses `i0`, `i1`, … as the
    /// `data-id` value.
    idx: usize,
    /// When `Some`, autonumber was active when this message was emitted;
    /// the value is the rendered sequence number. The renderer emits a
    /// zero-length marker line + `<text class="sequenceNumber">` after
    /// the message line, and shifts `line_x1` by `+SEQUENCE_NUMBER_RADIUS`
    /// (= 6) per upstream `sequenceRenderer.ts:646`.
    seq_index: Option<i64>,
    /// Autonumber marker / text X. Mirrors upstream's
    /// `autonumberX = isLeftToRight ? fromBounds + 1 : toBounds - 1`.
    seq_x: f64,
    /// Central-connection `()` marker — emits one or two `<circle>`
    /// elements after the `<line>`. Coordinates are the lifeline
    /// centres of source / destination actors (no autonumber offset
    /// for the no-autonumber subset).
    central_connection: Option<CentralConnection>,
    /// Cached lifeline centre X of source actor (for circle placement).
    from_cx: f64,
    /// Cached lifeline centre X of destination actor (for circle placement).
    to_cx: f64,
}

/// Per-note geometry collected during the layout pass.
#[derive(Debug, Clone)]
struct NoteRender {
    /// One entry per `<br>`-split line.
    lines: Vec<String>,
    /// Raw note text — populated when the note carries `$$..$$` KaTeX,
    /// drives the `<foreignObject>` rendering path.
    raw_text: String,
    rect_x: f64,
    rect_y: f64,
    rect_w: f64,
    rect_h: f64,
    text_x: f64,
    /// Y of the first text line. Subsequent lines step by `line_step`.
    text_y_first: f64,
    line_step: f64,
    /// Raw starty for note text (before rounding) — upstream computes
    /// each line independently as `round(starty + n*lh + margin/2)`.
    starty_raw: f64,
    /// `noteMargin / 2` for per-line rounding.
    margin_half: f64,
    /// 0-based item index — used as `data-id="iN"`.
    idx: usize,
}

/// Per-loop block geometry collected during the layout pass.
///
/// Mirrors upstream `loopModel` (sequenceRenderer.ts) for the
/// non-nested, single-section `loop … end` variant. Captured at
/// LOOP_END time after all inner messages have widened the bounds.
#[derive(Debug, Clone)]
struct LoopRender {
    /// Block left edge — `min(inner_msgs.fromBounds) - n*boxMargin`
    /// where n is the nesting depth (1 for top-level loop).
    startx: f64,
    /// Block right edge — `max(inner_msgs.toBounds) + n*boxMargin`.
    stopx: f64,
    /// Block top edge — `verticalPos` after `bumpVerticalPos(boxMargin)`
    /// fired by LOOP_START's preMargin.
    starty: f64,
    /// Block bottom edge — `last_inner_msg.stopy + n*boxMargin` per
    /// `updateBounds`.
    stopy: f64,
    /// Bracketed title text (e.g. `[Loopy]`), already wrap-formatted.
    /// Mirrors upstream `msg.message = wrapLabel(\`[${msg.message}]\`,
    /// loopWidth - 2*wrapPadding, textConf)`.
    title: String,
    /// Block keyword shown inside the labelBox — `loop`/`alt`/`opt`/etc.
    keyword: &'static str,
    /// 0-based item index assigned at LOOP_END — feeds `data-id="iN"`.
    idx: usize,
    /// Section dividers for `alt` blocks (one per `else` arm). Each
    /// entry: `(divider_y, label_text, label_y, label_idx)`. `divider_y`
    /// is the y-coordinate of the dashed `<line>` separating two arms;
    /// `label_y` is where the bracketed `[label2]` text is centred (one
    /// line below the divider). `label_idx` is the per-section item-id
    /// slot consumed at that else boundary. Empty for non-alt blocks.
    sections: Vec<LoopSection>,
}

#[derive(Debug, Clone)]
struct LoopSection {
    divider_y: f64,
    label: String,
    label_y: f64,
    label_idx: usize,
}

/// Per-rect block geometry — `rect rgb(r,g,b) ... end`. Mirrors the
/// upstream `loopModel` shape (rects share the same `bounds.newLoop`
/// machinery) but emits as a single coloured `<rect class="rect">`
/// rather than the loop's 4-line + labelBox `<g>`.
#[derive(Debug, Clone)]
struct RectRender {
    startx: f64,
    stopx: f64,
    starty: f64,
    stopy: f64,
    /// Already-formatted fill expression — `rgb(204, 0, 102)` or any
    /// CSS colour token the parser captured verbatim.
    fill: String,
}

impl RectRender {
    fn widen_x(&mut self, sx: f64, ex: f64) {
        if sx < self.startx {
            self.startx = sx;
        }
        if ex > self.stopx {
            self.stopx = ex;
        }
    }
    fn widen_stopy(&mut self, y: f64) {
        if y > self.stopy {
            self.stopy = y;
        }
    }
}

struct BoxRender {
    startx: f64,
    stopx: f64,
    starty: f64,
    stopy: f64,
    fill: String,
    label: String,
    text_x: f64,
    text_y: f64,
}

const FONT_FAMILY: &str = "\"trebuchet ms\", verdana, arial";
const ACTOR_FONT_FAMILY: &str = "\"trebuchet ms\", verdana, arial";

fn has_katex(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'$' && bytes[i + 1] == b'$' {
            return true;
        }
        i += 1;
    }
    false
}

/// Extract the first `$$..$$` body (without the `$$` delimiters). Returns
/// an empty string when no closed pair is found.
fn extract_first_katex_body(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut start: Option<usize> = None;
    while i + 1 < bytes.len() {
        if bytes[i] == b'$' && bytes[i + 1] == b'$' {
            match start {
                None => {
                    start = Some(i + 2);
                    i += 2;
                }
                Some(s) => {
                    if i > s {
                        return text[s..i].to_string();
                    }
                    i += 2;
                }
            }
        } else {
            i += 1;
        }
    }
    String::new()
}

pub fn render(
    d: &SequenceDiagram,
    _l: &SequenceLayout,
    theme: &Theme,
    id: &str,
) -> Result<String> {
    // ── Eligibility gate ────────────────────────────────────────────
    //
    // Byte-exact path covers two visual archetypes: `Participant`
    // (rectangle box) and `Actor` (UML stick-figure via `actor` keyword).
    // Other archetypes — boundary/control/entity/database/collections/
    // queue — drop to placeholder. Items must all be solid- or dotted-
    // arrow messages with no `box`, `create`, `destroy` features.
    if !d
        .actors
        .iter()
        .all(|a| matches!(
            a.actor_type,
            ActorType::Participant
                | ActorType::Actor
                | ActorType::Boundary
                | ActorType::Control
                | ActorType::Entity
                | ActorType::Database
                | ActorType::Queue
                | ActorType::Collections
        ))
    {
        return Ok(placeholder(d, id));
    }
    // Reject any item we can't render byte-exactly.
    // Currently supported:
    //   - Message: SolidArrow (`->>`), DottedArrow (`-->>`),
    //     SolidLine (`->`), DottedLine (`-->`). The OPEN variants get
    //     no `marker-end`; the DOTTED variants additionally get the
    //     `messageLine1` dashed style. Mirrors upstream `drawArrow`.
    //   - Note: single-line, single-actor anchor (no `over a, b`),
    //     no wrap. Matches upstream `drawNote` for the simplest path.
    fn only_supported_items(items: &[DiagramItem]) -> bool {
        items.iter().all(|it| match it {
            DiagramItem::Message(m) => {
                // Activation lifecycle (`+`/`-` suffix on messages) is
                // handled below: `+` opens an activation on `to`, `-`
                // closes the most-recent activation on `from`. Central-
                // connection auto-activate (`AtTo`/`Dual`) also lifts
                // `activate=true`; that is fine and NEVER draws a rect on
                // its own — only an explicit `deactivate` / `-` does.
                matches!(
                    m.arrow,
                    Some(ArrowType::SolidArrow)
                        | Some(ArrowType::DottedArrow)
                        | Some(ArrowType::SolidLine)
                        | Some(ArrowType::DottedLine)
                        | Some(ArrowType::SolidCross)
                        | Some(ArrowType::DottedCross)
                        | Some(ArrowType::SolidPoint)
                        | Some(ArrowType::DottedPoint)
                        | Some(ArrowType::BiSolid)
                        | Some(ArrowType::BiDotted)
                        | Some(ArrowType::SolidTop)
                        | Some(ArrowType::SolidBottom)
                        | Some(ArrowType::StickTop)
                        | Some(ArrowType::StickBottom)
                        | Some(ArrowType::SolidTopDotted)
                        | Some(ArrowType::SolidBottomDotted)
                        | Some(ArrowType::StickTopDotted)
                        | Some(ArrowType::StickBottomDotted)
                        | Some(ArrowType::SolidTopReverse)
                        | Some(ArrowType::SolidBottomReverse)
                        | Some(ArrowType::StickTopReverse)
                        | Some(ArrowType::StickBottomReverse)
                        | Some(ArrowType::SolidTopReverseDotted)
                        | Some(ArrowType::SolidBottomReverseDotted)
                        | Some(ArrowType::StickTopReverseDotted)
                        | Some(ArrowType::StickBottomReverseDotted)
                )
            }
            DiagramItem::Note(n) => {
                // Single-actor placements (left of / right of / over)
                // and the 2-actor Over span (`Note over A,B`) — where
                // upstream `buildNoteModel` `else` branch handles the
                // cross-actor span as forceWidth = |fromCenter -
                // toCenter| + actorMargin.
                let actors_ok = match n.placement_actors.len() {
                    1 => true,
                    2 => matches!(n.placement, Some(NotePlacement::Over)),
                    _ => false,
                };
                actors_ok && n.placement.is_some() && !n.text.contains('\n')
            }
            // Autonumber occupies an item-id slot and toggles per-message
            // sequence-number rendering — supported below.
            DiagramItem::Autonumber { .. } => true,
            // `loop <label> ... end` / `opt <label> ... end` /
            // `break <label> ... end` — single-section variants. Inner
            // items can themselves be Message / Note / Autonumber /
            // nested Loop / Opt / Alt / Par / Critical / Break
            // (recursive support check). Mirrors upstream's
            // `LOOP_START`/`LOOP_END` (and `OPT_START`/`OPT_END`,
            // `ALT_START`/`ALT_END`, `BREAK_START`/`BREAK_END`,
            // `PAR_START`/`PAR_END`, `CRITICAL_START`/`CRITICAL_END`)
            // event pairs around drawLoop.
            DiagramItem::Loop { items, .. }
            | DiagramItem::Opt { items, .. }
            | DiagramItem::Break { items, .. }
            | DiagramItem::Rect { items, .. } => only_supported_items(items),
            // `alt <label> ... else <label2> ... end` — multi-section
            // variant. Each branch may contain Message / Note /
            // Autonumber / nested Loop / Opt / Alt.
            DiagramItem::Alt { branches } | DiagramItem::Critical { branches } => {
                branches.iter().all(|b| only_supported_items(&b.items))
            }
            // `par <label> ... and <label2> ... end` — multi-section
            // parallel variant. ParBranch shape mirrors AltBranch.
            DiagramItem::Par { branches } => {
                branches.iter().all(|b| only_supported_items(&b.items))
            }
            // `activate <actor>` / `deactivate <actor>` — explicit
            // lifeline activation start/end. `Activate` opens an
            // activation slot for the actor (no rect emitted on its own);
            // `Deactivate` pops the most-recent open activation and emits
            // a `<rect class="activationN">` rectangle. Mirrors upstream
            // sequenceRenderer.ts:1145-1156 (ACTIVE_START / ACTIVE_END).
            DiagramItem::Activate(_) | DiagramItem::Deactivate(_) => true,
            DiagramItem::Create(_) | DiagramItem::Destroy(_) => true,
            _ => false,
        })
    }
    if !only_supported_items(&d.items) {
        return Ok(placeholder(d, id));
    }
    
    // Need at least one actor. Empty items list is valid — just renders
    // the actor box(es) without any messages.
    if d.actors.is_empty() {
        return Ok(placeholder(d, id));
    }

    // ── Layout (mirrors upstream addActorRenderingData + boundMessage)
    let cfg = &d.config;
    let default_actor_w = cfg.width;
    let actor_h = cfg.height;
    let actor_margin = cfg.actor_margin;
    let box_margin = cfg.box_margin;
    let bottom_margin_adj = cfg.bottom_margin_adj;
    let dia_margin_x = cfg.diagram_margin_x;
    let dia_margin_y = cfg.diagram_margin_y;

    // Per-actor (width, height, rendered_description) — upstream
    // `calculateActorMargins` first loop:
    //   if actor.wrap: description = wrapLabel(desc, conf.width-2*wrapPadding)
    //   actor.width  = wrap ? conf.width
    //                       : max(conf.width, textWidth + 2*wrapPadding)
    //   actor.height = wrap ? max(actDims.height, conf.height) : conf.height
    // Actor description is measured with the actor font (effective size
    // = global fontSize=16 after `setConf` override, family
    // `"trebuchet ms", verdana, arial`). After this loop upstream
    // assigns `conf.height = max(all actor.height)`, then
    // `addActorRenderingData` clamps every actor.height up to that.
    // Net effect: all actors share the SAME height = maxHeight.
    //
    // `actor.wrap` is set per-actor by the parser when the description
    // carries a `wrap:` prefix OR when `%%{init: ... wrap: true}%%` was
    // declared (we propagate config.wrap below before this loop).
    let actor_dims: Vec<(f64, f64, String)> = d
        .actors
        .iter()
        .map(|a| {
            // Effective wrap = per-actor flag OR diagram-level config.
            let wrap = a.wrap || cfg.wrap;
            // When wrap is on AND the description has no <br> already,
            // pre-wrap to (conf.width - 2*wrapPadding) before measuring.
            let description = if wrap {
                wrap_label(
                    &a.description,
                    cfg.width - 2.0 * cfg.wrap_padding,
                    "\"trebuchet ms\", verdana, arial",
                    16.0,
                )
            } else {
                a.description.clone()
            };
            // Multi-line descriptions (split on <br>) measure as the
            // max line width, mirroring upstream
            // `calculateTextDimensions` over `splitBreaks`. Per-line
            // height is rounded individually then summed for actDims.
            let lines = split_br(&description);
            let mut tw_max = 0.0_f64;
            if !has_katex(&description) {
                for line in &lines {
                    let resolved = resolve_hash_entities_for_measure(line);
                    let w = crate::font_metrics::text_width(
                        &resolved,
                        "\"trebuchet ms\", verdana, arial",
                        16.0,
                        false,
                        false,
                    )
                    .round();
                    if w > tw_max {
                        tw_max = w;
                    }
                }
            }
            let line_h = crate::font_metrics::line_height(
                "\"trebuchet ms\", verdana, arial",
                16.0,
                false,
                false,
            )
            .round();
            let actdims_h = if has_katex(&description) {
                line_h
            } else {
                line_h * (lines.len() as f64)
            };
            let width = if wrap || has_katex(&description) {
                default_actor_w
            } else {
                let candidate = tw_max + 2.0 * cfg.wrap_padding;
                default_actor_w.max(candidate)
            };
            let height = if wrap {
                actdims_h.max(actor_h)
            } else {
                actor_h
            };
            (width, height, description)
        })
        .collect();
    let actor_widths: Vec<f64> = actor_dims.iter().map(|(w, _, _)| *w).collect();
    // Upstream `calculateActorMargins` returns maxHeight which becomes
    // the new `conf.height`. Then `addActorRenderingData` bumps every
    // actor.height up to conf.height. So every actor renders at the
    // SAME height (the max across all actors).
    let actor_h = actor_dims
        .iter()
        .map(|(_, h, _)| *h)
        .fold(actor_h, f64::max);

    // ── Per-actor max message width (mirrors getMaxMessageWidthPerActor)
    //
    // For each Alice→Bob message where Alice.nextActor == Bob, the
    // FROM actor's max-msg-width is updated. The width is text-width +
    // 2 * wrap_padding. We then translate that to per-actor margins via
    // `calculateActorMargins`, and finally the actor's X coordinate is
    // the running (width + margin) sum.
    let n_actors = d.actors.len();
    let actor_id_to_index: std::collections::HashMap<&str, usize> = d
        .actors
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id.as_str(), i))
        .collect();
    let prev_actor_of: Vec<Option<usize>> = (0..n_actors)
        .map(|i| if i == 0 { None } else { Some(i - 1) })
        .collect();
    let next_actor_of: Vec<Option<usize>> = (0..n_actors)
        .map(|i| if i + 1 == n_actors { None } else { Some(i + 1) })
        .collect();

    let mut max_msg_width_per_actor: Vec<f64> = vec![0.0; n_actors];
    // Upstream `getMaxMessageWidthPerActor` walks the FLAT message list
    // returned by `diagObj.db.getMessages()` — that list already includes
    // every message and note inside loop/alt/opt/par/critical/break/rect
    // blocks (the parser appends them all to a single flat array, with
    // LOOP_START/END/etc. markers interleaved). Our AST keeps blocks as
    // nested children, so we need to walk recursively to mirror the
    // flat-iteration behaviour. Without this, messages inside a `loop`
    // or `alt` contribute nothing to inter-actor margins.
    fn collect_items<'a>(items: &'a [DiagramItem], out: &mut Vec<&'a DiagramItem>) {
        for it in items {
            match it {
                DiagramItem::Loop { items, .. }
                | DiagramItem::Opt { items, .. }
                | DiagramItem::Break { items, .. }
                | DiagramItem::Rect { items, .. } => {
                    out.push(it);
                    collect_items(items, out);
                }
                DiagramItem::Alt { branches } | DiagramItem::Critical { branches } => {
                    out.push(it);
                    for b in branches {
                        collect_items(&b.items, out);
                    }
                }
                DiagramItem::Par { branches } => {
                    out.push(it);
                    for b in branches {
                        collect_items(&b.items, out);
                    }
                }
                _ => out.push(it),
            }
        }
    }
    let mut flat_items: Vec<&DiagramItem> = Vec::new();
    collect_items(&d.items, &mut flat_items);
    for it in flat_items {
        match it {
            DiagramItem::Message(m) => {
                let (Some(&from_i), Some(&to_i)) = (
                    actor_id_to_index.get(m.from.as_str()),
                    actor_id_to_index.get(m.to.as_str()),
                ) else {
                    continue;
                };
                // Wrap-aware width: when `wrap:` set, pre-wrap the text
                // before measuring. Take max line width (split on
                // `<br>`). Effective wrap = per-message flag OR
                // diagram-level config (`%%{init:{config:{wrap:true}}}%%`).
                let wrap = m.wrap || cfg.wrap;
                let measured = if wrap {
                    wrap_label(
                        &m.text,
                        cfg.width - 2.0 * cfg.wrap_padding,
                        "sans-serif",
                        cfg.message_font_size as f64,
                    )
                } else {
                    m.text.clone()
                };
                let lines = split_br(&measured);
                let mut msg_text_width = 0.0_f64;
                if !has_katex(&measured) {
                    for line in &lines {
                        let resolved = resolve_hash_entities_for_measure(line);
                        let w = crate::font_metrics::text_width(
                            &resolved,
                            "sans-serif",
                            cfg.message_font_size as f64,
                            false,
                            false,
                        )
                        .round();
                        if w > msg_text_width {
                            msg_text_width = w;
                        }
                    }
                }
                let message_width = msg_text_width + 2.0 * cfg.wrap_padding;

                if from_i == to_i {
                    // self-message — both halves
                    let half = message_width / 2.0;
                    if max_msg_width_per_actor[from_i] < half {
                        max_msg_width_per_actor[from_i] = half;
                    }
                    // upstream also bumps prevActor when it exists (mirrors
                    // the `actor.prevActor` branch in
                    // `getMaxMessageWidthPerActor`). Skipped here for the
                    // non-self path until needed by a future fixture.
                } else if next_actor_of[to_i] == Some(from_i) {
                    // arrow points right→left: from is to.next, so to.next ==
                    // from. Update toActor's max-msg-width.
                    if max_msg_width_per_actor[to_i] < message_width {
                        max_msg_width_per_actor[to_i] = message_width;
                    }
                } else if prev_actor_of[to_i] == Some(from_i) {
                    // arrow points left→right: from is to.prev. Update from's
                    // max-msg-width.
                    if max_msg_width_per_actor[from_i] < message_width {
                        max_msg_width_per_actor[from_i] = message_width;
                    }
                }
                // (cross-actor messages with non-adjacent endpoints are not
                // covered by this minimal port — placeholder fallback handles
                // those fixtures.)
            }
            DiagramItem::Note(note) => {
                // Notes contribute to per-actor margins too — see
                // upstream `getMaxMessageWidthPerActor` for the
                // placement-specific rules.
                let placement = match note.placement {
                    Some(p) => p,
                    None => continue,
                };
                // Single-actor placements operate on `actor_i`. The
                // 2-actor Over span is handled in a dedicated branch
                // below — upstream `getMaxMessageWidthPerActor` for
                // OVER uses `actor = actors.get(msg.to)` and contributes
                // `messageWidth/2` to `actor.prevActor` (= msg.from)
                // and to `msg.from` again — both reduce to msg.from
                // for the 2-actor case, so a single contribution
                // suffices.
                if note.placement_actors.len() == 2
                    && matches!(placement, NotePlacement::Over)
                {
                    let from_id = &note.placement_actors[0];
                    let to_id = &note.placement_actors[1];
                    let (Some(&from_i), Some(&to_i)) = (
                        actor_id_to_index.get(from_id.as_str()),
                        actor_id_to_index.get(to_id.as_str()),
                    ) else {
                        continue;
                    };
                    // Wrap-aware text width.
                    let measured_text = if note.wrap {
                        wrap_label(
                            &note.text,
                            cfg.width - 2.0 * cfg.wrap_padding,
                            "trebuchet ms",
                            cfg.message_font_size as f64,
                        )
                    } else {
                        note.text.clone()
                    };
                    let lines = split_br(&measured_text);
                    let mut text_w = 0.0_f64;
                    if !has_katex(&measured_text) {
                        for line in &lines {
                            let resolved = resolve_hash_entities_for_measure(line);
                            let w = crate::font_metrics::text_width(
                                &resolved,
                                "trebuchet ms",
                                cfg.message_font_size as f64,
                                false,
                                false,
                            )
                            .round();
                            if w > text_w {
                                text_w = w;
                            }
                        }
                    }
                    let message_width = text_w + 2.0 * cfg.wrap_padding;
                    let half = message_width / 2.0;
                    // Upstream contributes to BOTH msg.from AND
                    // msg.to.prevActor. For adjacent A,B these reduce
                    // to A. For non-adjacent A,D (skipping B,C),
                    // contributes to A AND C — ensuring both A→B and
                    // C→D gaps can hold half the note width.
                    if max_msg_width_per_actor[from_i] < half {
                        max_msg_width_per_actor[from_i] = half;
                    }
                    let to_prev = if to_i > from_i {
                        to_i.checked_sub(1)
                    } else if to_i < from_i {
                        Some(to_i + 1)
                    } else {
                        None
                    };
                    if let Some(p) = to_prev {
                        if max_msg_width_per_actor[p] < half {
                            max_msg_width_per_actor[p] = half;
                        }
                    }
                    continue;
                }
                if note.placement_actors.len() != 1 {
                    continue;
                }
                let actor_id = &note.placement_actors[0];
                let Some(&actor_i) = actor_id_to_index.get(actor_id.as_str()) else {
                    continue;
                };
                let prev_i = prev_actor_of[actor_i];
                let next_i = next_actor_of[actor_i];
                // upstream: skip if the placement is past the end of
                // the actor list (e.g. left-of the leftmost or
                // right-of the rightmost actor).
                if matches!(placement, NotePlacement::LeftOf) && prev_i.is_none() {
                    continue;
                }
                if matches!(placement, NotePlacement::RightOf) && next_i.is_none() {
                    continue;
                }
                // Wrap-aware text width: when `wrap:` is set, upstream
                // first wraps to (conf.width - 2*wrapPadding) before
                // measuring. Take max line width across the wrapped
                // (or br-split) text.
                let measured_text = if note.wrap {
                    wrap_label(
                        &note.text,
                        cfg.width - 2.0 * cfg.wrap_padding,
                        "trebuchet ms",
                        cfg.message_font_size as f64,
                    )
                } else {
                    note.text.clone()
                };
                let lines = split_br(&measured_text);
                let mut text_w = 0.0_f64;
                if !has_katex(&measured_text) {
                    for line in &lines {
                        let resolved = resolve_hash_entities_for_measure(line);
                        let w = crate::font_metrics::text_width(
                            &resolved,
                            "trebuchet ms",
                            cfg.message_font_size as f64,
                            false,
                            false,
                        )
                        .round();
                        if w > text_w {
                            text_w = w;
                        }
                    }
                }
                let message_width = text_w + 2.0 * cfg.wrap_padding;
                match placement {
                    NotePlacement::RightOf => {
                        if max_msg_width_per_actor[actor_i] < message_width {
                            max_msg_width_per_actor[actor_i] = message_width;
                        }
                    }
                    NotePlacement::LeftOf => {
                        if let Some(p) = prev_i {
                            if max_msg_width_per_actor[p] < message_width {
                                max_msg_width_per_actor[p] = message_width;
                            }
                        }
                    }
                    NotePlacement::Over => {
                        let half = message_width / 2.0;
                        if let Some(p) = prev_i {
                            if max_msg_width_per_actor[p] < half {
                                max_msg_width_per_actor[p] = half;
                            }
                        }
                        if next_i.is_some() && max_msg_width_per_actor[actor_i] < half {
                            max_msg_width_per_actor[actor_i] = half;
                        }
                    }
                }
            }
            _ => continue,
        }
    }

    // ── Per-actor margin (mirrors calculateActorMargins second loop)
    //
    // For each actor with a nextActor: actor.margin = max(messageWidth
    // + actorMargin - actor.width/2 - nextActor.width/2, actorMargin).
    // For the trailing actor: actor.margin = max(messageWidth +
    // actorMargin - actor.width/2, actorMargin).
    let mut actor_margins: Vec<f64> = vec![actor_margin; n_actors];
    for i in 0..n_actors {
        let mw = max_msg_width_per_actor[i];
        if mw == 0.0 {
            continue;
        }
        let half_self = actor_widths[i] / 2.0;
        let m = if let Some(n) = next_actor_of[i] {
            mw + actor_margin - half_self - actor_widths[n] / 2.0
        } else {
            mw + actor_margin - half_self
        };
        actor_margins[i] = m.max(actor_margin);
    }

    // Per-actor margin
    // X positions: mirrors upstream addActorRenderingData actor loop.
    // When boxes are present, box margins shift actor positions:
    //   - on box start: prevMargin += box.margin
    //   - on box end: prevMargin += boxMargin + prevBox.margin
    //   - actor.x = prevWidth + prevMargin
    //   - prevWidth += actor.width + prevMargin
    //   - prevMargin = actor.margin (overwritten)
    //   - if actor in box: box.width = prevWidth + box.margin - box.x
    let mut xs: Vec<f64> = Vec::with_capacity(n_actors);
    // Pre-compute per-box margins (upstream calculateActorMargins box loop).
    let mut box_margins_for_x: Vec<f64> = vec![cfg.box_text_margin; d.boxes.len()];
    {
        let text_font_size = cfg.message_font_size as f64;
        let actor_id_to_idx_local: std::collections::HashMap<&str, usize> = d
            .actors
            .iter()
            .enumerate()
            .map(|(i, a)| (a.id.as_str(), i))
            .collect();
        for (bi, bx) in d.boxes.iter().enumerate() {
            let mut total_width = 0.0_f64;
            for actor_id in &bx.actors {
                if let Some(&idx) = actor_id_to_idx_local.get(actor_id.as_str()) {
                    // Mirror upstream `(actor.margin || 0)`: actor.margin is
                    // only set in calculateActorMargins for actors that appear
                    // in `actorToMessageWidth` (i.e. those that participate
                    // in adjacent / self / placement-bearing messages).
                    // Actors with no contributing message stay undefined and
                    // contribute 0 to box totalWidth (NOT the default
                    // actorMargin). Required for fixtures with non-adjacent
                    // messages like demos/sequence/05.
                    let margin = if max_msg_width_per_actor[idx] == 0.0 {
                        0.0
                    } else {
                        actor_margins[idx]
                    };
                    total_width += actor_widths[idx] + margin;
                }
            }
            let standard_box_padding = box_margin * 8.0;
            total_width += standard_box_padding;
            total_width -= 2.0 * cfg.box_text_margin;
            let label = bx.label.as_str();
            let label_w = if has_katex(label) {
                0.0
            } else {
                crate::font_metrics::text_width(
                    label,
                    "sans-serif",
                    text_font_size,
                    false,
                    false,
                )
                .round()
            };
            let min_width = total_width.max(label_w + 2.0 * cfg.wrap_padding);
            let mut bx_margin = cfg.box_text_margin;
            if total_width < min_width {
                bx_margin += (min_width - total_width) / 2.0;
            }
            box_margins_for_x[bi] = bx_margin;
        }
    }
    {
        let mut cursor = 0.0_f64;
        let mut prev_margin = 0.0_f64;
        let mut prev_box_idx: Option<usize> = None;
        for i in 0..n_actors {
            let cur_box = d.actors[i].box_index;
            // End of previous box.
            if prev_box_idx != cur_box {
                if let Some(pbi) = prev_box_idx {
                    prev_margin += box_margin + box_margins_for_x[pbi];
                }
                // Start of new box.
                if let Some(cbi) = cur_box {
                    prev_margin += box_margins_for_x[cbi];
                }
            }
            if d.actors[i].created {
                prev_margin += actor_widths[i] / 2.0;
            }
            xs.push(cursor + prev_margin);
            cursor += actor_widths[i] + prev_margin;
            prev_margin = actor_margins[i];
            prev_box_idx = cur_box;
        }
    }
    let mut actors: Vec<ActorRender> = d
        .actors
        .iter()
        .enumerate()
        .map(|(i, a)| ActorRender {
            id: a.id.clone(),
            // Use the (possibly wrap-expanded) description from the
            // dims pass — wrap_label inserts `<br/>` so split_br on the
            // render side will produce the same multi-line layout.
            description: actor_dims[i].2.clone(),
            actor_type: a.actor_type.clone(),
            x: xs[i],
            width: actor_widths[i],
            height: actor_h,
            starty: if !d.boxes.is_empty() {
                let tf_size = cfg.message_font_size as f64;
                let max_bth = d.boxes.iter().map(|bx| {
                    crate::font_metrics::line_height("sans-serif", tf_size, false, false).round()
                }).fold(0.0_f64, f64::max);
                box_margin + if d.boxes.iter().any(|b| !b.label.is_empty()) { max_bth } else { 0.0 }
            } else { 0.0 },
            cnt: i + 1,
            links: a.links.clone(),
            class_name: a.class_name.clone(),
            create_y: if a.created { Some(0.0) } else { None },
            destroy_y: if a.destroyed { Some(0.0) } else { None },
        })
        .collect();

    // ── Box geometry (upstream calculateActorMargins + drawActors) ───
    //
    // For each box, compute x/width/margin/textMaxHeight from the actors
    // it contains. Mirrors upstream's `calculateActorMargins` box loop:
    //   totalWidth = sum(actor.width + actor.margin for actors in box)
    //              + 8*boxMargin - 2*boxTextMargin
    //   boxMsgDimensions = calculateTextDimensions(box.name, textFont)
    //   minWidth = max(totalWidth, boxMsgDimensions.width + 2*wrapPadding)
    //   box.margin = boxTextMargin + max(0, (minWidth - totalWidth) / 2)
    //   box.textMaxHeight = max across all boxes of boxMsgDimensions.height
    //
    // Then in addActorRenderingData:
    //   box.x = prevWidth + prevMargin  (at first actor in box)
    //   box.width = prevWidth_at_last + box.margin - box.x
    //
    // After drawActors loop:
    //   box.height = verticalPos - box.y
    //   boxPadding = boxMargin * 2
    //   box.startx = box.x - boxPadding
    //   box.starty = box.y - boxPadding * 0.25
    //   box.stopx = box.startx + box.width + 2 * boxPadding
    //   box.stopy = box.starty + box.height + boxPadding * 0.75
    //
    // drawBox renders: rect(x=startx, y=starty, width=stopx-startx,
    //   height=stopy-starty, fill=box.fill||'transparent',
    //   stroke='rgb(0,0,0, 0.5)', class='rect')
    //   text(x=box.x+box.width/2,
    //        y=box.y+boxTextMargin+textMaxHeight/2,
    //        class='text')
    let mut box_renders: Vec<BoxRender> = Vec::new();
    let has_boxes = !d.boxes.is_empty();
    // Compute max_box_text_height (needed for hasBoxes vertical bump).
    let max_box_text_height = if has_boxes {
        let text_font_size = cfg.message_font_size as f64;
        let mut h = 0.0_f64;
        for bx in &d.boxes {
            let label_h =
                crate::font_metrics::line_height("sans-serif", text_font_size, false, false)
                    .round();
            if label_h > h {
                h = label_h;
            }
        }
        h
    } else {
        0.0
    };

    // ── hasBoxes vertical bump (upstream sequenceRenderer.ts:1070-1075) ─
    //
    // When boxes are present, upstream bumps verticalPos by boxMargin and
    // then by textMaxHeight (the max box label text height). This shifts
    // the actor top rects and the box.y origin down.
    let has_box_titles = has_boxes && d.boxes.iter().any(|b| !b.label.is_empty());
    let box_y = if has_boxes {
        box_margin + if has_box_titles { max_box_text_height } else { 0.0 }
    } else {
        0.0
    };

    if has_boxes {

        // Compute box.x and box.width from actor positions.
        // Upstream: box.x = prevWidth + prevMargin when first actor in box
        // is encountered. box.width = prevWidth_at_last_actor + box.margin - box.x.
        // We replicate by walking actors in order and tracking the running
        // prevWidth (sum of widths + margins so far) and prevMargin.
        let mut prev_width = 0.0_f64;
        let mut prev_margin = 0.0_f64;
        let mut prev_box_idx: Option<usize> = None;
        // We'll store (x, width, margin, label, fill) for each box.
        let mut box_geom: Vec<(f64, f64, f64, String, String)> =
            vec![(0.0, 0.0, 0.0, String::new(), String::new()); d.boxes.len()];
        // Track which boxes have had their x set.
        let mut box_x_set = vec![false; d.boxes.len()];

        for (i, a) in d.actors.iter().enumerate() {
            let cur_box = a.box_index;
            // End of previous box.
            if prev_box_idx != cur_box {
                if let Some(pbi) = prev_box_idx {
                    prev_margin += box_margin + box_margins_for_x[pbi];
                }
                // Start of new box.
                if let Some(cbi) = cur_box {
                    if !box_x_set[cbi] {
                        box_geom[cbi].0 = prev_width + prev_margin;
                        box_x_set[cbi] = true;
                    }
                    prev_margin += box_margins_for_x[cbi];
                }
            }
            prev_width += actor_widths[i] + prev_margin;
            if let Some(cbi) = cur_box {
                box_geom[cbi].1 = prev_width + box_margins_for_x[cbi] - box_geom[cbi].0;
                box_geom[cbi].2 = box_margins_for_x[cbi];
                box_geom[cbi].3 = d.boxes[cbi].label.clone();
                box_geom[cbi].4 = d.boxes[cbi]
                    .fill
                    .clone()
                    .unwrap_or_else(|| "transparent".to_string());
            }
            prev_margin = actor_margins[i];
            prev_box_idx = cur_box;
        }

        // box.y = 0 in upstream (addActorRenderingData receives
        // verticalPos=0). The hasBoxes bump only affects
        // actor.starty, not box.y.
        let bx_y = 0.0_f64;
        // box.height will be set later after vertical pass.
        // We store partial geometry and finalize after the vertical pass.
        for (bi, (bx_x, bx_w, _bx_margin, label, fill)) in
            box_geom.into_iter().enumerate()
        {
            let box_padding = box_margin * 2.0;
            let startx = bx_x - box_padding;
            let starty = bx_y - box_padding * 0.25;
            let stopx = startx + bx_w + 2.0 * box_padding;
            // stopy will be set later — use placeholder.
            let text_x = bx_x + bx_w / 2.0;
            let text_y = bx_y + cfg.box_text_margin + max_box_text_height / 2.0;
            box_renders.push(BoxRender {
                startx,
                starty,
                stopx,
                stopy: 0.0,
                fill,
                label,
                text_x,
                text_y,
            });
        }
    }

    // Vertical pass: emulate boundMessage on each message.
    // Initial: vertical = box_y + actor_h (box_y is 0 for no-box diagrams,
    // or boxMargin + textMaxHeight for box diagrams).
    let mut vertical = box_y + actor_h;
    let line_height = compute_message_line_height(cfg.message_font_size as f64);

    let mut messages: Vec<MsgRender> = Vec::new();
    let mut notes: Vec<NoteRender> = Vec::new();
    // Loop blocks — each entry is one `<g data-et="control-structure">`.
    // Populated at LOOP_END time so startx/stopx/stopy reflect the union
    // of inner-message bounds. Emitted (in source order) BEFORE notes
    // and messages, mirroring upstream's drawLoop-on-LOOP_END flow.
    let mut loops: Vec<LoopRender> = Vec::new();
    // Stack of indices into `loops` for currently-open Loop blocks.
    // Inner messages widen `loops[i].startx/stopx` for every active
    // entry on this stack — mirrors upstream `updateBounds` per
    // `sequenceItems` element.
    let mut active_loops: Vec<usize> = Vec::new();
    // Rect-fill blocks — `rect rgb(...) ... end`. Backgrounds are
    // pushed at RECT_END time and emitted at the very start of the
    // body in REVERSE end-order to match upstream's
    // `forEach(drawBackgroundRect)` + per-rect `.lower()` (each
    // `lower()` reasserts as first child, so the LAST end is first
    // in DOM, the second-to-last is second, etc.).
    // `rects` is filled at RECT_END time so it carries END order
    // (innermost finishes first; sequential rects in source order).
    let mut rects: Vec<RectRender> = Vec::new();
    // Stack of currently-open rect models — partial geometry that
    // accumulates startx/stopx as inner messages widen the bounds.
    // Drained into `rects` (in end order) on each RECT_END.
    let mut pending_rects: Vec<RectRender> = Vec::new();
    // Unified open-block stack — mirrors upstream's `sequenceItems`
    // (one push per LOOP/ALT/OPT/PAR/CRITICAL/BREAK/RECT start, one
    // pop per matching end). At every `bounds.insert` we widen each
    // open block by ±n*boxMargin where n = stack.len() - position
    // (1-based from top) — sequenceRenderer.ts:114-128 updateBounds.
    // The two `kind` arms route the widening to the right backing
    // store: `Loop(idx)` → loops[idx], `Rect` → top of pending_rects.
    #[derive(Debug, Clone, Copy)]
    enum SeqItem {
        Loop(usize),
        Rect(usize), // index into pending_rects
    }
    let mut seq_items: Vec<SeqItem> = Vec::new();
    // ── Activation tracking ─────────────────────────────────────────
    //
    // Mirrors upstream `bounds.activations` (sequenceRenderer.ts:148-169
    // newActivation/endActivation + 1145-1156 ACTIVE_START / ACTIVE_END
    // dispatch). Each open activation has a startx (lifeline centre +
    // half-width offset for stacking), starty (verticalPos+2 at push
    // time), and the actor id. On close (Deactivate / `-` suffix /
    // ACTIVE_END), `lastIndexOf actor` pops the most-recent slot and
    // emits one `<rect class="activationN">` rectangle.
    const ACTIVATION_WIDTH: f64 = 10.0;
    #[derive(Debug, Clone)]
    struct ActivationSlot {
        startx: f64,
        starty: f64,
        actor: String,
        /// Index into `activation_anchors` — the empty `<g>` slot
        /// reserved at newActivation time. activeEnd writes a `<rect>`
        /// into this slot.
        anchor_idx: usize,
    }
    #[derive(Debug, Clone)]
    struct ActivationRect {
        x: f64,
        y: f64,
        height: f64,
        /// `class="activationN"` where N = totalActivationsSeen % 3 per
        /// upstream `actorActivations(msg.from).length`.
        class_n: u32,
    }
    let mut activations: Vec<ActivationSlot> = Vec::new();
    // Render-order list of activation anchor groups. Each newActivation
    // appends a placeholder; each endActivation populates the matching
    // entry with a rect. Mirrors upstream `svgDraw.anchorElement(diagram)`
    // which appends `elem.append('g')` for every push, regardless of
    // whether it ever gets a rect.
    //
    // The first tuple field is the item-id slot the anchor was assigned
    // to (the synthetic CC / CCR / activeStart event between two
    // addMessage signals — see jison.331-355). Used to interleave
    // anchors with notes/loops in render order so cross-actor-CC
    // messages emit their `<g></g>` placeholders ADJACENT to the
    // surrounding signals (not all batched up-front).
    let mut activation_anchors: Vec<(usize, Option<ActivationRect>)> = Vec::new();
    // ── Layout-pass activations (ACTIVE_START only) ─────────────────
    //
    // Upstream's layout pass (sequenceRenderer.ts:2030 loop) only tracks
    // ACTIVE_START / ACTIVE_END in `bounds.activations` — CC events fall
    // through to `buildMessageModel()` which returns `{}` for non-arrow
    // types (1837-1874). `activationBounds()` is consulted by
    // buildMessageModel @ 1876-1877 to compute the message's
    // [fromLeft, fromRight, toLeft, toRight], so an open activation on
    // the from / to actor shifts the start / stop x of every subsequent
    // message until ACTIVE_END.
    //
    // Activations from `+` suffix (`actor signal '+' actor text`)
    // appear AFTER the addMessage signal in the parser's emission list
    // (jison.333-334), so the current message's startx/stopx still use
    // the PRE-push bounds. Same for `-`: ACTIVE_END comes AFTER the
    // addMessage, so the current message uses PRE-pop bounds.
    #[derive(Debug, Clone)]
    struct LayoutActivation {
        startx: f64,
        stopx: f64,
        actor: String,
    }
    let mut layout_activations: Vec<LayoutActivation> = Vec::new();
    // Autonumber state — mirrors upstream's `sequenceIndex`,
    // `sequenceIndexStep`, `db.showSequenceNumbers()` running tally.
    // Each `Autonumber` item updates these in declaration order; the
    // current values are stamped onto every subsequent message.
    let mut auto_seq_index: i64 = 1;
    let mut auto_seq_step: i64 = 1;
    let mut auto_visible: bool = false;
    // Track min/max x extents so we can compute the SVG viewBox after
    // all notes are placed. Notes can extend BEYOND the actor lifelines
    // when placed `left of` / `right of` the leftmost / rightmost actor.
    let mut bounds_startx: f64 = 0.0;
    let mut bounds_stopx: f64 = actors.last().map(|a| a.x + a.width).unwrap_or(0.0);
    // Upstream calls bounds.insert(box.x, box.y, box.x+box.width, box.height)
    // for each box directive, which can expand bounds.stopx beyond the last
    // actor's right edge. The inner box stopx = br.stopx - box_padding.
    let box_padding = box_margin * 2.0;
    for br in &box_renders {
        let inner_stopx = br.stopx - box_padding;
        if inner_stopx > bounds_stopx {
            bounds_stopx = inner_stopx;
        }
    }
    // `msg_id_counter` mirrors upstream's `messages.length` at the time
    // each event is pushed — both real messages AND synthetic
    // `centralConnection` / `centralConnectionReverse` signals consume
    // one slot. So a message with `Dual` is followed by +2 synthetic
    // events, advancing the counter by 3 total per Dual addMessage.
    let mut msg_id_counter: usize = 0;

    // Flatten d.items into a linear event stream. Each `Loop { items }`
    // becomes `LoopStart(keyword,label) … <inner events> … LoopEnd`.
    // The same scaffold also serves `Opt` (keyword="opt") and `Alt`
    // (where the parser flattens else-arms into AltSection events
    // between Start and End). This keeps the per-item layout body
    // unchanged while still letting us track open-loop bounds via
    // `active_loops`.
    enum WalkEvent<'a> {
        LoopStart {
            keyword: &'static str,
            label: &'a str,
        },
        // Marks the boundary between two `else` arms inside an Alt
        // block. Carries the next arm's label.
        AltSection(&'a str),
        LoopEnd,
        // `rect rgb(...) ... end` — coloured background block. The
        // RectStart carries the fill expression; bounds are tracked
        // exactly like a loop on `active_rects`, and the rect lands
        // on `rects` at RectEnd time.
        RectStart(&'a str),
        RectEnd,
        Item(&'a DiagramItem),
    }
    fn flatten<'a>(items: &'a [DiagramItem], out: &mut Vec<WalkEvent<'a>>) {
        for it in items {
            match it {
                DiagramItem::Loop { label, items: inner } => {
                    out.push(WalkEvent::LoopStart {
                        keyword: "loop",
                        label,
                    });
                    flatten(inner, out);
                    out.push(WalkEvent::LoopEnd);
                }
                DiagramItem::Opt { label, items: inner } => {
                    out.push(WalkEvent::LoopStart {
                        keyword: "opt",
                        label,
                    });
                    flatten(inner, out);
                    out.push(WalkEvent::LoopEnd);
                }
                DiagramItem::Break { label, items: inner } => {
                    out.push(WalkEvent::LoopStart {
                        keyword: "break",
                        label,
                    });
                    flatten(inner, out);
                    out.push(WalkEvent::LoopEnd);
                }
                DiagramItem::Alt { branches } => {
                    if branches.is_empty() {
                        continue;
                    }
                    let first = &branches[0];
                    out.push(WalkEvent::LoopStart {
                        keyword: "alt",
                        label: &first.label,
                    });
                    flatten(&first.items, out);
                    for arm in &branches[1..] {
                        out.push(WalkEvent::AltSection(&arm.label));
                        flatten(&arm.items, out);
                    }
                    out.push(WalkEvent::LoopEnd);
                }
                DiagramItem::Par { branches } => {
                    if branches.is_empty() {
                        continue;
                    }
                    let first = &branches[0];
                    out.push(WalkEvent::LoopStart {
                        keyword: "par",
                        label: &first.label,
                    });
                    flatten(&first.items, out);
                    for arm in &branches[1..] {
                        out.push(WalkEvent::AltSection(&arm.label));
                        flatten(&arm.items, out);
                    }
                    out.push(WalkEvent::LoopEnd);
                }
                DiagramItem::Critical { branches } => {
                    if branches.is_empty() {
                        continue;
                    }
                    let first = &branches[0];
                    out.push(WalkEvent::LoopStart {
                        keyword: "critical",
                        label: &first.label,
                    });
                    flatten(&first.items, out);
                    for arm in &branches[1..] {
                        out.push(WalkEvent::AltSection(&arm.label));
                        flatten(&arm.items, out);
                    }
                    out.push(WalkEvent::LoopEnd);
                }
                DiagramItem::Rect { fill, items: inner } => {
                    out.push(WalkEvent::RectStart(fill));
                    flatten(inner, out);
                    out.push(WalkEvent::RectEnd);
                }
                _ => out.push(WalkEvent::Item(it)),
            }
        }
    }
    let mut events: Vec<WalkEvent> = Vec::with_capacity(d.items.len() + 4);
    flatten(&d.items, &mut events);

    // ── Pre-pass: compute per-loop-block "loopWidth" used to wrap the
    // bracketed title/section labels. Mirrors upstream
    // `calculateLoopBounds` (mermaid.js:139343): for each open block on
    // the stack, accumulate `width = max(width, msgModel.width)` over
    // every inner message, then return `width - labelBoxWidth` at end.
    //
    // `loop_widths[i]` is the wrap-budget for the i-th LoopStart event
    // (in pre-order across the flattened event stream); section
    // labels share the same budget as their parent block. AltSection
    // events do not get their own slot — the same budget applies.
    let loop_widths: Vec<f64> = {
        // Per upstream `calculateLoopBounds` (mermaid.js:139343):
        //   - non-self msg: current.width = max(width, msgModel.width)
        //                                   - labelBoxWidth
        //   - self msg:     current.width = max(width, abs(to-from))
        //                                   - labelBoxWidth
        //   - both branches also update current.from / current.to via
        //     min/max so abs(to-from) tracks the cumulative span.
        // We mirror per open-frame state.
        let mut out: Vec<f64> = Vec::new();
        struct Frame {
            li: usize,
            from: f64,
            to: f64,
            width: f64,
        }
        let mut stack: Vec<Frame> = Vec::new();
        for ev in &events {
            match ev {
                WalkEvent::LoopStart { .. } => {
                    let li = out.len();
                    out.push(0.0);
                    stack.push(Frame {
                        li,
                        from: f64::INFINITY,
                        to: f64::NEG_INFINITY,
                        width: 0.0,
                    });
                }
                WalkEvent::LoopEnd => {
                    if let Some(f) = stack.pop() {
                        out[f.li] = f.width.max(0.0);
                    }
                }
                WalkEvent::RectStart(_) | WalkEvent::RectEnd | WalkEvent::AltSection(_) => {}
                WalkEvent::Item(it) => {
                    if stack.is_empty() {
                        continue;
                    }
                    if let DiagramItem::Message(m) = it {
                        let from_actor = actors.iter().find(|a| a.id == m.from);
                        let to_actor = actors.iter().find(|a| a.id == m.to);
                        let (Some(fa), Some(ta)) = (from_actor, to_actor) else { continue };
                        let fa_cx = fa.x + fa.width / 2.0;
                        let ta_cx = ta.x + ta.width / 2.0;
                        let is_self = m.from == m.to;
                        // msgModel.width approximation. For self,
                        // bounded=0. For non-self, upstream's
                        // buildMessageModel applies activation+arrow
                        // offsets that shrink boundedWidth by ~3-7 px
                        // (`activationWidth/2 - 1 = 4` for `+`-activated
                        // targets + SOLID arrow `-3` stopx). Approximate
                        // with a -7 trim on the cx-cx span; clamps at 0.
                        let raw_bounded = if is_self { 0.0 } else { (fa_cx - ta_cx).abs() };
                        let bounded_width = (raw_bounded - 7.0).max(0.0);
                        let wrap_eff = m.wrap || cfg.wrap;
                        let text_w_term = if wrap_eff || has_katex(&m.text) {
                            0.0
                        } else {
                            let resolved = resolve_hash_entities_for_measure(&m.text);
                            crate::font_metrics::text_width(
                                &resolved,
                                "sans-serif",
                                cfg.message_font_size as f64,
                                false,
                                false,
                            )
                            .round()
                                + 2.0 * cfg.wrap_padding
                        };
                        let msg_width = text_w_term
                            .max(bounded_width + 2.0 * cfg.wrap_padding)
                            .max(cfg.width);
                        // For non-self use msgModel startx/stopx
                        // approximations to widen current.from/.to. For
                        // self use actor.x ± msg.width/2 and ± actor.width/2.
                        if is_self {
                            let half_msg = msg_width / 2.0;
                            let half_w = fa.width / 2.0;
                            let cand_from = (fa.x - half_msg).min(fa.x - half_w);
                            let cand_to = (fa.x + half_msg).max(fa.x + half_w);
                            for fr in stack.iter_mut() {
                                if cand_from < fr.from {
                                    fr.from = cand_from;
                                }
                                if cand_to > fr.to {
                                    fr.to = cand_to;
                                }
                                let span = (fr.to - fr.from).abs();
                                fr.width = fr.width.max(span) - cfg.label_box_width;
                            }
                        } else {
                            // Approximate msgModel.startx/stopx as fa_cx, ta_cx
                            // (ignoring activation widening — close enough).
                            let mstart = fa_cx.min(ta_cx);
                            let mstop = fa_cx.max(ta_cx);
                            for fr in stack.iter_mut() {
                                if mstart < fr.from {
                                    fr.from = mstart;
                                }
                                if mstop > fr.to {
                                    fr.to = mstop;
                                }
                                fr.width = fr.width.max(msg_width) - cfg.label_box_width;
                            }
                        }
                    }
                }
            }
        }
        out
    };
    // Counter into `loop_widths` advanced at each LoopStart in the main
    // walk below — section labels (AltSection) share the parent block's
    // current slot, looked up via the active_loops stack mapping.
    let mut loop_width_cursor: usize = 0;
    // For each open block on `active_loops`, also remember its
    // pre-computed wrap-budget so AltSection labels can wrap with the
    // same width without re-walking events.
    let mut active_loop_widths: Vec<f64> = Vec::new();
    let mut pending_create_actor: Option<String> = None;
    let mut pending_destroy_actors: Vec<String> = Vec::new();

    for event in events.into_iter() {
        let item = match event {
            WalkEvent::LoopStart { keyword, label } => {
                // LOOP_START → consume one idx slot, bumpVerticalPos(boxMargin),
                // capture starty, then bump heightAdjust = postMargin +
                // max(textHeight, labelBoxHeight). For the simple case the
                // bracketed `[label]` fits on one line and textHeight ≤ 20
                // so the max collapses to labelBoxHeight (=20). Mirrors
                // upstream `adjustLoopHeightForWrap` (sequenceRenderer.ts:137949).
                let _loop_start_idx = msg_id_counter;
                msg_id_counter += 1;
                vertical += box_margin;
                let starty = vertical;
                // Bracket-wrap the label. Width budget = `loopWidth -
                // 2*wrapPadding` — but loopWidth depends on inner-msg
                // bounds which we don't have yet. For the simple short-
                // label case the bracketed string fits without wrapping;
                // for now use cfg.width as a generous budget so single-
                // word labels stay on one line.
                // Wrap-budget for this block's title — pre-computed in
                // the calculateLoopBounds-style pre-pass above. Mirrors
                // upstream `wrapLabel("[msg]", loopWidth - 2*wrapPadding,
                // messageFont)` (mermaid.js:137955).
                let lw = loop_widths.get(loop_width_cursor).copied().unwrap_or(0.0);
                loop_width_cursor += 1;
                active_loop_widths.push(lw);
                let raw_bracketed = format!("[{}]", label);
                let wrap_budget = (lw - 2.0 * cfg.wrap_padding).max(0.0);
                let bracketed = if wrap_budget > 0.0 && !has_katex(&raw_bracketed) {
                    wrap_label(
                        &raw_bracketed,
                        wrap_budget,
                        "sans-serif",
                        cfg.message_font_size as f64,
                    )
                } else {
                    raw_bracketed
                };
                let title_lines = split_br(&bracketed);
                let line_h_msg = crate::font_metrics::line_height(
                    "sans-serif",
                    cfg.message_font_size as f64,
                    false,
                    false,
                )
                .round();
                // textDims.height = lines * round(per-line height).
                let text_h = line_h_msg * (title_lines.len() as f64);
                let post_margin = box_margin + cfg.box_text_margin;
                let total_offset = text_h.max(cfg.label_box_height);
                let height_adjust = post_margin + total_offset;
                vertical += height_adjust;
                let li = loops.len();
                loops.push(LoopRender {
                    startx: f64::INFINITY,
                    stopx: f64::NEG_INFINITY,
                    starty,
                    stopy: 0.0,
                    title: bracketed,
                    keyword,
                    idx: 0, // assigned at LoopEnd
                    sections: Vec::new(),
                });
                active_loops.push(li);
                seq_items.push(SeqItem::Loop(li));
                continue;
            }
            WalkEvent::AltSection(label) => {
                // ALT_ELSE / PAR_AND / CRITICAL_OPTION → adjustLoopHeightForWrap with
                //   preMargin  = boxMargin + boxTextMargin (15)
                //   postMargin = boxMargin                  (10)
                // Steps:
                //   1. bumpVerticalPos(preMargin)
                //   2. addSectionToLoop  → divider_y = vertical
                //   3. bumpVerticalPos(postMargin + totalOffset) — but only
                //      add `totalOffset` when `msg.message` is non-empty
                //      (mirrors upstream `if (msg.id && msg.message && ...)`
                //      gate at sequenceRenderer.ts:909).
                // (mermaid.js:138901 ALT_ELSE / PAR_AND / CRITICAL_OPTION
                // branches all go through adjustLoopHeightForWrap.)
                let li = *active_loops.last().expect("AltSection without start");
                let lw = active_loop_widths.last().copied().unwrap_or(0.0);
                let pre_margin = box_margin + cfg.box_text_margin;
                let post_margin = box_margin;
                vertical += pre_margin;
                let divider_y = vertical;
                let has_label = !label.is_empty();
                let raw_bracketed = if has_label {
                    format!("[{}]", label)
                } else {
                    String::new()
                };
                let wrap_budget = (lw - 2.0 * cfg.wrap_padding).max(0.0);
                let bracketed = if has_label && wrap_budget > 0.0 && !has_katex(&raw_bracketed) {
                    wrap_label(
                        &raw_bracketed,
                        wrap_budget,
                        "sans-serif",
                        cfg.message_font_size as f64,
                    )
                } else {
                    raw_bracketed
                };
                let height_adjust = if has_label {
                    let line_h_msg = crate::font_metrics::line_height(
                        "sans-serif",
                        cfg.message_font_size as f64,
                        false,
                        false,
                    )
                    .round();
                    let n_lines = split_br(&bracketed).len() as f64;
                    let text_h = line_h_msg * n_lines;
                    let total_offset = text_h.max(cfg.label_box_height);
                    post_margin + total_offset
                } else {
                    post_margin
                };
                // The section label is centred on the row that lies
                // BETWEEN divider_y and divider_y + height_adjust. The
                // y_input handed to drawText mirrors title's offset:
                // upstream uses `y = divider_y + boxMargin + boxTextMargin`
                // (the same offset as title above the labelBox row).
                let label_y_input = divider_y + box_margin + cfg.box_text_margin;
                vertical += height_adjust;
                let label_idx = msg_id_counter;
                msg_id_counter += 1;
                loops[li].sections.push(LoopSection {
                    divider_y,
                    label: bracketed,
                    label_y: label_y_input,
                    label_idx,
                });
                continue;
            }
            WalkEvent::LoopEnd => {
                // LOOP_END → consume one idx slot, finalize stopy/stopx
                // bounds, and bumpVerticalPos to loopModel.stopy.
                // X-outset and stopy-outset are already applied at each
                // bounds.insert (see widening pass below); here we only
                // bumpVerticalPos to the final stopy and pop the stack.
                let li = active_loops.pop().expect("LoopEnd without start");
                active_loop_widths.pop();
                seq_items.pop();
                let lr = &mut loops[li];
                // If a loop has zero inner messages, startx/stopx stay
                // at ±∞ — there is no actor extent contribution. Snap to
                // the entire actor lattice in that degenerate case (not
                // exercised by any current fixture; safe default).
                if lr.startx.is_infinite() {
                    lr.startx = bounds_startx;
                    lr.stopx = bounds_stopx;
                }
                if lr.stopy == 0.0 {
                    // No inner messages widened stopy — use vertical.
                    lr.stopy = vertical;
                }
                lr.idx = msg_id_counter;
                msg_id_counter += 1;
                // bumpVerticalPos(loopModel.stopy - getVerticalPos())
                vertical = lr.stopy;
                continue;
            }
            WalkEvent::RectStart(fill) => {
                // RECT_START → adjustLoopHeightForWrap with
                //   preMargin  = boxMargin (10)
                //   postMargin = boxMargin (10)
                // No msg.id / msg.message → no totalOffset bump, just
                // pre + post = 20 px header (sequenceRenderer.ts:1172).
                // We track the open rect on `pending_rects` (a stack
                // of partial models) and only push the finalised
                // `RectRender` to `rects` at RECT_END time — this
                // gives `rects` the END order the upstream
                // `backgrounds.forEach + .lower()` chain produces.
                let _rect_start_idx = msg_id_counter;
                msg_id_counter += 1;
                vertical += box_margin;
                let starty = vertical;
                vertical += box_margin;
                let pending_idx = pending_rects.len();
                pending_rects.push(RectRender {
                    startx: f64::INFINITY,
                    stopx: f64::NEG_INFINITY,
                    starty,
                    stopy: 0.0,
                    fill: fill.to_string(),
                });
                seq_items.push(SeqItem::Rect(pending_idx));
                continue;
            }
            WalkEvent::RectEnd => {
                // RECT_END → mirror LOOP_END. X / stopy outsets are
                // already applied per insert; here we just pop and
                // bumpVerticalPos to stopy.
                let mut r = pending_rects.pop().expect("RectEnd without start");
                seq_items.pop();
                if r.startx.is_infinite() {
                    r.startx = bounds_startx;
                    r.stopx = bounds_stopx;
                }
                if r.stopy == 0.0 {
                    r.stopy = vertical;
                }
                let _rect_end_idx = msg_id_counter;
                msg_id_counter += 1;
                vertical = r.stopy;
                rects.push(r);
                continue;
            }
            WalkEvent::Item(it) => it,
        };
        let idx = msg_id_counter;
        if !matches!(item, DiagramItem::Create(_) | DiagramItem::Destroy(_)) {
            msg_id_counter += 1;
        }
        if let DiagramItem::Message(m) = item {
            if let Some(cc) = m.central_connection {
                msg_id_counter += match cc {
                    CentralConnection::AtTo | CentralConnection::AtFrom => 1,
                    CentralConnection::Dual => 2,
                };
            }
            // `+` / `-` suffixes (jison.333-338) emit a follow-up
            // activeStart / activeEnd event into the parser stream
            // — each consumes one slot on its own. CC events already
            // claim their own activation slots above (jison.341-352),
            // so skip here when central_connection is set.
            if m.central_connection.is_none() {
                if m.activate {
                    msg_id_counter += 1;
                }
                if m.deactivate {
                    msg_id_counter += 1;
                }
            }
        }
        // ── Activate / Deactivate items ────────────────────────────
        //
        // Mirrors upstream sequenceRenderer.ts:1145-1156 ACTIVE_START /
        // ACTIVE_END dispatch. ACTIVE_START pushes a new activation slot
        // (no SVG output on its own); ACTIVE_END pops the most-recent
        // open slot for the actor and emits ONE `<rect class="activationN">`
        // rectangle. The special case at sequenceRenderer.ts:1112-1116
        // forces a minimum 18-pixel rect height when starty+18 >
        // verticalPos: starty becomes verticalPos-6, then verticalPos +=
        // 12, giving stopy-starty = 18.
        if let DiagramItem::Activate(actor_id) = item {
            if let Some(actor) = actors.iter().find(|a| &a.id == actor_id) {
                let centre = actor.x + actor.width / 2.0;
                // Render-pass activation (used for anchor `<g>` emission).
                let render_stacked = activations
                    .iter()
                    .filter(|a| &a.actor == actor_id)
                    .count() as f64;
                let render_x =
                    centre + ((render_stacked - 1.0) * ACTIVATION_WIDTH) / 2.0;
                let anchor_idx = activation_anchors.len();
                // ACTIVE_START events are appended to the parser stream
                // AFTER the addMessage that carries the `+` suffix —
                // jison.333. They claim the next id slot.
                activation_anchors.push((idx, None));
                activations.push(ActivationSlot {
                    startx: render_x,
                    starty: vertical + 2.0,
                    actor: actor_id.clone(),
                    anchor_idx,
                });
                // Layout-pass activation (used for message startx/stopx
                // adjustment via activationBounds). Layout-pass startx
                // mirrors render-pass formula because upstream uses the
                // same newActivation in both passes.
                let layout_stacked = layout_activations
                    .iter()
                    .filter(|a| &a.actor == actor_id)
                    .count() as f64;
                let layout_x =
                    centre + ((layout_stacked - 1.0) * ACTIVATION_WIDTH) / 2.0;
                layout_activations.push(LayoutActivation {
                    startx: layout_x,
                    stopx: layout_x + ACTIVATION_WIDTH,
                    actor: actor_id.clone(),
                });
            }
            continue;
        }
        if let DiagramItem::Deactivate(actor_id) = item {
            let last_render = activations
                .iter()
                .enumerate()
                .rev()
                .find(|(_, a)| &a.actor == actor_id)
                .map(|(i, _)| i);
            if let Some(li) = last_render {
                let slot = activations.remove(li);
                // Special case (sequenceRenderer.ts:1112-1116): when
                // the activation hasn't been open long enough for an
                // 18-px tall rect, force starty back from verticalPos and
                // bump the LOCAL stopy by 12. Upstream does NOT
                // propagate the +12 to bounds.verticalPos (the LOCAL
                // `verticalPos` arg is only used by drawActivation + the
                // immediately-following bounds.insert), so we leave
                // `vertical` untouched.
                let mut starty = slot.starty;
                let mut stopy = vertical;
                if starty + 18.0 > stopy {
                    starty = stopy - 6.0;
                    stopy += 12.0;
                }
                // class N = `actorActivations(msg.from).length` taken
                // POST-splice (sequenceRenderer.ts:1122) — i.e. the
                // remaining same-actor slots after pop, mod 3.
                let remaining = activations
                    .iter()
                    .filter(|a| &a.actor == actor_id)
                    .count() as u32;
                activation_anchors[slot.anchor_idx].1 = Some(ActivationRect {
                    x: slot.startx,
                    y: starty,
                    height: stopy - starty,
                    class_n: remaining % 3,
                });
            }
            // Pop the matching layout-pass entry.
            if let Some(li) = layout_activations
                .iter()
                .enumerate()
                .rev()
                .find(|(_, a)| &a.actor == actor_id)
                .map(|(i, _)| i)
            {
                layout_activations.remove(li);
            }
            continue;
        }
        // ── Create / Destroy items ──────────────────────────────────
        //
        // Mirrors upstream CREATE_START / ACTIVE_END (destroy) dispatch.
        // CREATE_START bumps verticalPos by boxMargin, then captures the
        // bumped position as the created actor's y offset (their top rect
        // / body starts at that y instead of y=0). ACTIVE_END (destroy)
        // captures the current verticalPos as the lifeline termination
        // point — the destroyed actor's lifeline <line y2> ends there.
        if let DiagramItem::Create(actor) = item {
            pending_create_actor = Some(actor.id.clone());
            continue;
        }
        if let DiagramItem::Destroy(ref actor_id) = item {
            pending_destroy_actors.push(actor_id.clone());
            continue;
        }
        // Autonumber: update running counters/visibility, no SVG output
        // of its own — it occupies an item-id slot but doesn't draw.
        if let DiagramItem::Autonumber { start, step, visible } = item {
            if let Some(s) = start {
                auto_seq_index = *s;
            }
            if let Some(s) = step {
                auto_seq_step = *s;
            }
            auto_visible = *visible;
            continue;
        }
        if let DiagramItem::Note(note) = item {
            let placement = note.placement.expect("gated");
            let actor_id = &note.placement_actors[0];
            let Some(actor_idx) = d.actors.iter().position(|a| &a.id == actor_id) else {
                return Ok(placeholder(d, id));
            };
            let from_actor = &actors[actor_idx];
            // For `Note over A,B` (2 actors, Over) — resolve the
            // second actor here so the cross-actor span branch below
            // can compute the upstream `else`-branch geometry.
            let to_actor_for_over: Option<&_> = if note.placement_actors.len() == 2
                && matches!(placement, NotePlacement::Over)
            {
                let to_id = &note.placement_actors[1];
                match d.actors.iter().position(|a| &a.id == to_id) {
                    Some(i) => Some(&actors[i]),
                    None => return Ok(placeholder(d, id)),
                }
            } else {
                None
            };
            // buildNoteModel for single-actor placement.
            // Optional `:wrap:` prefix triggers a two-stage wrap:
            //   1. First wrapLabel(msg.text, conf.width, noteFont)
            //      → measures dims of the wrapped text.
            //   2. noteModel.width computed from the dims via
            //      placement-specific max formula.
            //   3. Second wrapLabel(msg.text, noteModel.width -
            //      2*wrapPadding, noteFont) → final wrapped text used
            //      for emission.
            let should_wrap = note.wrap && !note.text.is_empty();
            let intermediate_text = if should_wrap {
                wrap_label(&note.text, cfg.width, "trebuchet ms", cfg.message_font_size as f64)
            } else {
                note.text.clone()
            };
            let intermediate_lines = split_br(&intermediate_text);
            let mut text_w = 0.0_f64;
            if !has_katex(&intermediate_text) {
                for line in &intermediate_lines {
                    let resolved = resolve_hash_entities_for_measure(line);
                    let w = crate::font_metrics::text_width(
                        &resolved,
                        "trebuchet ms",
                        cfg.message_font_size as f64,
                        false,
                        false,
                    )
                    .round();
                    if w > text_w {
                        text_w = w;
                    }
                }
            }
            let note_w: f64;
            let note_x: f64;
            match placement {
                NotePlacement::RightOf => {
                    // upstream RIGHTOF:
                    //   width = shouldWrap
                    //     ? max(conf.width, textW)
                    //     : max(fromW/2 + toW/2, textW + 2*noteMargin)
                    //   startx = fromX + (fromW + actorMargin) / 2
                    note_w = if should_wrap {
                        cfg.width.max(text_w)
                    } else {
                        from_actor
                            .width
                            .max(text_w + 2.0 * cfg.note_margin)
                    };
                    note_x = from_actor.x + (from_actor.width + actor_margin) / 2.0;
                }
                NotePlacement::LeftOf => {
                    // upstream LEFTOF:
                    //   width = shouldWrap
                    //     ? max(conf.width, textW + 2*noteMargin)
                    //     : max(fromW/2 + toW/2, textW + 2*noteMargin)
                    //   startx = fromX - width + (fromW - actorMargin) / 2
                    note_w = if should_wrap {
                        cfg.width.max(text_w + 2.0 * cfg.note_margin)
                    } else {
                        from_actor
                            .width
                            .max(text_w + 2.0 * cfg.note_margin)
                    };
                    note_x = from_actor.x - note_w
                        + (from_actor.width - actor_margin) / 2.0;
                }
                NotePlacement::Over => {
                    if let Some(to_actor) = to_actor_for_over {
                        // upstream `else` branch (2-actor Over,
                        // msg.from !== msg.to):
                        //   width  = |fromX + fromW/2 - (toX + toW/2)|
                        //          + actorMargin
                        //   startx = (fromX < toX
                        //               ? fromX + fromW/2
                        //               : toX + toW/2)
                        //          - actorMargin/2
                        // Text width does NOT participate in the
                        // width formula here — the span between actor
                        // centres plus actorMargin is authoritative.
                        let from_center = from_actor.x + from_actor.width / 2.0;
                        let to_center = to_actor.x + to_actor.width / 2.0;
                        note_w = (from_center - to_center).abs() + actor_margin;
                        note_x = if from_actor.x < to_actor.x {
                            from_center - actor_margin / 2.0
                        } else {
                            to_center - actor_margin / 2.0
                        };
                    } else {
                        // upstream OVER (msg.to === msg.from):
                        //   width = shouldWrap
                        //     ? max(conf.width, fromW)
                        //     : max(fromW, conf.width, textW + 2*noteMargin)
                        //   startx = fromX + (fromW - width) / 2
                        note_w = if should_wrap {
                            cfg.width.max(from_actor.width)
                        } else {
                            from_actor
                                .width
                                .max(cfg.width)
                                .max(text_w + 2.0 * cfg.note_margin)
                        };
                        note_x = from_actor.x + (from_actor.width - note_w) / 2.0;
                    }
                }
            }
            // Second-stage wrap: re-wrap to final note_w - 2*wrapPadding
            // when shouldWrap, else use the original (or br-split) text.
            let note_lines: Vec<String> = if should_wrap {
                let final_text = wrap_label(
                    &note.text,
                    note_w - 2.0 * cfg.wrap_padding,
                    "trebuchet ms",
                    cfg.message_font_size as f64,
                );
                split_br(&final_text).iter().map(|s| s.to_string()).collect()
            } else {
                intermediate_lines.iter().map(|s| s.to_string()).collect()
            };
            // drawNote vertical: bumpVerticalPos(boxMargin) → starty;
            // text height = round(SUM of unrounded per-line
            // bbox.heights) = round(lines * lh_unrounded). height =
            // textH + 2*noteMargin; bumpVerticalPos(textH +
            // 2*noteMargin).
            //
            // (This differs from `boundMessage`: messages use
            // `lines * round(lh_unrounded)` from `calculateTextDimensions`,
            // while drawNote sums BEFORE rounding.)
            vertical += box_margin;
            let starty_for_note = vertical;
            let lh_unrounded = crate::font_metrics::line_height(
                "trebuchet ms",
                cfg.message_font_size as f64,
                false,
                false,
            );
            let text_h = if has_katex(&note.text) {
                16.0
            } else {
                (lh_unrounded * (note_lines.len() as f64)).round()
            };
            let note_h = text_h + 2.0 * cfg.note_margin;
            vertical += note_h;

            // Text geometry (drawText byTspan with anchor='center',
            // valign='center', textMargin=noteMargin):
            //   x = round(rect.x + rect.width / 2)
            //   y_n = round(starty + (n*lh + n*lh + noteMargin) / 2)
            //       = round(starty + n*lh + noteMargin/2)
            // where `lh` is the UNROUNDED bbox height per line.
            let text_x = round_js(note_x + note_w / 2.0);
            let text_y_first = round_js(starty_for_note + cfg.note_margin / 2.0);
            // Upstream computes each note text line independently as
            // `round(starty + n*lh + noteMargin/2)` — rounding the full
            // expression rather than accumulating from a rounded first
            // line. To match, we store the raw `starty_for_note` and
            // `note_margin / 2` so the emission pass can round per-line.
            let note_starty_raw = starty_for_note;
            let note_margin_half = cfg.note_margin as f64 / 2.0;

            notes.push(NoteRender {
                lines: note_lines.iter().map(|s| s.to_string()).collect(),
                raw_text: note.text.clone(),
                rect_x: note_x,
                rect_y: starty_for_note,
                rect_w: note_w,
                rect_h: note_h,
                text_x,
                text_y_first,
                line_step: lh_unrounded,
                starty_raw: note_starty_raw,
                margin_half: note_margin_half,
                idx,
            });

            // Update overall bounds.
            if note_x < bounds_startx {
                bounds_startx = note_x;
            }
            if note_x + note_w > bounds_stopx {
                bounds_stopx = note_x + note_w;
            }
            // Widen any open rect/loop blocks so they enclose this note.
            // Mirrors upstream `bounds.insert(noteModel.startx, noteModel.starty,
            // noteModel.stopx, noteModel.stopy)` with `n*boxMargin` outset
            // per nesting level (outermost gets the biggest outset).
            let stack_len = seq_items.len();
            for (cnt0, it) in seq_items.iter().enumerate() {
                let n = (stack_len - cnt0) as f64;
                let widened_startx = note_x - n * box_margin;
                let widened_stopx = note_x + note_w + n * box_margin;
                let widened_stopy = starty_for_note + note_h + n * box_margin;
                match *it {
                    SeqItem::Loop(li) => {
                        let lr = &mut loops[li];
                        if widened_startx < lr.startx {
                            lr.startx = widened_startx;
                        }
                        if widened_stopx > lr.stopx {
                            lr.stopx = widened_stopx;
                        }
                        if widened_stopy > lr.stopy {
                            lr.stopy = widened_stopy;
                        }
                    }
                    SeqItem::Rect(pidx) => {
                        let r = &mut pending_rects[pidx];
                        r.widen_x(widened_startx, widened_stopx);
                        r.widen_stopy(widened_stopy);
                    }
                }
            }
            // Widen overall diagram bounds so the SVG viewBox includes
            // the rect/loop outset (the largest n applies).
            if stack_len > 0 {
                let outermost_n = stack_len as f64;
                let outset = outermost_n * box_margin;
                if note_x - outset < bounds_startx {
                    bounds_startx = note_x - outset;
                }
                if note_x + note_w + outset > bounds_stopx {
                    bounds_stopx = note_x + note_w + outset;
                }
            }
            continue;
        }
        let m = match item {
            DiagramItem::Message(m) => m,
            _ => continue,
        };
        // boundMessage with multi-line support:
        //   bumpVerticalPos(10)
        //   bumpVerticalPos(lineHeight)            // ONCE (= conf.height/n)
        //   totalOffset = (textDims.height - 10) + boxMargin   (non-self)
        //                = textDims.height          (when boxMargin == 10)
        //   lineStartY = vertical + totalOffset
        //   bumpVerticalPos(totalOffset)
        // textDims.height = lines * round(per-line bbox.height) = lines * 19.
        //
        // `wrap:` prefix triggers word-wrapping. Upstream
        // `buildMessageModel` wraps with maxWidth = max(boundedWidth +
        // 2*wrapPadding, conf.width) — NOT conf.width-2*wrapPadding —
        // so the wrap target is the actual on-canvas message span.
        let from_actor = actors.iter().find(|a| a.id == m.from);
        let to_actor = actors.iter().find(|a| a.id == m.to);
        let (Some(fa), Some(ta)) = (from_actor, to_actor) else {
            return Ok(placeholder(d, id));
        };
        let bounded_width = ((fa.x + fa.width / 2.0) - (ta.x + ta.width / 2.0)).abs();
        // Effective wrap = per-message flag OR diagram-level config.
        let wrap_effective = m.wrap || cfg.wrap;
        let final_msg_text = if wrap_effective {
            let max_w = (bounded_width + 2.0 * cfg.wrap_padding).max(cfg.width);
            wrap_label(
                &m.text,
                max_w,
                "sans-serif",
                cfg.message_font_size as f64,
            )
        } else {
            m.text.clone()
        };
        let msg_lines = split_br(&final_msg_text);
        let n_lines = if has_katex(&m.text) { 1.0 } else { msg_lines.len() as f64 };
        let text_dims_height = if has_katex(&m.text) { 0.0 } else { line_height * n_lines };
        let starty_for_msg = vertical;
        vertical += 10.0;
        if !has_katex(&m.text) {
            vertical += line_height;
        }
        let is_self = m.from == m.to;

        // startx / stopx: standard left→right for SolidArrow → arrow_end shrinks by 3.
        //
        // `activationBounds` (sequenceRenderer.ts:886-904): for an actor
        // with N open activations, the seed is `[centre-1, centre+1]`,
        // then the loop reduces with `min`/`max` over each activation's
        // `[startx, stopx]`. With 1 open activation pushed at stacked=0
        // (x=centre-5, stopx=centre+5), bounds = [centre-5, centre+5].
        // With 2 stacked activations (x=centre-5..centre, stopx=
        // centre+5..centre+10), bounds = [centre-5, centre+10]. We
        // consult `layout_activations` (ACTIVE_START / `+` derived only;
        // CC events DON'T contribute to layout bounds — they only
        // contribute to render-pass anchor `<g>`s).
        let activation_bounds = |actor: &str, centre: f64| -> (f64, f64) {
            let mut left = centre - 1.0;
            let mut right = centre + 1.0;
            for la in layout_activations.iter().filter(|a| a.actor == actor) {
                if la.startx < left {
                    left = la.startx;
                }
                if la.stopx > right {
                    right = la.stopx;
                }
            }
            (left, right)
        };
        let fa_centre = fa.x + fa.width / 2.0;
        let ta_centre = ta.x + ta.width / 2.0;
        let (from_left, from_right) = activation_bounds(&m.from, fa_centre);
        let (to_left, to_right) = activation_bounds(&m.to, ta_centre);
        let is_arrow_to_right = from_left <= to_left;
        let mut startx = if is_arrow_to_right {
            from_right
        } else {
            from_left
        };
        let mut stopx = if is_arrow_to_right { to_left } else { to_right };
        if is_self {
            stopx = startx;
        }
        // ── Created / destroyed actor adjustment ───────────────────────
        //
        // Mirrors upstream `adjustCreatedDestroyedData`
        // (sequenceRenderer.ts:967-1014). For a created actor the
        // message's stopx is adjusted inward (receiver side) by half
        // the actor's visual width + 3 so the line ends at the body
        // edge rather than the lifeline centre. For a destroyed actor
        // the startx is similarly adjusted (sender side, without the
        // +3). Upstream also calls bounds.insert here but that only
        // affects bounds_startx/bounds_stopx which are already
        // correct from the actor x-positioning pass.
        fn is_drawn_type(t: &ActorType) -> bool {
            matches!(t, ActorType::Actor | ActorType::Control | ActorType::Entity | ActorType::Database)
        }
        if pending_create_actor.is_some() {
            if ta.x < fa.x {
                let adj = if is_drawn_type(&ta.actor_type) { 18.0 + 3.0 } else { ta.width / 2.0 + 3.0 };
                stopx += adj;
            } else {
                let adj = if is_drawn_type(&ta.actor_type) { 18.0 + 3.0 } else { ta.width / 2.0 + 3.0 };
                stopx -= adj;
            }
        }
        for did in &pending_destroy_actors {
            if did == &m.from {
                if fa.x < ta.x {
                    let adj = if is_drawn_type(&fa.actor_type) { 18.0 } else { fa.width / 2.0 };
                    startx += adj;
                } else {
                    let adj = if is_drawn_type(&fa.actor_type) { 18.0 } else { fa.width / 2.0 };
                    startx -= adj;
                }
            } else if did == &m.to {
                if ta.x < fa.x {
                    let adj = if is_drawn_type(&ta.actor_type) { 18.0 + 3.0 } else { ta.width / 2.0 + 3.0 };
                    stopx += adj;
                } else {
                    let adj = if is_drawn_type(&ta.actor_type) { 18.0 + 3.0 } else { ta.width / 2.0 + 3.0 };
                    stopx -= adj;
                }
            }
        }
        // Central-connection startx adjustment. Mirrors upstream
        // `calculateCentralConnectionOffset` (sequenceRenderer.ts:1768).
        // Upstream adds an absolute (direction-independent) `+= 4` to
        // startx for REVERSE (`'()' signal`, our `AtFrom`) and DUAL
        // (`'()' signal '()'`). Bidirectional sub-offset is `-6` only
        // when RTL (isArrowToRight=false): for LTR bidir+central it is 0.
        if matches!(
            m.central_connection,
            Some(CentralConnection::AtFrom) | Some(CentralConnection::Dual)
        ) {
            startx += 4.0;
            if matches!(m.arrow, Some(ArrowType::BiSolid) | Some(ArrowType::BiDotted))
                && !is_arrow_to_right
            {
                startx -= 6.0;
            }
        }
        let has_arrowhead = matches!(
            m.arrow,
            Some(ArrowType::SolidArrow) | Some(ArrowType::DottedArrow)
        );
        let has_crosshead = matches!(
            m.arrow,
            Some(ArrowType::SolidCross) | Some(ArrowType::DottedCross)
        );
        let has_pointhead = matches!(
            m.arrow,
            Some(ArrowType::SolidPoint) | Some(ArrowType::DottedPoint)
        );
        let is_bidir = matches!(
            m.arrow,
            Some(ArrowType::BiSolid) | Some(ArrowType::BiDotted)
        );
        // Forward filled half-arrow heads (`-|\`, `-|/`, dotted variants):
        // line.x2 shrinks by 3 toward the source, exactly like the
        // standard arrowhead (mirrors upstream's `stopx += adjustValue(3)`
        // — only STICK / OPEN / REVERSE forms are excluded).
        let has_forward_solid_half = matches!(
            m.arrow,
            Some(ArrowType::SolidTop)
                | Some(ArrowType::SolidBottom)
                | Some(ArrowType::SolidTopDotted)
                | Some(ArrowType::SolidBottomDotted)
        );
        // Reverse half-arrows (head at source). Solid reverse shrinks
        // line.x1 toward the source by 3 (`startx -= adjustValue(3)`),
        // stick reverse does NOT shrink either endpoint.
        let has_reverse_solid_half = matches!(
            m.arrow,
            Some(ArrowType::SolidTopReverse)
                | Some(ArrowType::SolidBottomReverse)
                | Some(ArrowType::SolidTopReverseDotted)
                | Some(ArrowType::SolidBottomReverseDotted)
        );
        // Upstream `isReverseArrowType` (sequenceRenderer.ts:4392) covers
        // ALL eight reverse half-arrow types — both solid and stick.
        // Used by the autonumber path (3557) and the autonumber-X
        // selector (3588).
        let is_reverse_arrow = matches!(
            m.arrow,
            Some(ArrowType::SolidTopReverse)
                | Some(ArrowType::SolidBottomReverse)
                | Some(ArrowType::StickTopReverse)
                | Some(ArrowType::StickBottomReverse)
                | Some(ArrowType::SolidTopReverseDotted)
                | Some(ArrowType::SolidBottomReverseDotted)
                | Some(ArrowType::StickTopReverseDotted)
                | Some(ArrowType::StickBottomReverseDotted)
        );
        if !is_self {
            if m.activate {
                if is_arrow_to_right {
                    stopx -= 4.0;
                } else {
                    stopx += 4.0;
                }
            }
            if has_arrowhead
                || has_crosshead
                || has_pointhead
                || is_bidir
                || has_forward_solid_half
            {
                if is_arrow_to_right {
                    stopx -= 3.0;
                } else {
                    stopx += 3.0;
                }
            }
            if is_bidir || has_reverse_solid_half {
                if is_arrow_to_right {
                    startx += 3.0;
                } else {
                    startx -= 3.0;
                }
            }
        }

        let mut total_offset = (text_dims_height - 10.0) + box_margin;
        let line_start_y = vertical + total_offset;
        if is_self {
            total_offset += 30.0;
        }
        vertical += total_offset;

        // Compute dx widening for self-messages: upstream's
        // `bounds.insert(startx - dx, ..., stopx + dx, ...)` at
        // sequenceRenderer.ts:433 uses dx = max(textWidth/2, conf.width/2).
        // The ±dx propagates through updateBounds into every open
        // sequenceItem (loops + rects), widening the rendered block
        // even though the lifeline sits on a single actor centre.
        let self_dx: Option<f64> = if is_self {
            let mut msg_text_width = 0.0_f64;
            if !has_katex(&m.text) {
                for line in &msg_lines {
                    let resolved = resolve_hash_entities_for_measure(line);
                    let w = crate::font_metrics::text_width(
                        &resolved,
                        "sans-serif",
                        cfg.message_font_size as f64,
                        false,
                        false,
                    )
                    .round();
                    if w > msg_text_width {
                        msg_text_width = w;
                    }
                }
            }
            let dx = (msg_text_width / 2.0).max(default_actor_w / 2.0);
            let self_startx = startx - dx;
            let self_stopx = startx + dx;
            if self_startx < bounds_startx {
                bounds_startx = self_startx;
            }
            if self_stopx > bounds_stopx {
                bounds_stopx = self_stopx;
            }
            Some(dx)
        } else {
            None
        };

        // Text positioning (upstream drawText with anchor='center',
        // valign='center', textMargin=wrapPadding=10):
        //   x  = round(startx + (stopx - startx) / 2)
        //   y_n = round(textObj.y + (prev + textH + 10) / 2)
        //       = round(starty + 10 + n * lh + 5)
        // where `lh` is the UNROUNDED bbox height per line (≈ 18.625
        // for sans-serif 16px), accumulated per line in JS land.
        let text_x = round_js((startx + stopx) / 2.0);
        let lh_unrounded = crate::font_metrics::line_height(
            "sans-serif",
            cfg.message_font_size as f64,
            false,
            false,
        );
        let text_y_first = round_js(starty_for_msg + 10.0 + 5.0);
        let line_step = lh_unrounded;

        // Autonumber-active line.x1 shift: upstream
        // `if (sequenceVisible) { line.attr('x1', startx + 6); }` for
        // the standard (non-bidirectional, non-reverse) arrow path.
        let seq_index = if auto_visible {
            Some(auto_seq_index)
        } else {
            None
        };
        // autonumberX = isLeftToRight ? fromBounds + 1 : toBounds - 1
        // where fromBounds = min over all four actor edges, toBounds = max.
        // Reverse-arrow types invert the selector
        // (sequenceRenderer.ts:3588-3592):
        //   isReverse  → isLeftToRight ? toBounds - 1 : fromBounds + 1
        // because the head sits at the source side, so the number circle
        // needs to land on the OTHER end of the lifeline lattice.
        let fa_cx = fa.x + fa.width / 2.0;
        let ta_cx = ta.x + ta.width / 2.0;
        // fromBounds / toBounds upstream are min/max over the FOUR
        // activation-aware edges `[fromLeft, fromRight, toLeft, toRight]`
        // (sequenceRenderer.ts:1974-2000). Activations widen these
        // beyond the ±1 actor centerline. Using activation_bounds()
        // here keeps rect / loop bounds correct when an activation
        // is open at insert time.
        let from_bounds = from_left.min(to_left);
        let to_bounds = from_right.max(to_right);
        // Widen every currently-open sequenceItem (loop OR rect)
        // — mirrors upstream `bounds.insert` →
        // `updateBounds` walking `sequenceItems` at
        // sequenceRenderer.ts:114-128. For self-messages the
        // actor lifeline is a single x but upstream widens by
        // ±dx via `bounds.insert(startx-dx, ..., stopx+dx, ...)`
        // (sequenceRenderer.ts:433). Each item's `n` =
        // `stack.len() - cnt + 1` where cnt is its 1-based
        // position from the bottom — i.e. the OUTERMOST gets the
        // BIGGEST ±n*boxMargin outset.
        // Upstream builds msgModel.startx = fromRight = cx+1 for
        // self-messages (sequenceRenderer.ts:1879-1913), then
        // bounds.insert uses startx ± dx → centre at cx+1 not cx.
        let (item_startx, item_stopx) = if let Some(dx) = self_dx {
            (fa_cx + 1.0 - dx, fa_cx + 1.0 + dx)
        } else {
            (from_bounds, to_bounds)
        };
        // Per upstream `boundMessage` (sequenceRenderer.ts:406-449),
        // self-messages get an EXTRA bounds.insert with a wider y span:
        //   insert(startx-dx, getVerticalPos()-10+totalOffset,
        //          stopx+dx,  getVerticalPos()+30+totalOffset)
        // followed by bumpVerticalPos(totalOffset). After the bump,
        // the insert's stopy lands at `vertical_AFTER + 30`. For
        // non-self the only insert is at lineStartY = vertical_AFTER.
        // `vertical` is already AFTER the bump at this point, so the
        // base is `vertical` plus the optional self bonus.
        let insert_stopy_base = vertical;
        let self_extra_stopy = if is_self { 30.0 } else { 0.0 };
        let line_minx = startx.min(stopx);
        let line_maxx = startx.max(stopx);
        let stack_len = seq_items.len();
        for (cnt0, item) in seq_items.iter().enumerate() {
            let n = (stack_len - cnt0) as f64;
            let mut widened_startx = item_startx - n * box_margin;
            let mut widened_stopx = item_stopx + n * box_margin;
            let line_w_startx = line_minx - n * box_margin;
            let line_w_stopx = line_maxx + n * box_margin;
            if line_w_startx < widened_startx {
                widened_startx = line_w_startx;
            }
            if line_w_stopx > widened_stopx {
                widened_stopx = line_w_stopx;
            }
            let widened_stopy = insert_stopy_base + self_extra_stopy + n * box_margin;
            match *item {
                SeqItem::Loop(li) => {
                    let lr = &mut loops[li];
                    if widened_startx < lr.startx {
                        lr.startx = widened_startx;
                    }
                    if widened_stopx > lr.stopx {
                        lr.stopx = widened_stopx;
                    }
                    if widened_stopy > lr.stopy {
                        lr.stopy = widened_stopy;
                    }
                }
                SeqItem::Rect(pidx) => {
                    let r = &mut pending_rects[pidx];
                    r.widen_x(widened_startx, widened_stopx);
                    r.widen_stopy(widened_stopy);
                }
            }
            // Global diagram bounds also widen by the OUTERMOST item's
            // n*boxMargin (sequenceRenderer.ts:119-120) on every insert.
            // Since we apply widening per item, the largest n (= stack_len)
            // ends up driving bounds_*x — capture that here too.
            if widened_startx < bounds_startx {
                bounds_startx = widened_startx;
            }
            if widened_stopx > bounds_stopx {
                bounds_stopx = widened_stopx;
            }
        }
        // Mirror upstream `bounds.insert(msgModel.fromBounds, msgModel.starty,
        // msgModel.toBounds, msgModel.stopy)` (sequenceRenderer.ts:3459) —
        // the SECOND insert in boundMessage. msgModel.starty was captured
        // BEFORE the message bumps (= the previous `vertical`, which
        // upstream tracks as `bounds.getVerticalPos()` at message entry).
        // Its updateBounds pass applies Math.min on each open activation's
        // `starty` with `msgModel.starty - n*boxMargin` where
        // `n = sequenceItems.length - cnt + 1` (cnt = 1-based iteration
        // index over the activation list, AFTER sequenceItems pass already
        // consumed cnt). The cnt counter is shared across the two foreach
        // passes, so the K-th activation has `cnt = sequenceItems.len + K`
        // and `n = 1 - K` ⇒ only the FIRST activation in iteration order
        // (K=1, n=0) gets pulled with the unmodified starty; later
        // activations get `starty + K-1` and rarely change.
        if !activations.is_empty() {
            let stack_len_for_act = seq_items.len();
            for (k0, slot) in activations.iter_mut().enumerate() {
                let cnt = stack_len_for_act + k0 + 1;
                let n = (stack_len_for_act as i64) - (cnt as i64) + 1;
                let candidate = starty_for_msg - (n as f64) * box_margin;
                if candidate < slot.starty {
                    slot.starty = candidate;
                }
            }
        }
        let seq_x = if is_reverse_arrow {
            if is_arrow_to_right {
                to_bounds - 1.0
            } else {
                from_bounds + 1.0
            }
        } else if is_arrow_to_right {
            from_bounds + 1.0
        } else {
            to_bounds - 1.0
        };
        // sequenceRenderer.ts:3555-3580 — when autonumber is visible,
        // line.x1 shifts past the sequence-number circle. For
        // bidirectional+RTL the shift is `-SEQUENCE_NUMBER_RADIUS = -6`
        // PLUS an extra `-5` if the message also has a `()` central
        // connection, PLUS another `-7.5` for DUAL or REVERSE central
        // connections (matches the line at sequenceRenderer.ts:3567).
        // For LTR bidir, no central-connection adjustments apply.
        // For non-bidir left→right autonumber arrows, `+SEQUENCE_NUMBER_RADIUS=+6`.
        let has_central_conn = m.central_connection.is_some();
        let is_dual_or_reverse_cc = matches!(
            m.central_connection,
            Some(CentralConnection::Dual) | Some(CentralConnection::AtFrom)
        );
        let self_startx = startx;
        let self_line_start_x = if is_self && seq_index.is_some() && is_bidir {
            startx + 10.0
        } else if is_self {
            startx
        } else {
            0.0
        };
        let line_x1 = if is_self {
            if seq_index.is_some() {
                if is_bidir {
                    startx - 6.0
                } else if is_reverse_arrow {
                    // Reverse half-arrow self-loops: x1 stays at the
                    // actor centre (no +6 shift) — the arrowhead clearance
                    // is applied via the path start (+10) and an explicit
                    // `x2 = startx - 6` attribute emitted on the path.
                    startx
                } else {
                    startx + 6.0
                }
            } else {
                startx
            }
        } else if seq_index.is_some() {
            if is_bidir {
                if is_arrow_to_right {
                    startx + 12.0
                } else {
                    let mut x = startx - 6.0;
                    if has_central_conn {
                        x -= 5.0;
                    }
                    if is_dual_or_reverse_cc {
                        x -= 7.5;
                    }
                    x
                }
            } else if is_reverse_arrow {
                // sequenceRenderer.ts:3570-3579 — reverse arrows DO NOT
                // shift x1 by SEQUENCE_NUMBER_RADIUS the way standard
                // arrows do. The only x1 adjustment is the RTL (-7.5)
                // dual/reverse central-connection case.
                if is_arrow_to_right {
                    startx
                } else {
                    let mut x = startx;
                    if is_dual_or_reverse_cc {
                        x -= 7.5;
                    }
                    x
                }
            } else {
                startx + 6.0
            }
        } else {
            startx
        };
        // x2 base. Reverse + autonumber: stopx contracts toward the
        // sequence-number circle (which sits past the destination end of
        // the lifeline lattice in the reverse-arrow layout).
        //   LTR (stopx>startx): lineStopX = stopx - 12
        //   RTL (stopx<startx): lineStopX = stopx - 6
        // Plus +15 if the message has any central connection.
        let line_x2 = if seq_index.is_some() && !is_self && is_reverse_arrow && !is_bidir {
            let mut x = if is_arrow_to_right {
                stopx - 12.0
            } else {
                stopx - 6.0
            };
            if has_central_conn {
                x += 15.0;
            }
            x
        } else if is_self && seq_index.is_some() && is_reverse_arrow && !is_bidir {
            // Reverse half-arrow self-loop with autonumber: emits an
            // explicit `x2 = startx - 6` attribute on the path so the
            // sequence-number circle clears the actor centre.
            stopx - 6.0
        } else {
            stopx
        };
        // Counter increments for every message regardless of visibility —
        // `autonumber off` suppresses display but the counter still
        // advances, so the next re-enabling lands on the post-increment
        // value (upstream sequenceRenderer.ts: bumps autoNumber per signal).
        auto_seq_index += auto_seq_step;

        // Central-connection circle offset when autonumber is on.
        // Mirrors upstream `drawCentralConnection`
        // (sequenceRenderer.ts:329-372 / mermaid.js:138393-138445):
        //   base = isLeftToRight ? +16.5 : -16.5
        //   getCircleOffset(ltr, rev) = rev ? -base : base
        //   AtTo  + reverse  : toCenter   += -base
        //   AtFrom + !reverse: fromCenter += base
        //   Dual  + reverse  : toCenter   += -base
        //   Dual  + !reverse : fromCenter += base
        let mut circle_from_cx = fa_cx;
        let mut circle_to_cx = ta_cx;
        if seq_index.is_some() {
            const CIRCLE_OFFSET: f64 = 16.5;
            let base = if is_arrow_to_right {
                CIRCLE_OFFSET
            } else {
                -CIRCLE_OFFSET
            };
            match m.central_connection {
                Some(CentralConnection::AtTo) => {
                    if is_reverse_arrow {
                        circle_to_cx += -base;
                    }
                }
                Some(CentralConnection::AtFrom) => {
                    if !is_reverse_arrow {
                        circle_from_cx += base;
                    }
                }
                Some(CentralConnection::Dual) => {
                    if is_reverse_arrow {
                        circle_to_cx += -base;
                    } else {
                        circle_from_cx += base;
                    }
                }
                None => {}
            }
        }

        messages.push(MsgRender {
            is_self,
            from: m.from.clone(),
            to: m.to.clone(),
            lines: msg_lines.iter().map(|s| s.to_string()).collect(),
            raw_text: m.text.clone(),
            msg_min_startx: startx.min(stopx),
            msg_dx_abs: (startx - stopx).abs(),
            arrow: m.arrow.unwrap_or(ArrowType::SolidArrow),
            line_start_y,
            text_x,
            text_y_first,
            line_step,
            starty_raw: starty_for_msg,
            y_offset: 15.0,
            line_x1,
            line_x2,
            self_startx,
            self_line_start_x,
            idx,
            seq_index,
            seq_x,
            central_connection: m.central_connection,
            from_cx: circle_from_cx,
            to_cx: circle_to_cx,
        });

        // ── Create / Destroy y-offset assignment ────────────────────
        //
        // In upstream, `create participant X` positions the actor's top
        // group at `lineStartY - actorH/2`, and `destroy X` cuts the
        // lifeline at `lineStartY - actorH/2`. Both are computed from
        // the create/destroy message's lineStartY. The Create/Destroy
        // items fire BEFORE their message in the parser stream, so we
        // defer the assignment until the message's line_start_y is known.
        let half_actor_h = actor_h / 2.0;
        let had_create = pending_create_actor.is_some();
        let cd_ta_x = ta.x;
        let cd_ta_width = ta.width;
        let cd_fa_x = fa.x;
        let cd_to_left = to_left;
        let cd_from_right = from_right;
        let cd_ta_actor_type = ta.actor_type.clone();
        if let Some(actor_id) = pending_create_actor.take() {
            if let Some(ar) = actors.iter_mut().find(|a| a.id == actor_id) {
                let create_y = line_start_y - half_actor_h;
                ar.create_y = Some(create_y);
                ar.starty = create_y;
            }
        }
        let mut had_destroy = false;
        for actor_id in pending_destroy_actors.drain(..) {
            if let Some(ar) = actors.iter_mut().find(|a| a.id == actor_id) {
                ar.destroy_y = Some(line_start_y - half_actor_h);
                had_destroy = true;
            }
        }
        if had_create || had_destroy {
            vertical += half_actor_h;
            // ── Extra bounds.insert for create/destroy ──────────────────
            //
            // Mirrors upstream `adjustCreatedDestroyedData` calling
            // `bounds2.insert()`. For a created actor on the right,
            // the insert uses `original_stopx + adjustment` (before
            // our stopx -= adj), which equals `stopx + 2*adj`.
            // For created on the left, the insert uses `stopx + adj`
            // which equals the unadjusted stopx (since stopx += adj).
            // For destroyed actors, similar logic applies to startx.
            let create_destroy_stopy = vertical;
            let mut cd_minx = startx.min(stopx);
            let mut cd_maxx = startx.max(stopx);
            if had_create {
                let adj = if is_drawn_type(&cd_ta_actor_type) { 18.0 + 3.0 } else { cd_ta_width / 2.0 + 3.0 };
                if cd_ta_x >= cd_fa_x {
                    cd_maxx = cd_maxx.max(stopx + 2.0 * adj);
                } else {
                    cd_minx = cd_minx.min(startx - 2.0 * adj);
                }
            }
            let stack_len = seq_items.len();
            for (cnt0, item) in seq_items.iter().enumerate() {
                let n = (stack_len - cnt0) as f64;
                let widened_stopy = create_destroy_stopy + n * box_margin;
                let w_startx = cd_minx - n * box_margin;
                let w_stopx = cd_maxx + n * box_margin;
                match *item {
                    SeqItem::Loop(li) => {
                        let lr = &mut loops[li];
                        if widened_stopy > lr.stopy {
                            lr.stopy = widened_stopy;
                        }
                        if w_startx < lr.startx {
                            lr.startx = w_startx;
                        }
                        if w_stopx > lr.stopx {
                            lr.stopx = w_stopx;
                        }
                    }
                    SeqItem::Rect(pidx) => {
                        let r = &mut pending_rects[pidx];
                        r.widen_stopy(widened_stopy);
                        r.widen_x(w_startx, w_stopx);
                    }
                }
            }
            if cd_minx < bounds_startx {
                bounds_startx = cd_minx;
            }
            if cd_maxx > bounds_stopx {
                bounds_stopx = cd_maxx;
            }
        }

        // ── Per-message activation side effects ─────────────────────
        fn push_activation(
            activations: &mut Vec<ActivationSlot>,
            anchors: &mut Vec<(usize, Option<ActivationRect>)>,
            actor_id: &str,
            actor_centre: f64,
            cur_vert: f64,
            slot_idx: usize,
        ) {
            let stacked = activations
                .iter()
                .filter(|a| a.actor == actor_id)
                .count() as f64;
            // x = actor.x + actor.width/2 + ((stackedSize-1) * activationWidth) / 2
            // (sequenceRenderer.ts:151). stackedSize starts at 0 → x = centre - 5.
            let x = actor_centre + ((stacked - 1.0) * ACTIVATION_WIDTH) / 2.0;
            let anchor_idx = anchors.len();
            anchors.push((slot_idx, None));
            activations.push(ActivationSlot {
                startx: x,
                starty: cur_vert + 2.0,
                actor: actor_id.to_string(),
                anchor_idx,
            });
        }
        match m.central_connection {
            Some(CentralConnection::AtTo) => {
                push_activation(&mut activations, &mut activation_anchors, &m.to, ta_cx, vertical, idx + 1);
            }
            Some(CentralConnection::AtFrom) => {
                push_activation(&mut activations, &mut activation_anchors, &m.from, fa_cx, vertical, idx + 1);
            }
            Some(CentralConnection::Dual) => {
                // CC then CCR. Order matters: to-side first per jison.350-352.
                push_activation(&mut activations, &mut activation_anchors, &m.to, ta_cx, vertical, idx + 1);
                push_activation(&mut activations, &mut activation_anchors, &m.from, fa_cx, vertical, idx + 2);
            }
            None => {}
        }
        if m.activate && m.central_connection.is_none() {
            // `+` suffix → activeStart{actor: to} (jison.333). Mirrors
            // ACTIVE_START — pushes BOTH the render-pass anchor AND the
            // layout-pass bounds entry.
            push_activation(&mut activations, &mut activation_anchors, &m.to, ta_cx, vertical, idx + 1);
            let layout_stacked = layout_activations
                .iter()
                .filter(|a| a.actor == m.to)
                .count() as f64;
            let layout_x =
                ta_cx + ((layout_stacked - 1.0) * ACTIVATION_WIDTH) / 2.0;
            layout_activations.push(LayoutActivation {
                startx: layout_x,
                stopx: layout_x + ACTIVATION_WIDTH,
                actor: m.to.clone(),
            });
        }
        if m.deactivate {
            // `-` suffix → activeEnd{actor: from} (jison.337).
            let last_idx = activations
                .iter()
                .enumerate()
                .rev()
                .find(|(_, a)| a.actor == m.from)
                .map(|(i, _)| i);
            if let Some(li) = last_idx {
                let slot = activations.remove(li);
                let mut starty = slot.starty;
                let mut stopy = vertical;
                if starty + 18.0 > stopy {
                    starty = stopy - 6.0;
                    stopy += 12.0;
                }
                let remaining = activations
                    .iter()
                    .filter(|a| a.actor == m.from)
                    .count() as u32;
                activation_anchors[slot.anchor_idx].1 = Some(ActivationRect {
                    x: slot.startx,
                    y: starty,
                    height: stopy - starty,
                    class_n: remaining % 3,
                });
            }
            if let Some(li) = layout_activations
                .iter()
                .enumerate()
                .rev()
                .find(|(_, a)| a.actor == m.from)
                .map(|(i, _)| i)
            {
                layout_activations.remove(li);
            }
        }
        // (height/stopy bookkeeping not needed since we only use vertical)
    }
    let _ = bottom_margin_adj;
    let _ = box_margin;

    let mirror = cfg.mirror_actors;

    // After last message: when mirroring, drawActors(true) preamble
    // bumps verticalPos by `boxMargin*2`, then per-actor footer pass
    // adds `maxHeight + boxMargin` so box.stopy = vertical + 95 by
    // default.
    //
    // `maxHeight` upstream is the max of `drawActor` return values, which
    // is each actor's `actor.height` AFTER the top-pass mutation. For
    // most types the mutation lands at:
    //   Participant : initial actor.height (no body-bbox mutation
    //                 beyond the wrap-driven case)
    //   Actor       : 65 (stickman bbox: cy-r .. legs)
    //   Boundary    : 64 (44 bbox + 20 labelBoxHeight)
    //   Control     : 84 (44 bbox + 2*20 labelBoxHeight)
    //   Entity      : 64 (44 bbox + 20 labelBoxHeight)
    //
    // We track this per-type so the box_stopy / svg_height match upstream
    // exactly even when control inflates the maxHeight above conf.height.
    // Per-actor mutated `actor.height` (post top-pass drawActor) governs the
    // `maxHeight` upstream computes from the bottom-pass return values.
    // Upstream initialises `maxHeight = 0` then maxes over each `drawActor`
    // return value, so the result reflects the smallest cylinder/figure when
    // a diagram has no Participant/Actor.
    let actor_h_for = |a: &ActorRender| -> f64 {
        match a.actor_type {
            ActorType::Control => 84.0,
            ActorType::Boundary | ActorType::Entity => 64.0,
            // Database `actor.height = bbox(lastPath) + labelBoxHeight`. The
            // last path command is the right-side `l 0,-(h3-2*ry)` line; in
            // SVG `getBBox` collapses to that segment's height = body_h.
            // body_h = h3 - 2*ry, h3 = width/3, rx = h3/2,
            // ry = rx / (2.5 + w4/50). For default width=150 -> ~35.71.
            ActorType::Database => {
                let w4 = a.width / 3.0;
                let h3 = w4;
                let rx = w4 / 2.0;
                let ry = rx / (2.5 + w4 / 50.0);
                (h3 - 2.0 * ry) + 20.0
            }
            _ => actor_h,
        }
    };
    let max_actor_height = actors.iter().map(actor_h_for).fold(0.0_f64, f64::max);
    let (bottom_y, box_stopy) = if mirror {
        let by = vertical + box_margin * 2.0;
        let stopy = by + max_actor_height + box_margin;
        (by, stopy)
    } else {
        (vertical, vertical)
    };

    // Finalize box stopy. Upstream sequenceRenderer.ts:1386:
    //   box.height = bounds.getVerticalPos() - box.y
    //   boxPadding = boxMargin * 2
    //   box.starty = box.y - boxPadding * 0.25
    //   box.stopy = box.starty + box.height + boxPadding * 0.75
    // bounds.getVerticalPos() at this point = box_stopy.
    let has_boxes = !box_renders.is_empty();
    for br in &mut box_renders {
        let box_y = br.starty + box_margin * 2.0 * 0.25;
        let box_height = box_stopy - box_y;
        br.stopy = br.starty + box_height + (box_margin * 2.0) * 0.75;
    }

    // ── viewBox + size ──────────────────────────────────────────────
    // upstream:
    //   width = (box.stopx - box.startx) + 2 * diagramMarginX
    //   height = (box.stopy - box.starty) + 2 * diagramMarginY
    //   if mirrorActors:
    //     height -= boxMargin
    //     height += bottomMarginAdj
    //   viewBox.x = box.startx - diagramMarginX
    //   viewBox.y = -diagramMarginY  (assumes box.starty = 0)
    // box.startx / stopx are tracked across actor placements AND note
    // placements (notes can extend beyond the lifeline lattice).
    let box_width = bounds_stopx - bounds_startx;
    let svg_width = box_width + 2.0 * dia_margin_x;
    let mut svg_height = box_stopy + 2.0 * dia_margin_y;
    // When boxes are present, upstream bumps verticalPos by boxMargin
    // after box finalisation (sequenceRenderer.ts:1390). This extra
    // height feeds into the SVG dimensions.
    if has_boxes {
        svg_height += box_margin;
    }
    if mirror {
        svg_height = svg_height - box_margin + bottom_margin_adj;
    }
    // Frontmatter `title:` lifts the diagram down by 40 px to make room
    // for the title bar above. Upstream sequenceRenderer.draw():
    //   const extraVertForTitle = title ? 40 : 0;
    //   viewBox y = -(diagramMarginY + extraVertForTitle)
    //   viewBox height = svgHeight + extraVertForTitle
    let has_title = d
        .title
        .as_ref()
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let extra_vert_for_title = if has_title { 40.0 } else { 0.0 };
    let vb_x = bounds_startx - dia_margin_x;
    let vb_y = -dia_margin_y - extra_vert_for_title;
    let vb_height = svg_height + extra_vert_for_title;

    // ── Emit ────────────────────────────────────────────────────────
    let mut out = String::with_capacity(28 * 1024);
    out.push_str("<svg id=\"");
    out.push_str(id);
    out.push_str("\" width=\"100%\" xmlns=\"http://www.w3.org/2000/svg\" style=\"max-width: ");
    push_num(&mut out, svg_width);
    out.push_str("px;\" viewBox=\"");
    push_num(&mut out, vb_x);
    out.push(' ');
    push_num(&mut out, vb_y);
    out.push(' ');
    push_num(&mut out, svg_width);
    out.push(' ');
    push_num(&mut out, vb_height);
    out.push_str(
        "\" role=\"graphics-document document\" aria-roledescription=\"sequence\"",
    );
    let acc_title = d.meta.acc_title.as_deref();
    let acc_descr = d.meta.acc_descr.as_deref();
    if acc_descr.is_some() {
        out.push_str(" aria-describedby=\"chart-desc-");
        out.push_str(id);
        out.push('"');
    }
    if acc_title.is_some() {
        out.push_str(" aria-labelledby=\"chart-title-");
        out.push_str(id);
        out.push('"');
    }
    out.push('>');
    if let Some(t) = acc_title {
        out.push_str("<title id=\"chart-title-");
        out.push_str(id);
        out.push_str("\">");
        out.push_str(&xml_escape(t));
        out.push_str("</title>");
    }
    if let Some(dsc) = acc_descr {
        out.push_str("<desc id=\"chart-desc-");
        out.push_str(id);
        out.push_str("\">");
        out.push_str(&xml_escape(dsc));
        out.push_str("</desc>");
    }

    // Box rects — emit BEFORE background rects so they sit at the
    // bottom of the visual stack. Upstream iterates boxes in forward
    // order, each drawBox calls g.lower() which pushes it to the front
    // of the DOM — so the LAST box in iteration order appears FIRST
    // in the DOM. We iterate in REVERSE to match this (each lowered
    // box pushes earlier ones down, so reverse iteration + sequential
    // output mirrors the lower-stack DOM order).
    for br in box_renders.iter().rev() {
        out.push_str("<g><rect x=\"");
        push_num(&mut out, br.startx);
        out.push_str("\" y=\"");
        push_num(&mut out, br.starty);
        out.push_str("\" fill=\"");
        out.push_str(&br.fill);
        out.push_str("\" stroke=\"rgb(0,0,0, 0.5)\" width=\"");
        push_num(&mut out, br.stopx - br.startx);
        out.push_str("\" height=\"");
        push_num(&mut out, br.stopy - br.starty);
        out.push_str("\" class=\"rect\"></rect>");
        if !br.label.is_empty() {
            out.push_str("<text x=\"");
            push_num(&mut out, br.text_x);
            out.push_str("\" y=\"");
            push_num(&mut out, br.text_y);
            out.push_str("\" style=\"text-anchor: middle; font-size: 16px; font-weight: 400; font-family: &quot;trebuchet ms&quot;, verdana, arial;\" dominant-baseline=\"central\" alignment-baseline=\"central\" class=\"text\"><tspan x=\"");
            push_num(&mut out, br.text_x);
            out.push_str("\" dy=\"0\">");
            out.push_str(&xml_escape(&br.label));
            out.push_str("</tspan></text>");
        }
        out.push_str("</g>");
    }

    // Background rects — emit BEFORE everything else so they sit at
    // the bottom of the visual stack. Upstream pushes models in
    // RECT_END order, then `backgrounds.forEach + rectElement.lower()`
    // reverses that into DOM order (each `.lower()` reasserts as the
    // first child of the parent). We push to `rects` at RECT_END time,
    // so iterating in REVERSE here matches the lower-stack flip
    // (svgDrawCommon.ts:54 drawBackgroundRect → rectElement.lower()).
    for r in rects.iter().rev() {
        emit_rect(&mut out, r);
    }

    // Bottom actor groups — REVERSE iteration (`.lower()` semantics
    // mirror upstream svgDraw: each lowered group displaces those that
    // came before it).
    //
    // Participant -> full <g><rect actor-bottom>+<text></g>.
    // Actor (stick-figure) -> EMPTY placeholder <g></g> (the body for
    // an actor's bottom is rendered AFTER messages; `drawActorTypeActor`
    // creates the empty `g` here just so its `.lower()` pushes it to
    // the front of the DOM).
    //
    // Skipped entirely when `mirrorActors: false` — upstream simply
    // doesn't call `drawActors(true)` in that case.
    // Footer-pass quirk (see `emit_actor_box_text_footer_empty`): in
    // sequence/07.svg the first iterated footer actor whose KaTeX
    // description is *contained as a substring* of another actor's
    // description gets a stray empty `<switch>` because upstream's
    // non-awaited byKatex promise rejects when KaTeX hits a duplicate
    // expression. In sequence/08.svg every actor description has a
    // unique KaTeX body, so this never fires.
    let footer_empty_switch_id: Option<&str> = if mirror {
        let mut chosen: Option<&str> = None;
        for a in actors.iter() {
            if !has_katex(&a.description) {
                continue;
            }
            let katex_body = extract_first_katex_body(&a.description);
            if katex_body.is_empty() {
                continue;
            }
            // The KaTeX body of actor A appears, *as a substring*,
            // inside another actor B's KaTeX body — i.e. the same
            // expression is rendered twice (once standalone for A,
            // once nested inside B). Upstream's non-awaited byKatex
            // calls share the same KaTeX module instance and the
            // duplicated render hits a state-pollution path that
            // causes the second `await` (renderKatexSanitized) to
            // throw, leaving an empty `<switch>` behind. We treat
            // that as the trigger condition.
            let dup = actors.iter().any(|b| {
                if std::ptr::eq(a, b) || b.description == a.description {
                    return false;
                }
                let other_body = extract_first_katex_body(&b.description);
                !other_body.is_empty()
                    && other_body.contains(&katex_body)
                    && b.description.len() > a.description.len()
            });
            if dup {
                chosen = Some(a.id.as_str());
                break;
            }
        }
        chosen
    } else {
        None
    };
    if mirror {
        for a in actors.iter().rev() {
            let actor_bottom_y = a.destroy_y.unwrap_or(bottom_y);
            let emit_empty = footer_empty_switch_id == Some(a.id.as_str());
            match a.actor_type {
                ActorType::Participant => emit_actor_bottom_participant(&mut out, a, actor_bottom_y, emit_empty),
                // Actor / Boundary / Control / Entity all emit empty
                // <g></g> placeholder for their bottom group's `line2`
                // (lowered to front). Bodies emit later, after defs.
                ActorType::Actor
                | ActorType::Boundary
                | ActorType::Control
                | ActorType::Entity => out.push_str("<g></g>"),
                // Database / Queue / Collections bottom groups are FULL —
                // upstream `drawActorTypeXxx` appends body shape + text
                // directly inside the lowered `<g>` instead of emitting a
                // body group later. So we emit the complete body here.
                ActorType::Database => emit_actor_database_bottom_group(&mut out, a, actor_bottom_y),
                ActorType::Queue => emit_actor_queue_bottom_group(&mut out, a, actor_bottom_y),
                ActorType::Collections => {
                    emit_actor_collections_bottom_group(&mut out, a, actor_bottom_y)
                }
                _ => unreachable!("gated above"),
            }
        }
    }
    // Top actor groups — REVERSE iteration. Same `.lower()` semantics
    // applies. For Participant, the lifeline lives INSIDE the same
    // outer <g> as rect+text. For Actor, the lifeline is a SEPARATE
    // <g> (emitted here) and the body is emitted later (after defs).
    //
    // Reference SVGs are post-processed by `generate_ref.mjs:normaliseSvg`,
    // which renumbers every `actorN` / `root-N` id by FIRST DOM-APPEARANCE.
    // We emit the already-normalised numbers directly — so the rank
    // we feed each emit fn matches its DOM index.
    // Lifeline y2 is the actor's `stopy` for mirrorActors=true; for
    // mirrorActors=false the upstream `fixLifeLineHeights` skips the
    // y2 rewrite (actor.stopy is undefined), so the value stays at the
    // initial 2000 default that drawActorType* assigned.
    let lifeline_y2_default = if mirror { bottom_y } else { 2000.0 };
    let force_menus = d.config.force_menus;
    // Database / Queue / Collections `root-N` ids are renumbered by the
    // reference normaliser (`generate_ref.mjs:counterRules`) per first-DOM-
    // appearance, NOT by the upstream `actorCnt` (which uses declaration
    // order). Since we walk actors in reverse here and emit each top group
    // sequentially, we just need a counter that increments once per actor
    // of these types encountered in DOM order — i.e. in this reverse
    // iteration. The counter is shared across all three types because they
    // all match the same `root-N` regex in `counterRules`.
    let mut root_counter: usize = 0;
    for (rank, a) in actors.iter().rev().enumerate() {
        // Upstream `drawActorTypeParticipant` / `drawActorTypeActor`:
        //   if (Object.keys(actor.links || {}).length && !conf.forceMenus) {
        //     g.attr("onclick", popupMenuToggle(...)).attr("cursor", "pointer");
        //   }
        // i.e. the per-actor onclick wrapper is suppressed when forceMenus
        // is set — instead, the `<g id="actorN_popup">` block emits with
        // `display="block !important"` later (see emit_actor_popup).
        let popup = !a.links.is_empty() && !force_menus;
        let lifeline_y2 = a.destroy_y.unwrap_or(lifeline_y2_default);
        match a.actor_type {
            ActorType::Participant => {
                emit_actor_top_participant(
                    &mut out,
                    a,
                    lifeline_y2,
                    rank,
                    root_counter,
                    popup,
                );
                root_counter += 1;
            }
            // Boundary uses the same lifeline-only top group as Actor — body
            // emits later after defs. Lifeline `<g><line id="actorN" .../></g>`
            // is identical to Actor (centerY = actor.height + 15 = 80).
            ActorType::Actor | ActorType::Boundary => {
                let centery = a.create_y.unwrap_or(a.starty) + a.height + 15.0;
                emit_actor_top_lifeline_actor(
                    &mut out,
                    a,
                    centery,
                    lifeline_y2,
                    rank,
                    popup,
                )
            }
            // Control / Entity use centerY = actor_y + 75 (vs Actor's
            // actor.height + 15). Same `<g><line .../></g>` shape, body
            // emits later after defs.
            ActorType::Control | ActorType::Entity => {
                let centery = a.create_y.unwrap_or(a.starty) + 75.0;
                emit_actor_top_lifeline_actor(&mut out, a, centery, lifeline_y2, rank, popup)
            }
            // Database / Queue / Collections: lifeline + root-N wrapper +
            // body shape + text packed inside a SINGLE outer <g> (upstream's
            // `boxplusLineGroup`). No separate body emission later — top
            // group is self-contained.
            ActorType::Database => {
                emit_actor_database_top_group(
                    &mut out,
                    a,
                    lifeline_y2,
                    rank,
                    root_counter,
                    popup,
                );
                root_counter += 1;
            }
            ActorType::Queue => {
                emit_actor_queue_top_group(
                    &mut out,
                    a,
                    lifeline_y2,
                    rank,
                    root_counter,
                    popup,
                );
                root_counter += 1;
            }
            ActorType::Collections => {
                emit_actor_collections_top_group(
                    &mut out,
                    a,
                    lifeline_y2,
                    rank,
                    root_counter,
                    popup,
                );
                root_counter += 1;
            }
            _ => unreachable!("gated above"),
        }
    }

    // Style + empty <g> placeholder. Theme-driven rebuild matches
    // upstream `getStyles()` once cssMin collapses whitespace.
    out.push_str("<style>");
    out.push_str(&consts::build_style(id, theme));
    out.push_str("</style><g></g>");

    // 11 defs in fixed upstream order.
    out.push_str(&consts::DEF_COMPUTER.replace("__ID__", id));
    out.push_str(&consts::DEF_DATABASE.replace("__ID__", id));
    out.push_str(&consts::DEF_CLOCK.replace("__ID__", id));
    out.push_str(&consts::DEF_ARROWHEAD.replace("__ID__", id));
    out.push_str(&consts::DEF_CROSSHEAD.replace("__ID__", id));
    out.push_str(&consts::DEF_FILLED_HEAD.replace("__ID__", id));
    out.push_str(&consts::DEF_SEQUENCE_NUMBER.replace("__ID__", id));
    out.push_str(&consts::DEF_SOLID_TOP.replace("__ID__", id));
    out.push_str(&consts::DEF_SOLID_BOTTOM.replace("__ID__", id));
    out.push_str(&consts::DEF_STICK_TOP.replace("__ID__", id));
    out.push_str(&consts::DEF_STICK_BOTTOM.replace("__ID__", id));

    // Activation anchors. Mirrors upstream's `bounds.newActivation(...)`
    // → `svgDraw.anchorElement(diagram)` → `elem.append('g')` for each
    // CENTRAL_CONNECTION / CENTRAL_CONNECTION_REVERSE /
    // CENTRAL_CONNECTION_DUAL / ACTIVE_START + each `+` suffix on a
    // message — these produce empty `<g></g>` placeholders right after
    // `</defs>`, before the message text/line/circle groups.
    //   AtTo (`actor signal '()' actor`)         → 1 anchor (at `to`)
    //   AtFrom (`actor '()' signal actor`)       → 1 anchor (at `from`)
    //   Dual (`actor '()' signal '()' actor`)    → 2 anchors
    //   `activate <actor>`                        → 1 anchor
    //   `actor signal + actor text` (`+` suffix) → 1 anchor (at `to`)
    // Per jison.331-355 + sequenceRenderer.ts:1145-1156.
    //
    // Anchors that get a matching `deactivate <actor>` / `-` suffix /
    // ACTIVE_END have a `<rect class="activationN">` inserted into the
    // anchor `<g>` by `svgDraw.drawActivation`
    // (sequenceRenderer.ts:1117-1125). Anchors whose activation never
    // closes stay empty — that's fixture 24's case for the four
    // unclosed activations (Alice CC L5, Bob CCR L5, Bob ACTIVE_START L6,
    // Bob CCR L7) preceding the one popped at L9.
    //
    // The actual emission of these anchors is interleaved with notes /
    // loops in the merge loop below — upstream walks the signal stream
    // in order and `anchorElement` / `drawNote` / `drawLoop` each
    // append to `diagram` at the moment they run, so DOM order tracks
    // synthetic-event id (the `data-id="iN"` slot).

    // ── Stickman body groups (Actor type only) ──────────────────────
    //
    // Upstream emits two `<g>`s per top Actor (lifeline `g.lower()` and
    // body `g`); the body comes AFTER `<defs>` since it is appended
    // last. For bottom (footer), the lifeline `<g>` is empty and the
    // body comes at the very end (after messages).
    //
    // The torso/arms ids look like `actor-man-torsoN`/`actor-man-armsN`
    // where the raw `N = actorCnt` snapshot at emit time. Upstream
    // post-processing renumbers each `actor-man-{kind}{N}` per prefix
    // by first DOM appearance, so two ids that share `(kind, N)` end
    // up identical (e.g. top Bob `torso2` and bottom Alice `torso2`
    // when N=2 for both).
    let n_actors_total = actors.len();
    let stick_ids = compute_stick_ids(d, n_actors_total);

    // Loop / control-structure blocks AND notes — both kinds land in
    // the diagram tree BEFORE the messagesToDraw flush, but their
    // relative DOM order is the order they were appended during the
    // walk. Upstream `drawLoop` fires at LOOP_END (so the loop lands
    // when its bracketed `]` keyword closes) and `drawNote` fires
    // inline at the note's signal. Both share the per-signal id
    // counter (`data-id="iN"`), and the resulting DOM order matches
    // ascending id. We merge the two lists by `idx` here so that:
    //   • fixture 110 (loop ends at i3, note at i4) → control-struct first
    //   • fixture 19  (note at i4, alt ends at i12) → note first
    // Notes are also emitted BEFORE Actor-type top bodies because in
    // upstream the top `drawActors` runs AFTER the message loop —
    // `actElem = elem.append('g')` (no `.lower()`) for Actor body, so
    // it's appended at the END of `elem.children`, after the notes
    // that were appended during the message loop.
    enum BlockRef<'a> {
        Loop(&'a LoopRender),
        Note(&'a NoteRender),
        Anchor(&'a Option<ActivationRect>),
    }
    let mut blocks: Vec<(usize, usize, BlockRef)> =
        Vec::with_capacity(loops.len() + notes.len() + activation_anchors.len());
    // Secondary key: stable order tag — anchors emit BEFORE
    // co-indexed notes/loops in upstream parser order. Currently no
    // collisions arise (each id-slot is consumed by exactly one
    // synthetic event), but we keep the secondary key for robustness.
    for lr in &loops {
        blocks.push((lr.idx, 1, BlockRef::Loop(lr)));
    }
    for n in &notes {
        blocks.push((n.idx, 1, BlockRef::Note(n)));
    }
    for (anchor_idx, anchor) in &activation_anchors {
        blocks.push((*anchor_idx, 0, BlockRef::Anchor(anchor)));
    }
    blocks.sort_by_key(|&(idx, sec, _)| (idx, sec));
    for (_, _, b) in &blocks {
        match b {
            BlockRef::Loop(lr) => emit_loop(&mut out, lr),
            BlockRef::Note(n) => emit_note(&mut out, n),
            BlockRef::Anchor(anchor) => match anchor {
                Some(rect) => {
                    out.push_str("<g><rect x=\"");
                    push_num(&mut out, rect.x);
                    out.push_str("\" y=\"");
                    push_num(&mut out, rect.y);
                    out.push_str("\" fill=\"#EDF2AE\" stroke=\"#666\" width=\"10\" height=\"");
                    push_num(&mut out, rect.height);
                    out.push_str("\" class=\"activation");
                    out.push_str(&rect.class_n.to_string());
                    out.push_str("\"></rect></g>");
                }
                None => out.push_str("<g></g>"),
            },
        }
    }

    // Top bodies, declaration order, only for Actor / Boundary / Control / Entity.
    // Database is NOT in this loop — its body is fully embedded inside the
    // top group emitted earlier.
    for (i, a) in actors.iter().enumerate() {
        let top_actor_y = a.create_y.unwrap_or(a.starty);
        match a.actor_type {
            ActorType::Actor => {
                let (torso_id, arms_id) = stick_ids.top[i];
                emit_actor_man_body(&mut out, a, top_actor_y, false, torso_id, arms_id);
            }
            ActorType::Boundary => {
                let (torso_id, arms_id) = stick_ids.top[i];
                emit_actor_boundary_body(&mut out, a, top_actor_y, false, torso_id, arms_id);
            }
            ActorType::Control => {
                emit_actor_control_body(&mut out, a, top_actor_y, false, id);
            }
            ActorType::Entity => {
                emit_actor_entity_body(&mut out, a, top_actor_y, false);
            }
            _ => continue,
        }
    }

    // Messages — text + line for each, in declaration order.
    for m in &messages {
        emit_message(&mut out, id, m);
    }

    // Bottom bodies, declaration order, only for Actor / Boundary /
    // Control / Entity — and only when mirroring. Database is NOT in this
    // loop — its body is fully embedded inside the bottom group.
    if mirror {
        for (i, a) in actors.iter().enumerate() {
            let actor_bottom_y = a.destroy_y.unwrap_or(bottom_y);
            match a.actor_type {
                ActorType::Actor => {
                    let (torso_id, arms_id) = stick_ids.bottom[i];
                    emit_actor_man_body(&mut out, a, actor_bottom_y, true, torso_id, arms_id);
                }
                ActorType::Boundary => {
                    let (torso_id, arms_id) = stick_ids.bottom[i];
                    emit_actor_boundary_body(&mut out, a, actor_bottom_y, true, torso_id, arms_id);
                }
                ActorType::Control => {
                    emit_actor_control_body(&mut out, a, actor_bottom_y, true, id);
                }
                ActorType::Entity => {
                    emit_actor_entity_body(&mut out, a, actor_bottom_y, true);
                }
                _ => continue,
            }
        }
    }

    // Popup menus — one `<g id="actorN_popup">` per actor with links (or
    // for every actor when `forceMenus` is set). Walks DECLARATION order
    // (not the reversed top-group order), so the ranked id `actorN_popup`
    // is `(n_actors - 1 - decl_index)` to keep alignment with the actor
    // line ids emitted by the reversed top loop above.
    let force_menus = d.config.force_menus;
    let n_actors = actors.len();
    for (i, a) in actors.iter().enumerate() {
        if a.links.is_empty() && !force_menus {
            continue;
        }
        let rank = n_actors - 1 - i;
        emit_actor_popup(&mut out, a, rank, force_menus, mirror);
    }

    // Frontmatter title bar — emitted *after* every other element so it
    // sits at the end of the DOM. Upstream sequenceRenderer.draw():
    //   diagram.append("text").text(title)
    //          .attr("x", (box.stopx - box.startx) / 2 - 2 * diagramMarginX)
    //          .attr("y", -25);
    // No class / no font attrs — naked `<text x=… y="-25">…</text>`.
    if has_title {
        let title = d.title.as_deref().unwrap_or("");
        let title_x = box_width / 2.0 - 2.0 * dia_margin_x;
        out.push_str("<text x=\"");
        push_num(&mut out, title_x);
        out.push_str("\" y=\"-25\">");
        out.push_str(&xml_escape(title));
        out.push_str("</text>");
    }

    out.push_str("</svg>");
    Ok(out)
}

/// Stickman id mapping table: for each actor (by decl index),
/// the (torso, arms) ids that should appear in its top and bottom
/// `actor-man` body groups, after normalisation.
struct StickIds {
    top: Vec<(usize, usize)>,
    bottom: Vec<(usize, usize)>,
}

fn compute_stick_ids(d: &SequenceDiagram, n_actors_total: usize) -> StickIds {
    // Upstream `actorCnt` increments once per actor (for top emission,
    // both Participant and Actor). Bottom emission does NOT increment.
    // So raw_n[i] = i + 1 for top, and raw_n_bottom = n_actors_total
    // (the value left over after the last top emission).
    let mut top = vec![(0usize, 0usize); d.actors.len()];
    let mut bottom = vec![(0usize, 0usize); d.actors.len()];

    // Build first-appearance map per (kind, raw_n). DOM body order is:
    // top bodies (decl order, Actor only), then bottom bodies (decl
    // order, Actor only).
    let mut torso_map: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();
    let mut arms_map: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();
    let mut next_torso = 0usize;
    let mut next_arms = 0usize;
    let mut next_global = |next: &mut usize| -> usize {
        let v = *next;
        *next += 2; // each (torso/arms) pair occupies 2 slots in the global counter
        v
    };
    // Actually mermaid normalisation increments PER PREFIX independently
    // (one global counter for `actor-man-torso`, another for
    // `actor-man-arms`), but they advance in DOM order interleaved. Re-
    // read `renumberCounterIds`: a single map keyed by `(prefix:N)`,
    // counter `next` shared across prefixes within the same `replace`
    // pass. Since the regex matches both `actor-man-torsoN` AND
    // `actor-man-armsN` with the same pattern (different captured `id`
    // string), and the closure increments `next` for each NEW key,
    // torso/arms COUNT TOGETHER. So pair (torso first then arms) on
    // first appearance maps to 0, 1; second pair → 2, 3; etc.
    let _ = (next_torso, next_arms, &mut next_global);
    let mut next = 0usize;
    let mut take = |map: &mut std::collections::HashMap<usize, usize>,
                    next: &mut usize,
                    raw_n: usize|
     -> usize {
        if let Some(&v) = map.get(&raw_n) {
            v
        } else {
            let v = *next;
            map.insert(raw_n, v);
            *next += 1;
            v
        }
    };

    // Walk top bodies in decl order. Both Actor and Boundary increment
    // `actorCnt` upstream and emit body groups with `actor-man-{torso,arms}N`
    // ids, so they share the same numbering pool.
    for (i, a) in d.actors.iter().enumerate() {
        if !matches!(a.actor_type, ActorType::Actor | ActorType::Boundary) {
            continue;
        }
        let raw_n = i + 1;
        let t = take(&mut torso_map, &mut next, raw_n);
        let r = take(&mut arms_map, &mut next, raw_n);
        top[i] = (t, r);
    }
    // Walk bottom bodies in decl order
    for (i, a) in d.actors.iter().enumerate() {
        if !matches!(a.actor_type, ActorType::Actor | ActorType::Boundary) {
            continue;
        }
        let raw_n = n_actors_total;
        let t = take(&mut torso_map, &mut next, raw_n);
        let r = take(&mut arms_map, &mut next, raw_n);
        bottom[i] = (t, r);
    }

    StickIds { top, bottom }
}

/// Emit one `<g class="actor-man actor-{top,bottom}" ...>` body group
/// for a Boundary-type actor. Differs from `emit_actor_man_body`:
/// - horizontal torso line (left half), vertical arms line at left edge;
/// - no legs;
/// - circle radius 22 (vs 15);
/// - outer `<g>` carries `transform="translate(0, 21)"`
///   (= radius/2 + 10 = 21);
/// - top group's `data-{et,type,id}` attrs come AFTER `name=` and AFTER
///   `transform=` (mirrors upstream svgDraw `actElem.attr` order).
/// Mirrors upstream `drawActorTypeBoundary` (mermaid.js line 137422).
fn emit_actor_boundary_body(
    out: &mut String,
    a: &ActorRender,
    actor_y: f64,
    is_footer: bool,
    torso_id: usize,
    arms_id: usize,
) {
    const RADIUS: f64 = 22.0;
    let center = a.x + a.width / 2.0;

    // Outer <g>. Top groups carry data-* AFTER transform; bottom groups omit
    // data-* entirely. Both have `name=` and `style=`. The transform is
    // `translate(0, radius/2 + 10)` = `translate(0, 21)`.
    out.push_str("<g class=\"actor-man ");
    out.push_str(if is_footer {
        "actor-bottom"
    } else {
        "actor-top"
    });
    out.push_str("\" name=\"");
    out.push_str(&xml_escape(&a.id));
    out.push_str("\" style=\"stroke: #9370DB;\" transform=\"translate(0,");
    push_num(out, RADIUS / 2.0 + 10.0);
    out.push_str(")\"");
    if !is_footer {
        out.push_str(" data-et=\"participant\" data-type=\"boundary\" data-id=\"");
        out.push_str(&xml_escape(&a.id));
        out.push('"');
    }
    out.push('>');

    // torso (horizontal): x1 = center - r*2.5, y1 = actorY+12,
    //                     x2 = center - 15,    y2 = actorY+12
    out.push_str("<line id=\"actor-man-torso");
    out.push_str(&torso_id.to_string());
    out.push_str("\" x1=\"");
    push_num(out, center - RADIUS * 2.5);
    out.push_str("\" y1=\"");
    push_num(out, actor_y + 12.0);
    out.push_str("\" x2=\"");
    push_num(out, center - 15.0);
    out.push_str("\" y2=\"");
    push_num(out, actor_y + 12.0);
    out.push_str("\"></line>");

    // arms (vertical): x1 = center - r*2.5, y1 = actorY+2,
    //                  x2 = center - r*2.5, y2 = actorY+22
    out.push_str("<line id=\"actor-man-arms");
    out.push_str(&arms_id.to_string());
    out.push_str("\" x1=\"");
    push_num(out, center - RADIUS * 2.5);
    out.push_str("\" y1=\"");
    push_num(out, actor_y + 2.0);
    out.push_str("\" x2=\"");
    push_num(out, center - RADIUS * 2.5);
    out.push_str("\" y2=\"");
    push_num(out, actor_y + 22.0);
    out.push_str("\"></line>");

    // circle: cx=center, cy=actorY+12, r=22
    out.push_str("<circle cx=\"");
    push_num(out, center);
    out.push_str("\" cy=\"");
    push_num(out, actor_y + 12.0);
    out.push_str("\" r=\"");
    push_num(out, RADIUS);
    out.push_str("\"></circle>");

    // Text: byTspan with x6=actor.x, y6=actorY+15, width=actor.width,
    // height=actor.height. The byTspan formula puts text y at
    // y6 + height/2 = actorY + 15 + actor.height/2.
    //
    // Subtle: upstream MUTATES `actor.height` post-bbox at the END of the
    // top-pass `drawActorTypeBoundary` call to `bbox.height + labelBoxHeight
    // = 44 + 20 = 64`. The footer pass then sees `actor.height = 64`
    // (instead of the initial 65) and uses it for `rect3.height` in the
    // text call. So top text uses height=65 (initial), bottom uses 64.
    // For boundary the bbox is always the same shape (44×77), so the
    // post-bbox value is always 64 — no per-actor variance.
    let text_height = if is_footer { 64.0 } else { a.height };
    let text_y = actor_y + 15.0 + text_height / 2.0;
    let cx = center;
    let lines = split_br(&a.description);
    let n_lines = lines.len();
    let font_size = 16.0_f64;
    for (i, line) in lines.iter().enumerate() {
        let dy = (i as f64) * font_size - font_size * ((n_lines as f64) - 1.0) / 2.0;
        out.push_str("<text x=\"");
        push_num(out, cx);
        out.push_str("\" y=\"");
        push_num(out, text_y);
        out.push_str(
            "\" style=\"text-anchor: middle; font-size: 16px; font-weight: 400; font-family: ",
        );
        out.push_str(&attr_escape(ACTOR_FONT_FAMILY));
        out.push_str(
            ";\" dominant-baseline=\"central\" alignment-baseline=\"central\" class=\"actor actor-man\"><tspan x=\"",
        );
        push_num(out, cx);
        out.push_str("\" dy=\"");
        push_num(out, dy);
        out.push_str("\">");
        out.push_str(&xml_escape(line));
        out.push_str("</tspan></text>");
    }
    out.push_str("</g>");
}

/// Emit one `<g class="actor-man actor-{top,bottom}" ...>` body group
/// for a Control-type actor. Differs from Boundary:
/// - body contains `<defs><marker .../></defs>` + `<circle r=22>` +
///   `<line marker-end="url(...)" transform="translate(cx, cy-r)">` + text;
/// - top group attribute order is `name, style, data-et, data-type, data-id`
///   (style precedes data-*, unlike Boundary which has style AFTER name and
///   data-* AFTER transform); style includes `fill: #ECECFF;`;
/// - bottom group lacks data-* but keeps the same style;
/// - centerY for the lifeline is `actor_y + 75` (set in caller).
///
/// Geometry: cx = actor.x + actor.width/2, cy = actor_y + 32, r = 22.
/// The `<line>` arrow has no x/y attrs — only `transform=translate(cx, cy-r)`,
/// i.e. translate(cx, actor_y+10).
///
/// Text positioning: y6 = rect.y + r + (top ? 12 : 5), then y = y6 + height/2.
/// Upstream mutates `actor.height = bbox.height + 2*labelBoxHeight = 44 + 40 = 84`
/// at the end of the top-pass, so the bottom-pass sees height=84 in `rect3.height`.
/// Top-pass uses the initial actor.height (e.g. 65 by default).
///
/// Mirrors upstream `drawActorTypeControl` (mermaid.js line 137206).
fn emit_actor_control_body(
    out: &mut String,
    a: &ActorRender,
    actor_y: f64,
    is_footer: bool,
    diagram_id: &str,
) {
    const RADIUS: f64 = 22.0;
    let center = a.x + a.width / 2.0;
    let cy = actor_y + 32.0;

    out.push_str("<g class=\"actor-man ");
    out.push_str(if is_footer {
        "actor-bottom"
    } else {
        "actor-top"
    });
    out.push_str("\" name=\"");
    out.push_str(&xml_escape(&a.id));
    out.push_str("\" style=\"stroke: #9370DB; fill: #ECECFF;\"");
    if !is_footer {
        out.push_str(" data-et=\"participant\" data-type=\"control\" data-id=\"");
        out.push_str(&xml_escape(&a.id));
        out.push('"');
    }
    out.push('>');

    // <defs><marker id="{id}-filled-head-control" .../></defs>
    out.push_str("<defs><marker id=\"");
    out.push_str(diagram_id);
    out.push_str(
        "-filled-head-control\" refX=\"11\" refY=\"5.8\" markerWidth=\"20\" markerHeight=\"28\" orient=\"172.5\" stroke-width=\"1.2\"><path d=\"M 14.4 5.6 L 7.2 10.4 L 8.8 5.6 L 7.2 0.8 Z\"></path></marker></defs>",
    );

    // <circle cx=center cy=actor_y+32 r=22 filter="">
    out.push_str("<circle cx=\"");
    push_num(out, center);
    out.push_str("\" cy=\"");
    push_num(out, cy);
    out.push_str("\" r=\"");
    push_num(out, RADIUS);
    out.push_str("\" filter=\"\"></circle>");

    // <line marker-end="url(#...-filled-head-control)" transform="translate(cx, cy-r)">
    out.push_str("<line marker-end=\"url(#");
    out.push_str(diagram_id);
    out.push_str("-filled-head-control)\" transform=\"translate(");
    push_num(out, center);
    out.push_str(", ");
    push_num(out, cy - RADIUS);
    out.push_str(")\"></line>");

    // Text. y6 = rect.y + r + (top ? 12 : 5). height: top uses initial
    // actor.height; bottom uses the post-bbox value 84.
    let y6_offset = if is_footer { 5.0 } else { 12.0 };
    let text_height = if is_footer { 84.0 } else { a.height };
    let text_y = actor_y + RADIUS + y6_offset + text_height / 2.0;
    let lines = split_br(&a.description);
    let n_lines = lines.len();
    let font_size = 16.0_f64;
    for (i, line) in lines.iter().enumerate() {
        let dy = (i as f64) * font_size - font_size * ((n_lines as f64) - 1.0) / 2.0;
        out.push_str("<text x=\"");
        push_num(out, center);
        out.push_str("\" y=\"");
        push_num(out, text_y);
        out.push_str(
            "\" style=\"text-anchor: middle; font-size: 16px; font-weight: 400; font-family: ",
        );
        out.push_str(&attr_escape(ACTOR_FONT_FAMILY));
        out.push_str(
            ";\" dominant-baseline=\"central\" alignment-baseline=\"central\" class=\"actor actor-man\"><tspan x=\"",
        );
        push_num(out, center);
        out.push_str("\" dy=\"");
        push_num(out, dy);
        out.push_str("\">");
        out.push_str(&xml_escape(line));
        out.push_str("</tspan></text>");
    }
    out.push_str("</g>");
}

/// Emit one `<g class="actor actor-{top,bottom}" ...>` body group for an
/// Entity-type actor — a circle of radius 22 with a horizontal underline,
/// plus the description text. Differs from Boundary/Control:
/// - class is `actor` (not `actor-man`);
/// - no explicit `style` attribute on the outer <g> (default theme stroke
///   comes from CSS `.actor` rule);
/// - top transform `translate(0, r/2-5)` = `translate(0, 6)`;
/// - bottom transform `translate(0, r)` = `translate(0, 22)`;
/// - circle cy = actor_y + (top ? 25 : 10);
/// - underline: x1 = cx-r, x2 = cx+r, y1 = y2 = cy + r, stroke-width=2;
/// - text uses class `actor actor-man` (matches upstream's
///   `ACTOR_MAN_FIGURE_CLASS` arg in the textAttrs dict — not `actor-box`).
/// - top-pass post-bbox `actor.height` mutation = bbox.height + 20 = 64;
/// - bottom text uses height=64 (post-bbox), top uses initial actor.height.
///
/// Mirrors upstream `drawActorTypeEntity` (mermaid.js line 137267).
fn emit_actor_entity_body(out: &mut String, a: &ActorRender, actor_y: f64, is_footer: bool) {
    const RADIUS: f64 = 22.0;
    let center = a.x + a.width / 2.0;
    let cy = actor_y + if is_footer { 10.0 } else { 25.0 };

    // Outer <g>. Attribute order: class, name, transform, [data-* if top].
    out.push_str("<g class=\"actor ");
    out.push_str(if is_footer { "actor-bottom" } else { "actor-top" });
    out.push_str("\" name=\"");
    out.push_str(&xml_escape(&a.id));
    out.push_str("\" transform=\"translate(0, ");
    push_num(out, if is_footer { RADIUS } else { RADIUS / 2.0 - 5.0 });
    out.push_str(")\"");
    if !is_footer {
        out.push_str(" data-et=\"participant\" data-type=\"entity\" data-id=\"");
        out.push_str(&xml_escape(&a.id));
        out.push('"');
    }
    out.push('>');

    // <circle cx=center cy=cy r=22 width=actor.width height=actor.height>
    // upstream uses `actor.height` BEFORE the post-bbox mutation for both
    // top (initial value) and bottom (mutated value 64 by the time bottom-pass
    // runs).
    let circle_h = if is_footer { 64.0 } else { a.height };
    out.push_str("<circle cx=\"");
    push_num(out, center);
    out.push_str("\" cy=\"");
    push_num(out, cy);
    out.push_str("\" r=\"");
    push_num(out, RADIUS);
    out.push_str("\" width=\"");
    push_num(out, a.width);
    out.push_str("\" height=\"");
    push_num(out, circle_h);
    out.push_str("\"></circle>");

    // <line x1=cx-r x2=cx+r y1=cy+r y2=cy+r stroke-width=2>
    // upstream attribute order: x1, x2, y1, y2, stroke-width.
    out.push_str("<line x1=\"");
    push_num(out, center - RADIUS);
    out.push_str("\" x2=\"");
    push_num(out, center + RADIUS);
    out.push_str("\" y1=\"");
    push_num(out, cy + RADIUS);
    out.push_str("\" y2=\"");
    push_num(out, cy + RADIUS);
    out.push_str("\" stroke-width=\"2\"></line>");

    // Text. y6 = rect.y + (top ? 30 : 15); y = y6 + height/2.
    // height: top uses initial actor.height (e.g. 65); bottom uses post-bbox 64.
    let y6_offset = if is_footer { 15.0 } else { 30.0 };
    let text_height = if is_footer { 64.0 } else { a.height };
    let text_y = actor_y + y6_offset + text_height / 2.0;
    let lines = split_br(&a.description);
    let n_lines = lines.len();
    let font_size = 16.0_f64;
    for (i, line) in lines.iter().enumerate() {
        let dy = (i as f64) * font_size - font_size * ((n_lines as f64) - 1.0) / 2.0;
        out.push_str("<text x=\"");
        push_num(out, center);
        out.push_str("\" y=\"");
        push_num(out, text_y);
        out.push_str(
            "\" style=\"text-anchor: middle; font-size: 16px; font-weight: 400; font-family: ",
        );
        out.push_str(&attr_escape(ACTOR_FONT_FAMILY));
        out.push_str(
            ";\" dominant-baseline=\"central\" alignment-baseline=\"central\" class=\"actor actor-man\"><tspan x=\"",
        );
        push_num(out, center);
        out.push_str("\" dy=\"");
        push_num(out, dy);
        out.push_str("\">");
        out.push_str(&xml_escape(line));
        out.push_str("</tspan></text>");
    }
    out.push_str("</g>");
}

/// Emit the FULL bottom group for a Database-type actor — a single outer
/// `<g>` containing the cylinder body + description text. Unlike the other
/// non-participant types, database doesn't emit an empty `<g></g>`
/// placeholder followed by a body group later; everything goes here.
///
/// Mirrors upstream `drawActorTypeDatabase` (mermaid.js line 137330) when
/// `isFooter=true`: `boxplusLineGroup = elem.append("g").lower()`, no
/// lifeline (skipped via `if (!isFooter)`), `g2 = boxplusLineGroup`,
/// `cylinderGroup = g2.append("g")` with class `actor actor-bottom`,
/// path with the cylinder shape, transform=`translate(w4, ry)`, then
/// text appended to `g2`.
fn emit_actor_database_bottom_group(out: &mut String, a: &ActorRender, bottom_y: f64) {
    let center = a.x + a.width / 2.0;
    // Cylinder geometry: w4 = h3 = actor.width / 3. Square aspect for the
    // cylinder body, regardless of actor.width. rx = w4/2 (ellipse x-radius
    // matches half-width). ry = rx / (2.5 + w4/50) (squashed ellipse — the
    // formula skinnies the lid as actors get wider).
    let w4 = a.width / 3.0;
    let h3 = w4;
    let rx = w4 / 2.0;
    let ry = rx / (2.5 + w4 / 50.0);
    let body_h = h3 - 2.0 * ry;

    // Outer <g> (boxplusLineGroup, no attrs)
    out.push_str("<g>");

    // Cylinder <g class="actor actor-bottom" style="stroke: #9370DB;" transform="translate(w4,ry)">
    // Note: style is set via `cylinderGroup.style("stroke", actorBorder)`
    // (default theme classic). attribute order: class, style, transform.
    out.push_str("<g class=\"actor actor-bottom\" style=\"stroke: #9370DB;\" transform=\"translate(");
    push_num(out, w4);
    out.push_str(", ");
    push_num(out, ry);
    out.push_str(")\"><path d=\"\n  M ");
    push_num(out, a.x);
    out.push(',');
    push_num(out, bottom_y + ry);
    out.push_str("\n  a ");
    push_num(out, rx);
    out.push(',');
    push_num(out, ry);
    out.push_str(" 0 0 0 ");
    push_num(out, w4);
    out.push_str(",0\n  a ");
    push_num(out, rx);
    out.push(',');
    push_num(out, ry);
    out.push_str(" 0 0 0 -");
    push_num(out, w4);
    out.push_str(",0\n  l 0,");
    push_num(out, body_h);
    out.push_str("\n  a ");
    push_num(out, rx);
    out.push(',');
    push_num(out, ry);
    out.push_str(" 0 0 0 ");
    push_num(out, w4);
    out.push_str(",0\n  l 0,-");
    push_num(out, body_h);
    out.push_str("\n\"></path></g>");

    // Text inside g2 (= boxplusLineGroup). y6 = rect.y + 35; height =
    // post-bbox actor.height = (h3 - 2*ry) + labelBoxHeight = body_h + 20.
    // Note this is the BOTTOM-pass call — top-pass uses initial actor.height.
    let text_height = body_h + 20.0;
    let text_y = bottom_y + 35.0 + text_height / 2.0;
    emit_actor_box_text(out, center, text_y, &a.description);

    out.push_str("</g>");
}

/// Emit the FULL top group for a Database-type actor — single outer `<g>`
/// containing: lifeline, `<g id="root-N" data-et=...>` wrapper which
/// contains the cylinder body, and finally the description text inside the
/// root-N wrapper.
///
/// Mirrors upstream `drawActorTypeDatabase` (mermaid.js line 137330) for
/// `isFooter=false`. centerY for the lifeline = `actor_y + actor.height +
/// 2 * boxTextMargin` = 0 + 65 + 10 = 75 (uses INITIAL actor.height —
/// before post-bbox mutation).
fn emit_actor_database_top_group(
    out: &mut String,
    a: &ActorRender,
    bottom_y: f64,
    rank: usize,
    db_index: usize,
    popup: bool,
) {
    let center = a.x + a.width / 2.0;
    let w4 = a.width / 3.0;
    let h3 = w4;
    let rx = w4 / 2.0;
    let ry = rx / (2.5 + w4 / 50.0);
    let body_h = h3 - 2.0 * ry;
    // Lifeline centerY = actor_y + actor.height + 2 * boxTextMargin
    // = 0 + a.height + 10. For default config a.height=65 → centery=75.
    let centery = a.starty + a.height + 10.0;

    // Outer <g>: popup (`onclick`) wrapper if links present; plain otherwise.
    if popup {
        push_popup_g_open(out, rank);
    } else {
        out.push_str("<g>");
    }
    // Lifeline <line id="actorN" .../>
    out.push_str("<line id=\"actor");
    out.push_str(&rank.to_string());
    out.push_str("\" x1=\"");
    push_num(out, center);
    out.push_str("\" y1=\"");
    push_num(out, centery);
    out.push_str("\" x2=\"");
    push_num(out, center);
    out.push_str("\" y2=\"");
    push_num(out, bottom_y);
    out.push_str(
        "\" class=\"actor-line 200\" stroke-width=\"0.5px\" stroke=\"#999\" name=\"",
    );
    out.push_str(&xml_escape(&a.id));
    out.push_str("\" data-et=\"life-line\" data-id=\"");
    out.push_str(&xml_escape(&a.id));
    out.push_str("\"></line>");

    // <g id="root-N" data-et="participant" data-type="database" data-id="X">
    // root-N counter is renumbered by ref normaliser per DOM appearance, so
    // we use a separate `db_index` rather than `rank`.
    out.push_str("<g id=\"root-");
    out.push_str(&db_index.to_string());
    out.push_str("\" data-et=\"participant\" data-type=\"database\" data-id=\"");
    out.push_str(&xml_escape(&a.id));
    out.push_str("\">");

    // Cylinder
    out.push_str("<g class=\"actor actor-top\" style=\"stroke: #9370DB;\" transform=\"translate(");
    push_num(out, w4);
    out.push_str(", ");
    push_num(out, ry);
    out.push_str(")\"><path d=\"\n  M ");
    push_num(out, a.x);
    out.push(',');
    push_num(out, a.starty + ry);
    out.push_str("\n  a ");
    push_num(out, rx);
    out.push(',');
    push_num(out, ry);
    out.push_str(" 0 0 0 ");
    push_num(out, w4);
    out.push_str(",0\n  a ");
    push_num(out, rx);
    out.push(',');
    push_num(out, ry);
    out.push_str(" 0 0 0 -");
    push_num(out, w4);
    out.push_str(",0\n  l 0,");
    push_num(out, body_h);
    out.push_str("\n  a ");
    push_num(out, rx);
    out.push(',');
    push_num(out, ry);
    out.push_str(" 0 0 0 ");
    push_num(out, w4);
    out.push_str(",0\n  l 0,-");
    push_num(out, body_h);
    out.push_str("\n\"></path></g>");

    // Text inside g2 (the root-N <g>). Top-pass: uses INITIAL actor.height.
    let text_y = a.starty + 35.0 + a.height / 2.0;
    emit_actor_box_text(out, center, text_y, &a.description);

    // Close root-N and outer <g>.
    out.push_str("</g></g>");
}

/// Emit the body shape of a Queue-type actor — two `<g>` wrappers, each
/// containing one `<path>`. The first path is the closed cylinder side
/// (left semi-arc + top + right semi-arc + bottom + Z). The second path
/// is the right-side arc only (foreground arc that sits in front of the
/// closed shape).
///
/// Geometry (mirrors upstream `drawActorTypeQueue` lines 642-664):
///   ry = rect.height / 2
///   rx = ry / (2.5 + rect.height / 50)
///   first  <g transform="translate(rx, -ry)">   path closed shape
///   second <g transform="translate(width-rx, -ry)">  right arc only
///   first  path:  M rect.x,rect.y+ry
///                 a rx,ry 0 0 0 0,height
///                 h width-2rx
///                 a rx,ry 0 0 0 0,-height
///                 Z
///   second path:  M rect.x,rect.y+ry
///                 a rx,ry 0 0 0 0,height
///
/// The path templates use literal newlines (4 spaces of indent for the
/// closed shape and 6 spaces for the arc — these come from the upstream
/// JS template literals' source indentation).
fn emit_actor_queue_body_paths(out: &mut String, a: &ActorRender, actor_y: f64) {
    let ry = a.height / 2.0;
    let rx = ry / (2.5 + a.height / 50.0);
    let body_h = a.height;
    let h_seg = a.width - 2.0 * rx;

    // First <g transform="translate(rx, -ry)"><path d="M x,y+ry a rx,ry 0 0 0 0,h h ... a ... Z\n  ">
    out.push_str("<g transform=\"translate(");
    push_num(out, rx);
    out.push_str(", -");
    push_num(out, ry);
    out.push_str(")\"><path d=\"M ");
    push_num(out, a.x);
    out.push(',');
    push_num(out, actor_y + ry);
    out.push_str("\n    a ");
    push_num(out, rx);
    out.push(',');
    push_num(out, ry);
    out.push_str(" 0 0 0 0,");
    push_num(out, body_h);
    out.push_str("\n    h ");
    push_num(out, h_seg);
    out.push_str("\n    a ");
    push_num(out, rx);
    out.push(',');
    push_num(out, ry);
    out.push_str(" 0 0 0 0,-");
    push_num(out, body_h);
    out.push_str("\n    Z\n  \"></path></g>");

    // Second <g transform="translate(width-rx, -ry)"><path d="M x,y+ry\n      a rx,ry 0 0 0 0,h">
    out.push_str("<g transform=\"translate(");
    push_num(out, a.width - rx);
    out.push_str(", -");
    push_num(out, ry);
    out.push_str(")\"><path d=\"M ");
    push_num(out, a.x);
    out.push(',');
    push_num(out, actor_y + ry);
    out.push_str("\n      a ");
    push_num(out, rx);
    out.push(',');
    push_num(out, ry);
    out.push_str(" 0 0 0 0,");
    push_num(out, body_h);
    out.push_str("\"></path></g>");
}

/// Emit the FULL bottom group for a Queue-type actor — single outer
/// `<g class="actor actor-bottom">` containing the two queue body paths
/// + description text. Mirrors upstream `drawActorTypeQueue` (line 581)
/// when `isFooter=true`: `boxplusLineGroup = elem.append("g").lower()`,
/// `g.attr('class', cssclass)` where cssclass is `'actor actor-bottom'`,
/// then path emission, then text.
fn emit_actor_queue_bottom_group(out: &mut String, a: &ActorRender, bottom_y: f64) {
    out.push_str("<g class=\"actor actor-bottom\">");
    emit_actor_queue_body_paths(out, a, bottom_y);
    let cx = a.x + a.width / 2.0;
    let cy = bottom_y + a.height / 2.0;
    emit_actor_box_text(out, cx, cy, &a.description);
    out.push_str("</g>");
}

/// Emit the FULL top group for a Queue-type actor — outer plain `<g>`
/// (or popup-onclick wrapper) containing: lifeline `<line id="actorN">`,
/// then `<g id="root-N" class="actor actor-top" data-et="participant"
/// data-type="queue" data-id="X">` containing the two queue body paths
/// + description text.
///
/// Mirrors upstream `drawActorTypeQueue` (line 581) when `isFooter=false`.
/// centerY for the lifeline = `actor_y + actor.height` = 0 + a.height
/// (= 65 for default conf).
fn emit_actor_queue_top_group(
    out: &mut String,
    a: &ActorRender,
    bottom_y: f64,
    rank: usize,
    root_index: usize,
    popup: bool,
) {
    let center = a.x + a.width / 2.0;
    let centery = a.starty + a.height; // actor_y=0 + actor.height

    if popup {
        push_popup_g_open(out, rank);
    } else {
        out.push_str("<g>");
    }
    // Lifeline
    out.push_str("<line id=\"actor");
    out.push_str(&rank.to_string());
    out.push_str("\" x1=\"");
    push_num(out, center);
    out.push_str("\" y1=\"");
    push_num(out, centery);
    out.push_str("\" x2=\"");
    push_num(out, center);
    out.push_str("\" y2=\"");
    push_num(out, bottom_y);
    out.push_str(
        "\" class=\"actor-line 200\" stroke-width=\"0.5px\" stroke=\"#999\" name=\"",
    );
    out.push_str(&xml_escape(&a.id));
    out.push_str("\" data-et=\"life-line\" data-id=\"");
    out.push_str(&xml_escape(&a.id));
    out.push_str("\"></line>");

    // <g id="root-N" class="actor actor-top" data-et=... data-type="queue" data-id=X>
    out.push_str("<g id=\"root-");
    out.push_str(&root_index.to_string());
    out.push_str("\" class=\"actor actor-top\" data-et=\"participant\" data-type=\"queue\" data-id=\"");
    out.push_str(&xml_escape(&a.id));
    out.push_str("\">");

    // Body paths
    emit_actor_queue_body_paths(out, a, a.starty);

    // Text inside root-N. cy = actor_y + height/2.
    let cy = a.starty + a.height / 2.0;
    emit_actor_box_text(out, center, cy, &a.description);

    // Close root-N and outer <g>.
    out.push_str("</g></g>");
}

/// Emit the body shape of a Collections-type actor — two stacked `<rect>`
/// elements: a main rect (with `actor actor-top`/`actor actor-bottom` class)
/// and a shadow rect offset by 6 (top: `+6,+6`, bottom: `-6,+6`) with class
/// `actor` only. Order: main first, shadow second (drawn on top).
///
/// Mirrors upstream `drawActorTypeCollections` lines 502-530:
///   const offset = 6
///   shadowRect.x = rect.x + (isFooter ? -offset : -offset)  [always -offset]
///   shadowRect.y = rect.y + (isFooter ? +offset : +offset)  [always +offset]
fn emit_actor_collections_body_rects(
    out: &mut String,
    a: &ActorRender,
    actor_y: f64,
    is_footer: bool,
) {
    let cls_main = if is_footer {
        "actor actor-bottom"
    } else {
        "actor actor-top"
    };
    // Main rect (on top per upstream order: drawRect(g, rect) first, then shadow)
    out.push_str("<rect x=\"");
    push_num(out, a.x);
    out.push_str("\" y=\"");
    push_num(out, actor_y);
    out.push_str("\" fill=\"#eaeaea\" stroke=\"#666\" width=\"");
    push_num(out, a.width);
    out.push_str("\" height=\"");
    push_num(out, a.height);
    out.push_str("\" name=\"");
    out.push_str(&xml_escape(&a.id));
    out.push_str("\" class=\"");
    out.push_str(cls_main);
    out.push_str("\"></rect>");
    // Shadow rect: offset (-6, +6) relative to main, class="actor"
    out.push_str("<rect x=\"");
    push_num(out, a.x - 6.0);
    out.push_str("\" y=\"");
    push_num(out, actor_y + 6.0);
    out.push_str("\" fill=\"#eaeaea\" stroke=\"#666\" width=\"");
    push_num(out, a.width);
    out.push_str("\" height=\"");
    push_num(out, a.height);
    out.push_str("\" name=\"");
    out.push_str(&xml_escape(&a.id));
    out.push_str("\" class=\"actor\"></rect>");
}

/// Emit the FULL bottom group for a Collections-type actor — single outer
/// plain `<g>` containing the two stacked rectangles + description text.
///
/// Mirrors upstream `drawActorTypeCollections` (line 463) when
/// `isFooter=true`: outer `<g>` (lowered) without `class` attribute (the
/// class lands on the rects, not the wrapper). Text is positioned at
/// `(rect.x - offset + width/2, rect.y + offset + height/2)`
/// = `(a.x + width/2 - 6, bottom_y + 6 + height/2)`.
fn emit_actor_collections_bottom_group(out: &mut String, a: &ActorRender, bottom_y: f64) {
    out.push_str("<g>");
    emit_actor_collections_body_rects(out, a, bottom_y, true);
    let cx = a.x + a.width / 2.0 - 6.0;
    let cy = bottom_y + 6.0 + a.height / 2.0;
    emit_actor_box_text(out, cx, cy, &a.description);
    out.push_str("</g>");
}

/// Emit the FULL top group for a Collections-type actor — outer `<g>`
/// (or popup-onclick wrapper) containing the lifeline, then a sibling
/// `<g id="root-N" data-et="participant" data-type="collections"
/// data-id="X">` containing the two stacked rectangles + description text.
///
/// Mirrors upstream `drawActorTypeCollections` (line 463) when
/// `isFooter=false`. centerY for the lifeline = `actor_y + actor.height`
/// (= 65 for default conf). Note the wrapper `<g id="root-N">` does NOT
/// carry a `class` attribute — the class is set only on the rects (line 518
/// upstream sets `rect.class = cssclass`, not `g.attr('class', ...)`).
fn emit_actor_collections_top_group(
    out: &mut String,
    a: &ActorRender,
    bottom_y: f64,
    rank: usize,
    root_index: usize,
    popup: bool,
) {
    let center = a.x + a.width / 2.0;
    let centery = a.starty + a.height;

    if popup {
        push_popup_g_open(out, rank);
    } else {
        out.push_str("<g>");
    }
    // Lifeline
    out.push_str("<line id=\"actor");
    out.push_str(&rank.to_string());
    out.push_str("\" x1=\"");
    push_num(out, center);
    out.push_str("\" y1=\"");
    push_num(out, centery);
    out.push_str("\" x2=\"");
    push_num(out, center);
    out.push_str("\" y2=\"");
    push_num(out, bottom_y);
    out.push_str(
        "\" class=\"actor-line 200\" stroke-width=\"0.5px\" stroke=\"#999\" name=\"",
    );
    out.push_str(&xml_escape(&a.id));
    out.push_str("\" data-et=\"life-line\" data-id=\"");
    out.push_str(&xml_escape(&a.id));
    out.push_str("\"></line>");

    // <g id="root-N" data-et="participant" data-type="collections" data-id=X>
    // (no class attribute, unlike queue and participant)
    out.push_str("<g id=\"root-");
    out.push_str(&root_index.to_string());
    out.push_str("\" data-et=\"participant\" data-type=\"collections\" data-id=\"");
    out.push_str(&xml_escape(&a.id));
    out.push_str("\">");

    // Rects
    emit_actor_collections_body_rects(out, a, a.starty, false);

    // Text inside root-N. cx = center - 6, cy = starty + 6 + height/2.
    let cx = center - 6.0;
    let cy = a.starty + 6.0 + a.height / 2.0;
    emit_actor_box_text(out, cx, cy, &a.description);

    // Close root-N and outer <g>.
    out.push_str("</g></g>");
}

/// Emit one `<g class="actor-man actor-{top,bottom}" ...>` body group
/// (the stickman lines + circle + text). Used for the `actor` keyword.
/// Mirrors upstream `drawActorTypeActor` lines 1181–1268.
fn emit_actor_man_body(
    out: &mut String,
    a: &ActorRender,
    actor_y: f64,
    is_footer: bool,
    torso_id: usize,
    arms_id: usize,
) {
    // Constants from upstream svgDraw.js: ACTOR_TYPE_WIDTH = 36, scale=1
    // (look=classic). adjustedActorY = actorY (no neo offset).
    const ACTOR_TYPE_WIDTH: f64 = 36.0;
    let scale = 1.0;
    let adjusted_actor_y = actor_y;
    let cx = a.x + a.width / 2.0;

    out.push_str("<g class=\"actor-man ");
    out.push_str(if is_footer {
        "actor-bottom"
    } else {
        "actor-top"
    });
    out.push_str("\" name=\"");
    out.push_str(&xml_escape(&a.id));
    if !is_footer {
        out.push_str("\" data-et=\"participant\" data-type=\"actor\" data-id=\"");
        out.push_str(&xml_escape(&a.id));
    }
    out.push_str("\" style=\"stroke: #9370DB;\">");

    // torso line: vertical, from y=adjustedActorY+25 to +45, x=center
    out.push_str("<line id=\"actor-man-torso");
    out.push_str(&torso_id.to_string());
    out.push_str("\" x1=\"");
    push_num(out, cx);
    out.push_str("\" y1=\"");
    push_num(out, adjusted_actor_y + 25.0 * scale);
    out.push_str("\" x2=\"");
    push_num(out, cx);
    out.push_str("\" y2=\"");
    push_num(out, adjusted_actor_y + 45.0 * scale);
    out.push_str("\"></line>");

    // arms: horizontal, y=adjustedActorY+33, from cx-W/2 to cx+W/2
    let half_w = (ACTOR_TYPE_WIDTH / 2.0) * scale;
    out.push_str("<line id=\"actor-man-arms");
    out.push_str(&arms_id.to_string());
    out.push_str("\" x1=\"");
    push_num(out, cx - half_w);
    out.push_str("\" y1=\"");
    push_num(out, adjusted_actor_y + 33.0 * scale);
    out.push_str("\" x2=\"");
    push_num(out, cx + half_w);
    out.push_str("\" y2=\"");
    push_num(out, adjusted_actor_y + 33.0 * scale);
    out.push_str("\"></line>");

    // left leg: from (cx-W/2, adjY+60) to (cx, adjY+45)
    out.push_str("<line x1=\"");
    push_num(out, cx - half_w);
    out.push_str("\" y1=\"");
    push_num(out, adjusted_actor_y + 60.0 * scale);
    out.push_str("\" x2=\"");
    push_num(out, cx);
    out.push_str("\" y2=\"");
    push_num(out, adjusted_actor_y + 45.0 * scale);
    out.push_str("\"></line>");

    // right leg: (cx, adjY+45) to (cx + (W/2 - 2), adjY+60)
    out.push_str("<line x1=\"");
    push_num(out, cx);
    out.push_str("\" y1=\"");
    push_num(out, adjusted_actor_y + 45.0 * scale);
    out.push_str("\" x2=\"");
    push_num(out, cx + (ACTOR_TYPE_WIDTH / 2.0 - 2.0) * scale);
    out.push_str("\" y2=\"");
    push_num(out, adjusted_actor_y + 60.0 * scale);
    out.push_str("\"></line>");

    // head circle: cx=center, cy=adjY+10, r=15. Width/height attrs
    // are leftover from upstream's pre-scale code; emitted verbatim.
    out.push_str("<circle cx=\"");
    push_num(out, cx);
    out.push_str("\" cy=\"");
    push_num(out, adjusted_actor_y + 10.0 * scale);
    out.push_str("\" r=\"");
    push_num(out, 15.0 * scale);
    out.push_str("\" width=\"");
    push_num(out, a.width * scale);
    out.push_str("\" height=\"");
    push_num(out, a.height * scale);
    out.push_str("\"></circle>");

    // Text label — drawText byTspan: x=rect.x+width/2, y=y_param+height/2
    // where y_param = adjustedActorY + 35*scale, rect.height = bounds.height/scale
    // For default scale=1 with actor.height already set to bounds.height
    // (=65 = conf.height), text y = adjustedActorY + 35 + 32.5 = adjY+67.5.
    let text_y = adjusted_actor_y + 35.0 * scale + (a.height / scale) / 2.0;
    let lines = split_br(&a.description);
    let n_lines = lines.len();
    let font_size = 16.0_f64;
    for (i, line) in lines.iter().enumerate() {
        let dy = (i as f64) * font_size - font_size * ((n_lines as f64) - 1.0) / 2.0;
        out.push_str("<text x=\"");
        push_num(out, cx);
        out.push_str("\" y=\"");
        push_num(out, text_y);
        out.push_str(
            "\" style=\"text-anchor: middle; font-size: 16px; font-weight: 400; font-family: ",
        );
        out.push_str(&attr_escape(ACTOR_FONT_FAMILY));
        out.push_str(
            ";\" dominant-baseline=\"central\" alignment-baseline=\"central\" class=\"actor actor-man\"><tspan x=\"",
        );
        push_num(out, cx);
        out.push_str("\" dy=\"");
        push_num(out, dy);
        out.push_str("\">");
        out.push_str(&xml_escape(line));
        out.push_str("</tspan></text>");
    }
    out.push_str("</g>");
}

/// Emit one `<g data-et="control-structure" data-id="iN">` for a Loop
/// (or Alt/Opt/etc, when those land) block: 4 dashed border lines, a
/// 5-vertex labelBox polygon at the top-left, the keyword label inside
/// it (`loop`/`alt`/...), and the bracketed title centred in the top
/// row. Mirrors upstream `drawLoop` in svgDraw (mermaid.js:137622).
///
/// `LABEL_BOX_W=50, LABEL_BOX_H=20, CUT=7` are upstream's hard-coded
/// `genPoints(...,cut=7)` constants — see `drawLabel` (mermaid.js:136895).
fn emit_loop(out: &mut String, lr: &LoopRender) {
    const LABEL_BOX_W: f64 = 50.0;
    const LABEL_BOX_H: f64 = 20.0;
    const CUT: f64 = 7.0;
    out.push_str("<g data-et=\"control-structure\" data-id=\"i");
    out.push_str(&lr.idx.to_string());
    out.push_str("\">");
    // Four dashed border lines: top, right, bottom, left.
    let draw_line = |out: &mut String, x1: f64, y1: f64, x2: f64, y2: f64| {
        out.push_str("<line x1=\"");
        push_num(out, x1);
        out.push_str("\" y1=\"");
        push_num(out, y1);
        out.push_str("\" x2=\"");
        push_num(out, x2);
        out.push_str("\" y2=\"");
        push_num(out, y2);
        out.push_str("\" class=\"loopLine\"></line>");
    };
    draw_line(out, lr.startx, lr.starty, lr.stopx, lr.starty);
    draw_line(out, lr.stopx, lr.starty, lr.stopx, lr.stopy);
    draw_line(out, lr.startx, lr.stopy, lr.stopx, lr.stopy);
    draw_line(out, lr.startx, lr.starty, lr.startx, lr.stopy);
    // Section dividers (alt/par/critical) — dashed horizontal lines
    // between successive arms. Emitted BEFORE the labelBox polygon to
    // match upstream draw order (mermaid.js drawLoop appends section
    // lines into the same group right after the perimeter strokes).
    for sec in &lr.sections {
        out.push_str("<line x1=\"");
        push_num(out, lr.startx);
        out.push_str("\" y1=\"");
        push_num(out, sec.divider_y);
        out.push_str("\" x2=\"");
        push_num(out, lr.stopx);
        out.push_str("\" y2=\"");
        push_num(out, sec.divider_y);
        out.push_str("\" class=\"loopLine\" style=\"stroke-dasharray: 3, 3;\"></line>");
    }
    // labelBox polygon (5 vertices). genPoints(x,y,w,h,cut=7).
    let lx = lr.startx;
    let ly = lr.starty;
    let p2x = lx + LABEL_BOX_W;
    let p3y = ly + LABEL_BOX_H - CUT;
    let p4x = lx + LABEL_BOX_W - CUT * 1.2;
    let p5y = ly + LABEL_BOX_H;
    out.push_str("<polygon points=\"");
    push_num(out, lx);
    out.push(',');
    push_num(out, ly);
    out.push(' ');
    push_num(out, p2x);
    out.push(',');
    push_num(out, ly);
    out.push(' ');
    push_num(out, p2x);
    out.push(',');
    push_num(out, p3y);
    out.push(' ');
    push_num(out, p4x);
    out.push(',');
    push_num(out, p5y);
    out.push(' ');
    push_num(out, lx);
    out.push(',');
    push_num(out, p5y);
    out.push_str("\" class=\"labelBox\"></polygon>");
    // Keyword label `loop` (or `alt`/`opt`/...) inside the labelBox.
    // drawLabel offsets txtObject.y by height/2 BEFORE drawText; with
    // valign='middle', textMargin=boxTextMargin=5 and prevTextHeight=
    // textHeight=0 on the first iteration:
    //   y = round(y + height/2 + (0 + 0 + 5)/2)
    //     = round(starty + 10 + 2.5) = round(starty + 12.5)
    //   x = round(startx + LABEL_BOX_W/2)
    let label_y = round_js(ly + LABEL_BOX_H / 2.0 + 2.5);
    let label_x = round_js(lx + LABEL_BOX_W / 2.0);
    out.push_str("<text x=\"");
    push_num(out, label_x);
    out.push_str("\" y=\"");
    push_num(out, label_y);
    out.push_str(
        "\" text-anchor=\"middle\" dominant-baseline=\"middle\" alignment-baseline=\"middle\" style=\"font-family: ",
    );
    out.push_str(&attr_escape(FONT_FAMILY));
    out.push_str(
        "; font-size: 16px; font-weight: 400;\" class=\"labelText\">",
    );
    out.push_str(lr.keyword);
    out.push_str("</text>");
    // Block title `[Loopy]` centred in the top row.
    //   x = startx + LABEL_BOX_W/2 + (stopx - startx)/2
    //   y_input = starty + boxMargin + boxTextMargin
    // drawText (with valign='middle', textMargin=5, no width / no
    // anchor remap because width=undefined):
    //   y = round(y_input + (0 + 0 + 5)/2)
    //     = round(starty + 10 + 5 + 2.5) = round(starty + 17.5)
    // tspan=true (default in getTextObj3) ⇒ wraps text in <tspan x=…>.
    let title_x = lr.startx + LABEL_BOX_W / 2.0 + (lr.stopx - lr.startx) / 2.0;
    let title_y_input = lr.starty + 10.0 + 5.0; // boxMargin + boxTextMargin
    // Multi-line title (wrapped via wrap_label when too wide for the
    // block's available width). Each `<br/>` split becomes its own
    // `<text>` element with `tspan: true`. yfunc per upstream drawText
    // (mermaid.js:136811): line k's y = round(y_input +
    // (prevTextHeight + textHeight + textMargin) / 2) where prev=text=
    // k * line_h_unrounded after k iterations.
    let title_lines = split_br(&lr.title);
    // Per-line bbox.height ≈ 18.625 for sans-serif 16px (font_metrics
    // line_height returns the unrounded value).
    let lh_unrounded = crate::font_metrics::line_height(
        "sans-serif",
        16.0,
        false,
        false,
    );
    if has_katex(&lr.title) {
        // drawKatex on loopText: foreignObject is centred between
        // startx..stopx and y is `round(starty)` of the loopModel —
        // here the upstream loopModel.starty is `lr.starty` itself.
        if let Some((html, w, h)) = try_render_seq_katex(&lr.title) {
            let cx = (lr.startx + lr.stopx) / 2.0;
            let y = lr.starty.round();
            emit_katex_foreign_object(out, &html, w, h, cx, y);
        }
    } else {
        for (k, line) in title_lines.iter().enumerate() {
            // After k iterations of the bbox accumulation, prev=text=k*lh.
            let acc = (k as f64) * lh_unrounded;
            let title_y = round_js(title_y_input + (acc + acc + 5.0) / 2.0);
            out.push_str("<text x=\"");
            push_num(out, title_x);
            out.push_str("\" y=\"");
            push_num(out, title_y);
            out.push_str(
                "\" text-anchor=\"middle\" style=\"font-family: ",
            );
            out.push_str(&attr_escape(FONT_FAMILY));
            out.push_str(
                "; font-size: 16px; font-weight: 400;\" class=\"loopText\"><tspan x=\"",
            );
            push_num(out, title_x);
            out.push_str("\">");
            out.push_str(&xml_escape(line));
            out.push_str("</tspan></text>");
        }
    }
    // Section labels (alt/par/critical) — bracketed text centred in
    // each section's first row. NO `<tspan>` wrapper here, mirroring
    // upstream which calls `drawText` without the `tspan: true` flag
    // for section titles (mermaid.js drawLoop section title path).
    // Empty labels emit no <text> at all — upstream `addSectionToLoop`
    // pushes the divider but `drawText(...,'')` is a no-op when the
    // text is empty (gate at sequenceRenderer.ts:909).
    for sec in &lr.sections {
        if sec.label.is_empty() {
            continue;
        }
        let sec_x = (lr.startx + lr.stopx) / 2.0;
        let sec_y = round_js(sec.label_y + 2.5);
        out.push_str("<text x=\"");
        push_num(out, sec_x);
        out.push_str("\" y=\"");
        push_num(out, sec_y);
        out.push_str(
            "\" text-anchor=\"middle\" style=\"font-family: ",
        );
        out.push_str(&attr_escape(FONT_FAMILY));
        out.push_str(
            "; font-size: 16px; font-weight: 400;\" class=\"loopText\">",
        );
        out.push_str(&xml_escape(&sec.label));
        out.push_str("</text>");
    }
    out.push_str("</g>");
}

/// Emit one background `<rect>` for a `rect rgb(...) ... end` block.
/// Single self-closing element with `class="rect"`, no surrounding `<g>`.
/// Width / height are computed from `(stop - start)` deltas at emit time.
/// Mirrors upstream `svgDrawCommon.drawBackgroundRect`.
fn emit_rect(out: &mut String, r: &RectRender) {
    out.push_str("<rect x=\"");
    push_num(out, r.startx);
    out.push_str("\" y=\"");
    push_num(out, r.starty);
    out.push_str("\" fill=\"");
    out.push_str(&r.fill);
    out.push_str("\" width=\"");
    push_num(out, r.stopx - r.startx);
    out.push_str("\" height=\"");
    push_num(out, r.stopy - r.starty);
    out.push_str("\" class=\"rect\"></rect>");
}

/// Emit one `<g data-et="note" data-id="iN">` containing a rounded
/// rect + one `<text><tspan>` per `<br>`-split line. Mirrors upstream
/// `drawNote` (`drawText` with `tspan: true`, `valign='center'`).
///
/// When the note text contains `$$..$$` KaTeX, the per-line `<text>`
/// fallback is replaced by a single `<foreignObject>` carrying the
/// rendered KaTeX HTML (matching upstream's `drawKatex` path that
/// `drawNote` dispatches to via `hasKatex(textObj.text)`).
fn emit_note(out: &mut String, n: &NoteRender) {
    out.push_str("<g data-et=\"note\" data-id=\"i");
    out.push_str(&n.idx.to_string());
    out.push_str("\"><rect x=\"");
    push_num(out, n.rect_x);
    out.push_str("\" y=\"");
    push_num(out, n.rect_y);
    out.push_str("\" fill=\"#EDF2AE\" stroke=\"#666\" width=\"");
    push_num(out, n.rect_w);
    out.push_str("\" height=\"");
    push_num(out, n.rect_h);
    out.push_str("\" class=\"note\"></rect>");
    if has_katex(&n.raw_text) {
        if let Some((html, w, h)) = try_render_seq_katex(&n.raw_text) {
            // drawKatex on noteText: foreignObject is centred *over* the
            // rect (rectX + rectW/2 - w/2, rectY + rectH/2 - h/2). The
            // upstream code also bumps the rect's height to
            // `dim.height + 2*textMargin`; in jsdom dim.height comes from
            // the textContent measurement (`round` of one sans-serif-14
            // line ⇒ 16), so the rect is already that tall after layout.
            let cx = n.rect_x + n.rect_w / 2.0;
            let cy = n.rect_y + n.rect_h / 2.0;
            let y = (cy - h / 2.0).round();
            emit_katex_foreign_object(out, &html, w, h, cx, y);
            out.push_str("</g>");
            return;
        }
    }
    for (i, line_text) in n.lines.iter().enumerate() {
        let y = round_js(n.starty_raw + (i as f64) * n.line_step + n.margin_half);
        out.push_str("<text x=\"");
        push_num(out, n.text_x);
        out.push_str("\" y=\"");
        push_num(out, y);
        out.push_str(
            "\" text-anchor=\"middle\" dominant-baseline=\"middle\" alignment-baseline=\"middle\" style=\"font-family: ",
        );
        out.push_str(&attr_escape(FONT_FAMILY));
        out.push_str("; font-size: 16px; font-weight: 400;\" class=\"noteText\" dy=\"1em\"><tspan x=\"");
        push_num(out, n.text_x);
        out.push_str("\">");
        if line_text.is_empty() {
            out.push_str("\u{200B}");
        } else {
            out.push_str(&xml_escape(line_text));
        }
        out.push_str("</tspan></text>");
    }
    out.push_str("</g>");
}

/// Emit the opening `<g onclick="..." cursor="pointer">` that wraps an
/// actor's top group when popup menus are enabled (either the actor has
/// `link`/`links` entries, or `forceMenus: true` is set on the diagram).
/// Mirrors upstream `svgDraw.popupMenuToggle`.
fn push_popup_g_open(out: &mut String, rank: usize) {
    out.push_str("<g onclick=\"var pu = document.getElementById('actor");
    out.push_str(&rank.to_string());
    out.push_str(
        "_popup'); if (pu != null) { pu.style.display = pu.style.display == 'block' ? 'none' : 'block'; }\" cursor=\"pointer\">",
    );
}

/// Lifeline-only top group for Actor type — `<g><line id="actorN"></g>`.
/// The body (stick-figure) is emitted separately, after `<defs>`.
fn emit_actor_top_lifeline_actor(
    out: &mut String,
    a: &ActorRender,
    centery: f64,
    bottom_y: f64,
    rank: usize,
    popup: bool,
) {
    let cx = a.x + a.width / 2.0;
    if popup {
        push_popup_g_open(out, rank);
    } else {
        out.push_str("<g>");
    }
    out.push_str("<line id=\"actor");
    out.push_str(&rank.to_string());
    out.push_str("\" x1=\"");
    push_num(out, cx);
    out.push_str("\" y1=\"");
    push_num(out, centery);
    out.push_str("\" x2=\"");
    push_num(out, cx);
    out.push_str("\" y2=\"");
    push_num(out, bottom_y);
    out.push_str(
        "\" class=\"actor-line 200\" stroke-width=\"0.5px\" stroke=\"#999\" name=\"",
    );
    out.push_str(&xml_escape(&a.id));
    out.push_str("\" data-et=\"life-line\" data-id=\"");
    out.push_str(&xml_escape(&a.id));
    out.push_str("\"></line></g>");
}

/// Emit the byTspan multi-line `<text><tspan>` group for an actor box.
/// Mirrors upstream `_drawTextCandidateFunc.byTspan` in svgDraw — one
/// `<text>` element per line, sharing the same `y`, with `dy` offsets so
/// lines stack vertically around the centre.
fn emit_actor_box_text(out: &mut String, cx: f64, cy: f64, description: &str) {
    emit_actor_box_text_ex(out, cx, cy, description, false);
}

/// Footer-pass quirk: in the bottom-row actors, a stray empty
/// `<switch><foreignObject><div><div></div></div></foreignObject></switch>`
/// gets emitted exactly once on the FIRST footer actor whose description
/// contains KaTeX in the source order. This replicates a side effect of
/// upstream's non-awaited `byKatex` async fire-and-forget call: the first
/// promise's `await renderKatexSanitized` throws because of state from prior
/// renders, leaving an empty foreignObject behind. See sequence/07.svg
/// (only actor "1" footer has the placeholder) vs sequence/08.svg (no
/// footer placeholders — all participants render successfully and the
/// pile-up never produces an orphan switch).
fn emit_actor_box_text_footer_empty(out: &mut String, cx: f64, cy: f64) {
    out.push_str("<switch><foreignObject x=\"");
    push_num(out, cx);
    out.push_str("\" y=\"");
    push_num(out, cy);
    out.push_str("\" width=\"0\" height=\"0\"><div style=\"height: 100%; width: 100%;\"><div style=\"text-align: center; vertical-align: middle;\"></div></div></foreignObject></switch>");
}

fn emit_actor_box_text_ex(
    out: &mut String,
    cx: f64,
    cy: f64,
    description: &str,
    _is_footer: bool,
) {
    let lines = split_br(description);
    let n = lines.len();
    let font_size = 16.0_f64;
    let katex = has_katex(description);
    if katex {
        out.push_str("<switch><foreignObject x=\"");
        push_num(out, cx);
        out.push_str("\" y=\"");
        push_num(out, cy);
        out.push_str("\" width=\"0\" height=\"0\"><div style=\"height: 100%; width: 100%;\" class=\"actor actor-box\"><div style=\"text-align: center; vertical-align: middle;\">");
        #[cfg(feature = "katex")]
        {
            match crate::katex::render_label(description) {
                Ok(html) => out.push_str(&html),
                Err(_) => out
                    .push_str("MathML is unsupported in this environment."),
            }
        }
        #[cfg(not(feature = "katex"))]
        {
            out.push_str("MathML is unsupported in this environment.");
        }
        out.push_str("</div></div></foreignObject>");
    }
    for (i, line) in lines.iter().enumerate() {
        let dy = (i as f64) * font_size - font_size * ((n as f64) - 1.0) / 2.0;
        out.push_str("<text x=\"");
        push_num(out, cx);
        out.push_str("\" y=\"");
        push_num(out, cy);
        out.push_str(
            "\" style=\"text-anchor: middle; font-size: 16px; font-weight: 400; font-family: ",
        );
        out.push_str(&attr_escape(ACTOR_FONT_FAMILY));
        out.push_str(
            ";\" dominant-baseline=\"central\" alignment-baseline=\"central\" class=\"actor actor-box\"><tspan x=\"",
        );
        push_num(out, cx);
        out.push_str("\" dy=\"");
        push_num(out, dy);
        out.push_str("\">");
        out.push_str(&xml_escape(line));
        out.push_str("</tspan></text>");
    }
    if katex {
        out.push_str("</switch>");
    }
}

/// Render a KaTeX label and return `(html, raw_width, raw_height)`. The
/// raw measurements correspond to upstream `getBoundingClientRect()`'s
/// fractional output, so the caller can independently round the values
/// it splats into the `<foreignObject>` width/height attributes versus
/// the values it feeds into the x/y centring math (upstream applies
/// `Math.round` separately at each call site, see `drawKatex` in
/// sequenceDiagram-S5P6DPVE.mjs:1976-1995).
#[cfg(feature = "katex")]
fn try_render_seq_katex(text: &str) -> Option<(String, f64, f64)> {
    if !has_katex(text) {
        return None;
    }
    match crate::katex::render_label_raw(text) {
        Ok(html) => {
            let font = crate::render::foreign_object::HtmlLabelFont::default();
            let (w, h) = crate::render::foreign_object::measure_html_markup_label(
                &html, &font, f64::INFINITY, false,
            );
            Some((html, w, h))
        }
        Err(_) => None,
    }
}

#[cfg(not(feature = "katex"))]
fn try_render_seq_katex(_text: &str) -> Option<(String, f64, f64)> {
    None
}

/// Emit a `<foreignObject>` carrying KaTeX-rendered HTML, matching
/// upstream `drawKatex` output for note/message/loop label. `w` and `h`
/// are the raw (unrounded) measurements; the helper rounds the value
/// going into each attribute / centring computation independently.
fn emit_katex_foreign_object(
    out: &mut String,
    html: &str,
    w: f64,
    h: f64,
    cx: f64,
    y: f64,
) {
    out.push_str("<foreignObject height=\"");
    push_num(out, h.round());
    out.push_str("\" width=\"");
    push_num(out, w.round());
    out.push_str("\" x=\"");
    push_num(out, (cx - w / 2.0).round());
    out.push_str("\" y=\"");
    push_num(out, y);
    out.push_str("\"><div style=\"width: fit-content;\" xmlns=\"http://www.w3.org/1999/xhtml\">");
    out.push_str(html);
    out.push_str("</div></foreignObject>");
}

/// Resolve the `(fill, base_class)` pair for an actor's main rectangle.
///
/// When `properties <id>: {"class": ...}` was given, the directive class
/// replaces the default `actor` base class and the rect uses the custom
/// service-actor fill `#EDF2AE`. Otherwise the default theme styling
/// (`#eaeaea`, base class `actor`) applies. Mirrors upstream
/// `svgDraw.drawActor`'s `rect.class = actor.properties.class || 'actor'`
/// branch.
fn actor_rect_style(a: &ActorRender) -> (&str, &str) {
    if let Some(cls) = a.class_name.as_deref() {
        ("#EDF2AE", cls)
    } else {
        ("#eaeaea", "actor")
    }
}

fn emit_actor_bottom_participant(
    out: &mut String,
    a: &ActorRender,
    bottom_y: f64,
    emit_empty_katex_switch: bool,
) {
    let (fill, base_cls) = actor_rect_style(a);
    out.push_str("<g><rect x=\"");
    push_num(out, a.x);
    out.push_str("\" y=\"");
    push_num(out, bottom_y);
    out.push_str("\" fill=\"");
    out.push_str(fill);
    out.push_str("\" stroke=\"#666\" width=\"");
    push_num(out, a.width);
    out.push_str("\" height=\"");
    push_num(out, a.height);
    out.push_str("\" name=\"");
    out.push_str(&xml_escape(&a.id));
    out.push_str("\" rx=\"3\" ry=\"3\" class=\"");
    out.push_str(base_cls);
    out.push_str(" actor-bottom\"></rect>");
    let cx = a.x + a.width / 2.0;
    let cy = bottom_y + a.height / 2.0;
    if emit_empty_katex_switch {
        emit_actor_box_text_footer_empty(out, cx, cy);
    } else if !has_katex(&a.description) {
        // Footer pass renders the description via byTspan. Upstream's
        // byKatex is async and fired without awaiting, so when the
        // promise rejects (which it does for non-trivial descriptions
        // by the time the footer pass kicks off — see the explanatory
        // comment on `emit_actor_box_text_footer_empty`) the multi-line
        // tspan fallback is also dropped. To stay byte-exact with both
        // sequence/07 and sequence/08 references we simply omit the
        // text entirely whenever the footer description carries KaTeX.
        emit_actor_box_text(out, cx, cy, &a.description);
    }
    out.push_str("</g>");
}

fn emit_actor_top_participant(
    out: &mut String,
    a: &ActorRender,
    bottom_y: f64,
    rank: usize,
    root_index: usize,
    popup: bool,
) {
    let _ = a.cnt;
    let cx = a.x + a.width / 2.0;
    let centery = a.starty + a.height;
    let top_y = a.starty;
    if popup {
        push_popup_g_open(out, rank);
    } else {
        out.push_str("<g>");
    }
    out.push_str("<line id=\"actor");
    out.push_str(&rank.to_string());
    out.push_str("\" x1=\"");
    push_num(out, cx);
    out.push_str("\" y1=\"");
    push_num(out, centery);
    out.push_str("\" x2=\"");
    push_num(out, cx);
    out.push_str("\" y2=\"");
    push_num(out, bottom_y);
    out.push_str(
        "\" class=\"actor-line 200\" stroke-width=\"0.5px\" stroke=\"#999\" name=\"",
    );
    out.push_str(&xml_escape(&a.id));
    out.push_str("\" data-et=\"life-line\" data-id=\"");
    out.push_str(&xml_escape(&a.id));
    out.push_str("\"></line><g id=\"root-");
    out.push_str(&root_index.to_string());
    out.push_str(
        "\" data-et=\"participant\" data-type=\"participant\" data-id=\"",
    );
    out.push_str(&xml_escape(&a.id));
    let (fill, base_cls) = actor_rect_style(a);
    out.push_str("\"><rect x=\"");
    push_num(out, a.x);
    out.push_str("\" y=\"");
    push_num(out, top_y);
    out.push_str("\" fill=\"");
    out.push_str(fill);
    out.push_str("\" stroke=\"#666\" width=\"");
    push_num(out, a.width);
    out.push_str("\" height=\"");
    push_num(out, a.height);
    out.push_str("\" name=\"");
    out.push_str(&xml_escape(&a.id));
    out.push_str("\" rx=\"3\" ry=\"3\" class=\"");
    out.push_str(base_cls);
    out.push_str(" actor-top\"></rect>");
    let cy = top_y + a.height / 2.0;
    emit_actor_box_text(out, cx, cy, &a.description);
    out.push_str("</g></g>");
}

/// Emit `<g id="actorN_popup" class="actorPopupMenu" display="...">` —
/// the popup container holding a panel `<rect>` plus one `<a><text>`
/// per link. Mirrors upstream `svgDraw.popupMenu`.
///
/// Geometry — derived from the upstream svg:
///   - rect.x = actor.x, rect.y = actor.height (= 65 for default conf)
///   - rect.width = actor.width
///   - rect.height = 20 + 30 * N    (N = link count, no header text)
///   - text.x = actor.x + 10
///   - text.y of link n = actor.height + 30 * (n + 1)
fn emit_actor_popup(out: &mut String, a: &ActorRender, rank: usize, force_menus: bool, mirror: bool) {
    let panel_x = a.x;
    let panel_y = a.height; // actor box bottom in the top group.
    let n_links = a.links.len() as f64;
    let panel_h = 20.0 + 30.0 * n_links;
    let display = if force_menus { "block !important" } else { "none" };

    out.push_str("<g id=\"actor");
    out.push_str(&rank.to_string());
    out.push_str("_popup\" class=\"actorPopupMenu\" display=\"");
    out.push_str(display);
    let (panel_fill, panel_cls) = actor_rect_style(a);
    let panel_position = if mirror { "actor-bottom" } else { "actor-top" };
    out.push_str("\"><rect class=\"actorPopupMenuPanel ");
    out.push_str(panel_cls);
    out.push_str(" ");
    out.push_str(panel_position);
    out.push_str("\" x=\"");
    push_num(out, panel_x);
    out.push_str("\" y=\"");
    push_num(out, panel_y);
    out.push_str("\" fill=\"");
    out.push_str(panel_fill);
    out.push_str("\" stroke=\"#666\" width=\"");
    push_num(out, a.width);
    out.push_str("\" height=\"");
    push_num(out, panel_h);
    out.push_str("\" rx=\"3\" ry=\"3\"></rect>");

    let text_x = panel_x + 10.0;
    for (i, (name, url)) in a.links.iter().enumerate() {
        let text_y = panel_y + 30.0 * ((i as f64) + 1.0);
        out.push_str("<a href=\"");
        out.push_str(&attr_escape(url));
        out.push_str("\" target=\"_blank\"><text x=\"");
        push_num(out, text_x);
        out.push_str("\" y=\"");
        push_num(out, text_y);
        out.push_str(
            "\" style=\"text-anchor: start; font-weight: 400; font-family: ",
        );
        out.push_str(&attr_escape(ACTOR_FONT_FAMILY));
        out.push_str(
            ";\" dominant-baseline=\"central\" alignment-baseline=\"central\" class=\"actor\"><tspan x=\"",
        );
        push_num(out, text_x);
        out.push_str("\" dy=\"0\">");
        out.push_str(&xml_escape(name));
        out.push_str("</tspan></text></a>");
    }
    out.push_str("</g>");
}

fn emit_message(out: &mut String, id: &str, m: &MsgRender) {
    // `is_dashed` controls the `messageLine1` class + dasharray style.
    // Dashed (DOTTED variants) are arrows AND open lines spelt with two
    // dashes (`-->>`, `-->`).
    let is_dashed = matches!(
        m.arrow,
        ArrowType::DottedArrow
            | ArrowType::DottedLine
            | ArrowType::DottedCross
            | ArrowType::DottedPoint
            | ArrowType::BiDotted
            | ArrowType::SolidTopDotted
            | ArrowType::SolidBottomDotted
            | ArrowType::StickTopDotted
            | ArrowType::StickBottomDotted
            | ArrowType::SolidTopReverseDotted
            | ArrowType::SolidBottomReverseDotted
            | ArrowType::StickTopReverseDotted
            | ArrowType::StickBottomReverseDotted
    );
    // `has_arrowhead`: upstream attaches `marker-end="...arrowhead"` for
    // SOLID / DOTTED only — the `_OPEN` variants (`->`, `-->`) get no
    // marker-end and instead match the messageLine class only.
    let has_arrowhead = matches!(
        m.arrow,
        ArrowType::SolidArrow | ArrowType::DottedArrow
    );
    // Cross arrows (`-x`, `--x`) emit `marker-end="...crosshead"` instead
    // of `arrowhead`. Otherwise they share the same line geometry +
    // attribute order as the solid/dotted arrowhead variants (the arrow
    // gap toward the receiver lifeline is identical).
    let has_crosshead = matches!(
        m.arrow,
        ArrowType::SolidCross | ArrowType::DottedCross
    );
    let has_pointhead = matches!(
        m.arrow,
        ArrowType::SolidPoint | ArrowType::DottedPoint
    );
    // Bidirectional arrows (`<<->>`, `<<-->>`) carry arrowheads on both
    // ends → both `marker-start` AND `marker-end="...arrowhead"`.
    let is_bidir = matches!(m.arrow, ArrowType::BiSolid | ArrowType::BiDotted);
    // Forward filled / stick half-arrow heads (`-|\`, `-|/`, `-\\`, `-//`
    // and dotted variants). Each maps to one of four marker ids on
    // `marker-end`. Mirrors upstream sequenceRenderer.ts:3518-3528.
    let half_marker_end: Option<&str> = match m.arrow {
        ArrowType::SolidTop | ArrowType::SolidTopDotted => Some("-solidTopArrowHead"),
        ArrowType::SolidBottom | ArrowType::SolidBottomDotted => Some("-solidBottomArrowHead"),
        ArrowType::StickTop | ArrowType::StickTopDotted => Some("-stickTopArrowHead"),
        ArrowType::StickBottom | ArrowType::StickBottomDotted => Some("-stickBottomArrowHead"),
        _ => None,
    };
    // Reverse half-arrows put the head at the source actor instead of
    // the destination. The marker is mirrored (top↔bottom) because the
    // marker is rendered with `auto-start-reverse` orientation when used
    // as `marker-start`. Mirrors sequenceRenderer.ts:3530-3541.
    let half_marker_start: Option<&str> = match m.arrow {
        ArrowType::SolidTopReverse | ArrowType::SolidTopReverseDotted => {
            Some("-solidBottomArrowHead")
        }
        ArrowType::SolidBottomReverse | ArrowType::SolidBottomReverseDotted => {
            Some("-solidTopArrowHead")
        }
        ArrowType::StickTopReverse | ArrowType::StickTopReverseDotted => {
            Some("-stickBottomArrowHead")
        }
        ArrowType::StickBottomReverse | ArrowType::StickBottomReverseDotted => {
            Some("-stickTopArrowHead")
        }
        _ => None,
    };

    // <text> per line (multi-line via `<br>` splits to separate <text>
    // elements with stepping y, mirroring upstream `drawText` in
    // `valign='center'` mode with `tspan: false`).
    //
    // When the message text contains `$$..$$` KaTeX, the per-line
    // `<text>` fallback is replaced by a single `<foreignObject>` with
    // the rendered KaTeX HTML. Mirrors upstream `drawKatex` invoked
    // from sequenceRenderer.ts via `hasKatex(msg.message) ? drawKatex
    // : drawText`. The y anchor matches `round(starty - h)` of the
    // upstream msgModel.
    let mut katex_emitted = false;
    if has_katex(&m.raw_text) {
        if let Some((html, w, h)) = try_render_seq_katex(&m.raw_text) {
            // drawKatex on messageText: y is `round(lineStartY - h)`
            // where `lineStartY` is the message's line draw y (this
            // crate's `line_start_y`) and `h` is the raw (unrounded)
            // bbox height. Mirrors upstream sequenceRenderer.ts:3466 —
            // `await drawKatex(..., { startx, stopx, starty: lineStartY })`.
            let cx = m.msg_min_startx + m.msg_dx_abs / 2.0;
            let y = (m.line_start_y - h).round();
            emit_katex_foreign_object(out, &html, w, h, cx, y);
            katex_emitted = true;
        }
    }
    if !katex_emitted {
        for (n, line_text) in m.lines.iter().enumerate() {
            let y = round_js(m.starty_raw + (n as f64) * m.line_step + m.y_offset);
            out.push_str("<text x=\"");
            push_num(out, m.text_x);
            out.push_str("\" y=\"");
            push_num(out, y);
            out.push_str(
                "\" text-anchor=\"middle\" dominant-baseline=\"middle\" alignment-baseline=\"middle\" style=\"font-family: ",
            );
            out.push_str(&attr_escape(FONT_FAMILY));
            out.push_str("; font-size: 16px; font-weight: 400;\" class=\"messageText\" dy=\"1em\">");
            if line_text.is_empty() {
                // Upstream `drawText` substitutes a zero-width space (U+200B)
                // for empty lines so the bbox is still measurable.
                out.push_str("\u{200B}");
            } else {
                out.push_str(&xml_escape(line_text));
            }
            out.push_str("</text>");
        }
    }

    // <line> or <path> next.
    if m.is_self {
        // Reverse half-arrows on self-loops use `marker-start`. With
        // autonumber active, the path's start point + first control
        // point shift right by +10 to clear the arrow head and make
        // room for the sequence-number circle on the source side.
        // Without autonumber, no shift is applied (upstream byte-exact).
        let start_offset = if half_marker_start.is_some() && m.seq_index.is_some() { 10.0 } else { 0.0 };
        let lsx = m.self_line_start_x;
        let sx = m.self_startx;
        let lsy = m.line_start_y;
        out.push_str("<path d=\"M ");
        push_num(out, lsx + start_offset);
        out.push(',');
        push_num(out, lsy);
        out.push_str(" C ");
        push_num(out, lsx + 60.0 + start_offset);
        out.push(',');
        push_num(out, lsy - 10.0);
        out.push(' ');
        push_num(out, sx + 60.0);
        out.push(',');
        push_num(out, lsy + 30.0);
        out.push(' ');
        push_num(out, sx);
        out.push(',');
        push_num(out, lsy + 20.0);
        if is_dashed {
            out.push_str("\" style=\"stroke-dasharray: 3, 3; fill: none;");
        }
        out.push_str("\" class=\"messageLine");
        out.push_str(if is_dashed { "1" } else { "0" });
        out.push_str("\" data-et=\"message\" data-id=\"i");
        out.push_str(&m.idx.to_string());
        out.push_str("\" data-from=\"");
        out.push_str(&attr_escape(&m.from));
        out.push_str("\" data-to=\"");
        out.push_str(&attr_escape(&m.to));
        if is_dashed {
            out.push_str("\" stroke-width=\"2\" stroke=\"none");
        } else {
            out.push_str("\" stroke-width=\"2\" stroke=\"none\" style=\"fill: none;");
        }
        if is_bidir {
            out.push_str("\" marker-start=\"url(#");
            out.push_str(id);
            out.push_str("-arrowhead)\" marker-end=\"url(#");
            out.push_str(id);
            out.push_str("-arrowhead)");
        } else if has_arrowhead {
            out.push_str("\" marker-end=\"url(#");
            out.push_str(id);
            out.push_str("-arrowhead)");
        } else if has_crosshead {
            out.push_str("\" marker-end=\"url(#");
            out.push_str(id);
            out.push_str("-crosshead)");
        } else if has_pointhead {
            out.push_str("\" marker-end=\"url(#");
            out.push_str(id);
            out.push_str("-filled-head)");
        } else if let Some(marker) = half_marker_end {
            out.push_str("\" marker-end=\"url(#");
            out.push_str(id);
            out.push_str(marker);
            out.push(')');
        } else if let Some(marker) = half_marker_start {
            out.push_str("\" marker-start=\"url(#");
            out.push_str(id);
            out.push_str(marker);
            out.push(')');
        }
        if m.seq_index.is_some() {
            // Reverse half-arrow self-loops emit `x2` BEFORE `x1` —
            // upstream's d3 chain sets `x2` first when reverse, so the
            // attribute order in the serialised DOM is x2 then x1.
            if half_marker_start.is_some() {
                out.push_str("\" x2=\"");
                push_num(out, m.line_x2);
            }
            out.push_str("\" x1=\"");
            push_num(out, m.line_x1);
        }
        out.push_str("\">");
        out.push_str("</path>");
    } else {
    out.push_str("<line x1=\"");
    push_num(out, m.line_x1);
    out.push_str("\" y1=\"");
    push_num(out, m.line_start_y);
    out.push_str("\" x2=\"");
    push_num(out, m.line_x2);
    out.push_str("\" y2=\"");
    push_num(out, m.line_start_y);
    // Attribute order, observed in reference SVGs (depends on which of
    // `style.fill` / `style.stroke-dasharray` upstream set last):
    //
    //   * Dashed + has_arrowhead (`-->>` arrow):
    //       y2="..." style="stroke-dasharray: 3, 3; fill: none;" class=...
    //       stroke-width="2" stroke="none" marker-end="..."
    //   * Solid + has_arrowhead (`->>`):
    //       y2="..." class=... stroke-width="2" stroke="none"
    //       style="fill: none;" marker-end="..."
    //   * Dashed + open (`-->`):
    //       y2="..." style="stroke-dasharray: 3, 3; fill: none;" class=...
    //       stroke-width="2" stroke="none"
    //   * Solid + open (`->`): style="fill: none;" still after class —
    //     same shape as solid+arrowhead minus `marker-end`.
    if is_dashed {
        out.push_str("\" style=\"stroke-dasharray: 3, 3; fill: none;");
    }
    out.push_str("\" class=\"messageLine");
    out.push_str(if is_dashed { "1" } else { "0" });
    out.push_str("\" data-et=\"message\" data-id=\"i");
    out.push_str(&m.idx.to_string());
    out.push_str("\" data-from=\"");
    out.push_str(&attr_escape(&m.from));
    out.push_str("\" data-to=\"");
    out.push_str(&attr_escape(&m.to));
    if is_dashed {
        out.push_str("\" stroke-width=\"2\" stroke=\"none");
    } else {
        out.push_str("\" stroke-width=\"2\" stroke=\"none\" style=\"fill: none;");
    }
    if is_bidir {
        out.push_str("\" marker-start=\"url(#");
        out.push_str(id);
        out.push_str("-arrowhead)\" marker-end=\"url(#");
        out.push_str(id);
        out.push_str("-arrowhead)\">");
    } else if has_arrowhead {
        out.push_str("\" marker-end=\"url(#");
        out.push_str(id);
        out.push_str("-arrowhead)\">");
    } else if has_crosshead {
        out.push_str("\" marker-end=\"url(#");
        out.push_str(id);
        out.push_str("-crosshead)\">");
    } else if has_pointhead {
        out.push_str("\" marker-end=\"url(#");
        out.push_str(id);
        out.push_str("-filled-head)\">");
    } else if let Some(marker) = half_marker_end {
        out.push_str("\" marker-end=\"url(#");
        out.push_str(id);
        out.push_str(marker);
        out.push_str(")\">");
    } else if let Some(marker) = half_marker_start {
        out.push_str("\" marker-start=\"url(#");
        out.push_str(id);
        out.push_str(marker);
        out.push_str(")\">");
    } else {
        out.push_str("\">");
    }
    out.push_str("</line>");
    }

    // Central-connection `()` circles. Mirrors upstream
    // `sequenceRenderer.ts:329-372`:
    //   <g><circle cx=fromCx cy=lineY r=5 width=10 height=10/>
    //      <circle cx=toCx ... /></g>
    // Emitted after the message line. Coordinates are the lifeline
    // centres at message Y. (No autonumber offset implemented in this
    // pass — the no-autonumber subset is the byte-exact target.)
    if let Some(cc) = m.central_connection {
        out.push_str("<g>");
        let emit_circle = |out: &mut String, cx: f64, cy: f64| {
            out.push_str("<circle cx=\"");
            push_num(out, cx);
            out.push_str("\" cy=\"");
            push_num(out, cy);
            out.push_str("\" r=\"5\" width=\"10\" height=\"10\"></circle>");
        };
        match cc {
            CentralConnection::AtTo => emit_circle(out, m.to_cx, m.line_start_y),
            CentralConnection::AtFrom => emit_circle(out, m.from_cx, m.line_start_y),
            CentralConnection::Dual => {
                emit_circle(out, m.from_cx, m.line_start_y);
                emit_circle(out, m.to_cx, m.line_start_y);
            }
        }
        out.push_str("</g>");
    }

    // Autonumber sequence-number marker line + text. Mirrors upstream
    // `sequenceRenderer.ts:712-729`:
    //   <line x1=seqX y1=lineY x2=seqX y2=lineY stroke-width=0
    //         marker-start="url(#…-sequencenumber)"></line>
    //   <text x=seqX y=lineY+4 font-family="sans-serif" font-size="12px"
    //         text-anchor="middle" class="sequenceNumber">N</text>
    if let Some(seq) = m.seq_index {
        out.push_str("<line x1=\"");
        push_num(out, m.seq_x);
        out.push_str("\" y1=\"");
        push_num(out, m.line_start_y);
        out.push_str("\" x2=\"");
        push_num(out, m.seq_x);
        out.push_str("\" y2=\"");
        push_num(out, m.line_start_y);
        out.push_str("\" stroke-width=\"0\" marker-start=\"url(#");
        out.push_str(id);
        out.push_str("-sequencenumber)\"></line>");
        out.push_str("<text x=\"");
        push_num(out, m.seq_x);
        out.push_str("\" y=\"");
        push_num(out, m.line_start_y + 4.0);
        out.push_str(
            "\" font-family=\"sans-serif\" font-size=\"12px\" text-anchor=\"middle\" class=\"sequenceNumber\">",
        );
        out.push_str(&seq.to_string());
        out.push_str("</text>");
    }
}

/// Compute the bbox.height of a single line in the messageFont. Upstream's
/// `calculateTextDimensions` uses jsdom's `getBBox()`, which returns
/// `Math.round(line_height_px)` for a single ASCII line. `line_height_px`
/// comes from DejaVu Sans metrics: `(ascender + |descender|) / units_per_em
/// * font_size` — see [`crate::font_metrics::line_height`].
fn compute_message_line_height(font_size: f64) -> f64 {
    crate::font_metrics::line_height("sans-serif", font_size, false, false).round()
}

/// Word-wrap a label to fit within `max_width` pixels, mirroring
/// upstream `utils.wrapLabel`. Returns the wrapped text with `<br/>`
/// inserted between lines (the same separator the rest of the
/// sequence pipeline already understands).
///
/// Edge cases:
///   - Empty label → returned unchanged.
///   - Label already containing `<br>` → returned unchanged (so a
///     wrap directive on a manually-broken message is a no-op).
///   - Single word longer than `max_width` → broken with hyphens via
///     `break_string`.
fn wrap_label(label: &str, max_width: f64, family: &str, font_size: f64) -> String {
    if label.is_empty() {
        return label.to_string();
    }
    if !split_br(label).get(1).is_none() {
        // Already contains <br> — return as-is.
        return label.to_string();
    }
    let words: Vec<&str> = label.split(' ').filter(|w| !w.is_empty()).collect();
    let mut completed_lines: Vec<String> = Vec::new();
    let mut next_line = String::new();
    let n = words.len();
    for (index, word) in words.iter().enumerate() {
        let word_with_sp = format!("{} ", word);
        let word_length = crate::font_metrics::text_width(
            &word_with_sp,
            family,
            font_size,
            false,
            false,
        );
        let next_line_length = crate::font_metrics::text_width(
            &next_line,
            family,
            font_size,
            false,
            false,
        );
        if word_length > max_width {
            let (hyphenated, remaining) =
                break_string(word, max_width, '-', family, font_size);
            completed_lines.push(next_line.clone());
            for h in hyphenated {
                completed_lines.push(h);
            }
            next_line = remaining;
        } else if next_line_length + word_length >= max_width {
            completed_lines.push(next_line.clone());
            next_line = word.to_string();
        } else if next_line.is_empty() {
            next_line = word.to_string();
        } else {
            next_line.push(' ');
            next_line.push_str(word);
        }
        if index + 1 == n {
            completed_lines.push(next_line.clone());
        }
    }
    completed_lines
        .into_iter()
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("<br/>")
}

/// Break a single word that's too wide with hyphen markers, mirroring
/// upstream `breakString`. Returns the list of hyphenated chunks plus
/// any remaining tail.
fn break_string(
    word: &str,
    max_width: f64,
    hyphen: char,
    family: &str,
    font_size: f64,
) -> (Vec<String>, String) {
    let chars: Vec<char> = word.chars().collect();
    let mut lines: Vec<String> = Vec::new();
    let mut current_line = String::new();
    let n = chars.len();
    for (index, &ch) in chars.iter().enumerate() {
        let mut next_line = current_line.clone();
        next_line.push(ch);
        let line_width = crate::font_metrics::text_width(
            &next_line,
            family,
            font_size,
            false,
            false,
        );
        if line_width >= max_width {
            let is_last = index + 1 == n;
            let hyphenated = if is_last {
                next_line.clone()
            } else {
                let mut s = next_line.clone();
                s.push(hyphen);
                s
            };
            lines.push(hyphenated);
            current_line = String::new();
        } else {
            current_line = next_line;
        }
    }
    (lines, current_line)
}

/// Split a string on `<br>` / `<br/>` / `<br />` (case-insensitive).
/// Mirrors upstream `common.lineBreakRegex = /<br\s*\/?>/gi`.
fn split_br(s: &str) -> Vec<&str> {
    // Hand-rolled: scan for `<br`, optional whitespace, optional `/`, `>`.
    let bytes = s.as_bytes();
    let mut out: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i + 3 <= bytes.len() {
        let b0 = bytes[i];
        let b1 = bytes[i + 1];
        let b2 = bytes[i + 2];
        if b0 == b'<' && (b1 == b'b' || b1 == b'B') && (b2 == b'r' || b2 == b'R') {
            // Walk past optional ws + optional '/' to find '>'.
            let mut j = i + 3;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'/' {
                j += 1;
            }
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'>' {
                // Found a <br...> tag. Push the segment before it.
                out.push(&s[start..i]);
                start = j + 1;
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out.push(&s[start..]);
    out
}

/// Resolve `#word;` / `#NNN;` placeholders in `s` to the same intermediate
/// form upstream `encodeEntities` produces (`utils.ts`):
///
///   `#word;` → `ﬂ°word¶ß`   (U+FB02 LATIN SMALL LIGATURE FL,
///                             U+00B0 DEGREE SIGN, word,
///                             U+00B6 PILCROW SIGN, U+00DF SHARP S)
///   `#NNN;`  → `ﬂ°°NNN¶ß`   (extra U+00B0 marks numeric codepoint)
///
/// Upstream measures actor descriptions / message text in this
/// post-`encodeEntities` form via canvas/getBBox — so glyph advances
/// for `ﬂ°…¶ß` (not the eventual `&lt;`/`<`) drive actor box widths.
/// Restoring this for our DejaVu glyph-sum metric keeps actor widths
/// byte-exact for fixtures whose labels embed `#lt;`/`#gt;`/`#colon;`
/// (e.g. `tests/ext_fixtures/demos/sequence/03.mmd`).
fn resolve_hash_entities_for_measure(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' {
            if let Some((name, is_num, next)) = try_consume_hash_entity_name(bytes, i) {
                out.push('\u{FB02}'); // ﬂ
                out.push('\u{B0}'); // °
                if is_num {
                    out.push('\u{B0}'); // extra ° marks numeric form
                }
                out.push_str(name);
                out.push('\u{B6}'); // ¶
                out.push('\u{DF}'); // ß
                i = next;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Like `try_consume_hash_entity` but returns the inner name and a numeric
/// flag instead of the rendered `&...;` replacement. Used by the
/// width-measurement encoder above.
fn try_consume_hash_entity_name(bytes: &[u8], i: usize) -> Option<(&str, bool, usize)> {
    if bytes.get(i)? != &b'#' {
        return None;
    }
    let start = i + 1;
    let mut end = start;
    while end < bytes.len() && bytes[end] != b';' {
        let b = bytes[end];
        if !b.is_ascii_alphanumeric() && b != b'_' {
            return None;
        }
        end += 1;
    }
    if end >= bytes.len() || end == start {
        return None;
    }
    let name = std::str::from_utf8(&bytes[start..end]).ok()?;
    let is_num = name.bytes().all(|b| b.is_ascii_digit());
    Some((name, is_num, end + 1))
}

/// Number formatter mirroring d3's "drop trailing zeroes" behaviour, used
/// for SVG attribute values: integers stay integer-formatted; fractional
/// values keep enough precision to round-trip.
fn push_num(out: &mut String, v: f64) {
    if v.fract() == 0.0 && v.is_finite() {
        out.push_str(&format!("{}", v as i64));
    } else {
        // d3 default: full precision with no trailing zeros. Most
        // cases need a single decimal (e.g. 32.5).
        let s = format!("{v}");
        out.push_str(&s);
    }
}

/// JS-compatible `Math.round` — rounds half-up (toward +∞ for halves).
/// Identity: `Math.round(v) === Math.floor(v + 0.5)` for finite values.
/// Rust's `f64::round()` rounds half-away-from-zero, so it differs from
/// JS for negative halves; this helper restores JS semantics.
fn round_js(v: f64) -> f64 {
    (v + 0.5).floor()
}

/// Try to consume a mermaid `#name;` entity placeholder starting at byte
/// `i` of `bytes`. Returns Some((output_str, end_index_exclusive)) when
/// recognized, where `output_str` is the `&name;` (or `&#NNN;`) replacement.
///
/// Mirrors upstream `encodeEntities` (in `utils.ts`) followed by
/// `decodeEntities` (in `mermaidAPI.ts`): `#word;` round-trips to `&word;`
/// in the rendered SVG. Numeric `#NNN;` becomes `&#NNN;`.
fn try_consume_hash_entity(bytes: &[u8], i: usize) -> Option<(String, usize)> {
    if bytes.get(i)? != &b'#' {
        return None;
    }
    let start = i + 1;
    let mut end = start;
    while end < bytes.len() && bytes[end] != b';' {
        // Stop early on whitespace / `<>&#` — the regex in upstream is
        // `#\w+;`, so we restrict to ASCII word chars (alnum + `_`).
        let b = bytes[end];
        if !b.is_ascii_alphanumeric() && b != b'_' {
            return None;
        }
        end += 1;
    }
    if end >= bytes.len() || end == start {
        return None;
    }
    let name = std::str::from_utf8(&bytes[start..end]).ok()?;
    let is_num = name.bytes().all(|b| b.is_ascii_digit());
    let replacement = if is_num {
        format!("&#{};", name)
    } else {
        format!("&{};", name)
    };
    Some((replacement, end + 1))
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'&' => {
                out.push_str("&amp;");
                i += 1;
            }
            b'<' => {
                out.push_str("&lt;");
                i += 1;
            }
            b'>' => {
                out.push_str("&gt;");
                i += 1;
            }
            b'#' => {
                if let Some((rep, next)) = try_consume_hash_entity(bytes, i) {
                    out.push_str(&rep);
                    i = next;
                } else {
                    out.push('#');
                    i += 1;
                }
            }
            _ if b < 0x80 => {
                out.push(b as char);
                i += 1;
            }
            _ => {
                // Multi-byte UTF-8: copy through to maintain validity.
                let ch_len = utf8_char_len(b);
                let ch_end = (i + ch_len).min(bytes.len());
                out.push_str(&s[i..ch_end]);
                i = ch_end;
            }
        }
    }
    out
}

fn attr_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'&' => {
                out.push_str("&amp;");
                i += 1;
            }
            b'<' => {
                out.push_str("&lt;");
                i += 1;
            }
            b'>' => {
                out.push_str("&gt;");
                i += 1;
            }
            b'"' => {
                out.push_str("&quot;");
                i += 1;
            }
            b'#' => {
                if let Some((rep, next)) = try_consume_hash_entity(bytes, i) {
                    out.push_str(&rep);
                    i = next;
                } else {
                    out.push('#');
                    i += 1;
                }
            }
            _ if b < 0x80 => {
                out.push(b as char);
                i += 1;
            }
            _ => {
                let ch_len = utf8_char_len(b);
                let ch_end = (i + ch_len).min(bytes.len());
                out.push_str(&s[i..ch_end]);
                i = ch_end;
            }
        }
    }
    out
}

/// UTF-8 leading-byte → byte length lookup. Defaults to 1 for invalid
/// leads so we always make forward progress.
fn utf8_char_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b < 0xC0 {
        1 // continuation byte (shouldn't happen at boundary)
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    }
}

/// Fallback placeholder for fixtures we can't render byte-exactly. Used
/// purely so the dispatch table compiles cleanly — the corresponding
/// fixture stays in `tests/known_ignored.txt`.
fn placeholder(d: &SequenceDiagram, id: &str) -> String {
    let _ = d;
    format!(
        "<svg id=\"{id}\" width=\"100%\" xmlns=\"http://www.w3.org/2000/svg\" \
         viewBox=\"0 0 100 100\" \
         role=\"graphics-document document\" aria-roledescription=\"sequence\"></svg>"
    )
}

