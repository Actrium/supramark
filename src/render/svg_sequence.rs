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
    /// 1-based actor counter as upstream's `actorCnt`. Kept for
    /// future fixtures that need self-message / activation IDs.
    cnt: usize,
    /// Popup-menu entries from `link`/`links` directives.
    links: Vec<(String, String)>,
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
    arrow: ArrowType,
    line_start_y: f64,
    text_x: f64,
    /// Y of the FIRST text line. Subsequent lines step by `line_step`.
    text_y_first: f64,
    line_step: f64,
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
    rect_x: f64,
    rect_y: f64,
    rect_w: f64,
    rect_h: f64,
    text_x: f64,
    /// Y of the first text line. Subsequent lines step by `line_step`.
    text_y_first: f64,
    line_step: f64,
    /// 0-based item index — used as `data-id="iN"`.
    idx: usize,
}

const FONT_FAMILY: &str = "\"trebuchet ms\", verdana, arial";
const ACTOR_FONT_FAMILY: &str = "\"trebuchet ms\", verdana, arial";

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
                // Reject implicit `+` / `-` activations — full activation
                // rendering is not yet ported. Central-connection auto-
                // activate (`AtTo`/`Dual` set `activate=true` in the parser)
                // is fine: the renderer applies the upstream stopx shorten
                // without drawing an actual activation rect.
                if m.deactivate {
                    return false;
                }
                if m.activate && m.central_connection.is_none() {
                    return false;
                }
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
                n.placement_actors.len() == 1
                    && n.placement.is_some()
                    && !n.text.contains('\n')
            }
            // Autonumber occupies an item-id slot and toggles per-message
            // sequence-number rendering — supported below.
            DiagramItem::Autonumber { .. } => true,
            _ => false,
        })
    }
    if !only_supported_items(&d.items) {
        return Ok(placeholder(d, id));
    }
    // No `box`, no created/destroyed actors.
    if !d.boxes.is_empty() {
        return Ok(placeholder(d, id));
    }
    if d.actors.iter().any(|a| a.created || a.destroyed) {
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
            let line_h = crate::font_metrics::line_height(
                "\"trebuchet ms\", verdana, arial",
                16.0,
                false,
                false,
            )
            .round();
            let actdims_h = line_h * (lines.len() as f64);
            let width = if wrap {
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
    for it in &d.items {
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

    // X positions: x_0 = 0; x_{i+1} = x_i + actor.width_i + actor.margin_i.
    let mut xs: Vec<f64> = Vec::with_capacity(n_actors);
    {
        let mut cursor = 0.0_f64;
        for i in 0..n_actors {
            xs.push(cursor);
            cursor += actor_widths[i] + actor_margins[i];
        }
    }
    let actors: Vec<ActorRender> = d
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
            cnt: i + 1,
            links: a.links.clone(),
        })
        .collect();

    // Vertical pass: emulate boundMessage on each message.
    // Initial: vertical = 0, then bumpVerticalPos(actor_h) → vertical = actor_h.
    let mut vertical = actor_h;
    let line_height = compute_message_line_height(cfg.message_font_size as f64);

    let mut messages: Vec<MsgRender> = Vec::new();
    let mut notes: Vec<NoteRender> = Vec::new();
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
    // `msg_id_counter` mirrors upstream's `messages.length` at the time
    // each event is pushed — both real messages AND synthetic
    // `centralConnection` / `centralConnectionReverse` signals consume
    // one slot. So a message with `Dual` is followed by +2 synthetic
    // events, advancing the counter by 3 total per Dual addMessage.
    let mut msg_id_counter: usize = 0;
    for (_orig_idx, item) in d.items.iter().enumerate() {
        let idx = msg_id_counter;
        msg_id_counter += 1;
        if let DiagramItem::Message(m) = item {
            if let Some(cc) = m.central_connection {
                msg_id_counter += match cc {
                    CentralConnection::AtTo | CentralConnection::AtFrom => 1,
                    CentralConnection::Dual => 2,
                };
            }
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
            let text_h = (lh_unrounded * (note_lines.len() as f64)).round();
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

            notes.push(NoteRender {
                lines: note_lines.iter().map(|s| s.to_string()).collect(),
                rect_x: note_x,
                rect_y: starty_for_note,
                rect_w: note_w,
                rect_h: note_h,
                text_x,
                text_y_first,
                line_step: lh_unrounded,
                idx,
            });

            // Update overall bounds.
            if note_x < bounds_startx {
                bounds_startx = note_x;
            }
            if note_x + note_w > bounds_stopx {
                bounds_stopx = note_x + note_w;
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
        let n_lines = msg_lines.len() as f64;
        let text_dims_height = line_height * n_lines;
        let starty_for_msg = vertical;
        vertical += 10.0;
        vertical += line_height;
        let is_self = m.from == m.to;

        // startx / stopx: standard left→right for SolidArrow → arrow_end shrinks by 3.
        let from_left = fa.x + fa.width / 2.0 - 1.0;
        let from_right = fa.x + fa.width / 2.0 + 1.0;
        let to_left = ta.x + ta.width / 2.0 - 1.0;
        let to_right = ta.x + ta.width / 2.0 + 1.0;
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

        if is_self {
            let mut msg_text_width = 0.0_f64;
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
            let dx = (msg_text_width / 2.0).max(default_actor_w / 2.0);
            let self_startx = startx - dx;
            let self_stopx = startx + dx;
            if self_startx < bounds_startx {
                bounds_startx = self_startx;
            }
            if self_stopx > bounds_stopx {
                bounds_stopx = self_stopx;
            }
        }

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
        let fa_cx = fa.x + fa.width / 2.0;
        let ta_cx = ta.x + ta.width / 2.0;
        let from_bounds = (fa_cx - 1.0).min(ta_cx - 1.0);
        let to_bounds = (fa_cx + 1.0).max(ta_cx + 1.0);
        let seq_x = if is_arrow_to_right {
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
            } else {
                startx + 6.0
            }
        } else {
            startx
        };
        let line_x2 = stopx;
        if seq_index.is_some() {
            auto_seq_index += auto_seq_step;
        }

        // Central-connection circle offset when autonumber is on.
        // Mirrors upstream `drawCentralConnection`
        // (sequenceRenderer.ts:329-372): with autonumber visible, the
        // CIRCLE offset 16.5 shifts whichever endpoint is the source
        // for non-reverse arrows. AtTo: no shift; AtFrom: fromCenter
        // shifts by ±16.5 (LTR=+, RTL=-); Dual: fromCenter only.
        // `isReverse` here = top/bottom-reverse arrow types
        // (`/|-`, `//-`, `\\-`, etc.) — none of which are emitted from
        // the current parser, so isReverse is treated as false.
        let mut circle_from_cx = fa_cx;
        let circle_to_cx = ta_cx;
        if seq_index.is_some() {
            const CIRCLE_OFFSET: f64 = 16.5;
            let base = if is_arrow_to_right {
                CIRCLE_OFFSET
            } else {
                -CIRCLE_OFFSET
            };
            match m.central_connection {
                Some(CentralConnection::AtFrom) | Some(CentralConnection::Dual) => {
                    circle_from_cx += base;
                }
                _ => {}
            }
        }

        messages.push(MsgRender {
            is_self,
            from: m.from.clone(),
            to: m.to.clone(),
            lines: msg_lines.iter().map(|s| s.to_string()).collect(),
            arrow: m.arrow.unwrap_or(ArrowType::SolidArrow),
            line_start_y,
            text_x,
            text_y_first,
            line_step,
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
    let max_actor_height = d
        .actors
        .iter()
        .map(|a| match a.actor_type {
            ActorType::Control => 84.0_f64,
            _ => actor_h,
        })
        .fold(actor_h, f64::max);
    let (bottom_y, box_stopy) = if mirror {
        let by = vertical + box_margin * 2.0;
        let stopy = by + max_actor_height + box_margin;
        (by, stopy)
    } else {
        (vertical, vertical)
    };

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
        "\" role=\"graphics-document document\" aria-roledescription=\"sequence\">",
    );

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
    if mirror {
        for a in actors.iter().rev() {
            match a.actor_type {
                ActorType::Participant => emit_actor_bottom_participant(&mut out, a, bottom_y),
                // Actor / Boundary / Control / Entity all emit empty
                // <g></g> placeholder for their bottom group's `line2`
                // (lowered to front). Bodies emit later, after defs.
                ActorType::Actor
                | ActorType::Boundary
                | ActorType::Control => out.push_str("<g></g>"),
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
    let lifeline_y2 = if mirror { bottom_y } else { 2000.0 };
    let force_menus = d.config.force_menus;
    for (rank, a) in actors.iter().rev().enumerate() {
        // Upstream `drawActorTypeParticipant` / `drawActorTypeActor`:
        //   if (Object.keys(actor.links || {}).length && !conf.forceMenus) {
        //     g.attr("onclick", popupMenuToggle(...)).attr("cursor", "pointer");
        //   }
        // i.e. the per-actor onclick wrapper is suppressed when forceMenus
        // is set — instead, the `<g id="actorN_popup">` block emits with
        // `display="block !important"` later (see emit_actor_popup).
        let popup = !a.links.is_empty() && !force_menus;
        match a.actor_type {
            ActorType::Participant => {
                emit_actor_top_participant(&mut out, a, lifeline_y2, rank, popup)
            }
            // Boundary uses the same lifeline-only top group as Actor — body
            // emits later after defs. Lifeline `<g><line id="actorN" .../></g>`
            // is identical to Actor (centerY = actor.height + 15 = 80).
            ActorType::Actor | ActorType::Boundary => {
                emit_actor_top_lifeline_actor(
                    &mut out,
                    a,
                    a.height + 15.0,
                    lifeline_y2,
                    rank,
                    popup,
                )
            }
            // Control uses centerY = actor_y + 75 (vs Actor's
            // actor.height + 15). Same `<g><line .../></g>` shape, body
            // emits later after defs.
            ActorType::Control => {
                emit_actor_top_lifeline_actor(&mut out, a, 75.0, lifeline_y2, rank, popup)
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

    // Central-connection activation anchors. Mirrors upstream's
    // `bounds.newActivation(...)` → `svgDraw.anchorElement(diagram)` →
    // `elem.append('g')` for each CENTRAL_CONNECTION /
    // CENTRAL_CONNECTION_REVERSE / CENTRAL_CONNECTION_DUAL — these
    // produce empty `<g></g>` placeholders right after `</defs>`,
    // before the message text/line/circle groups.
    //   AtTo (`actor signal '()' actor`)         → 1 anchor (at `to`)
    //   AtFrom (`actor '()' signal actor`)       → 1 anchor (at `from`)
    //   Dual (`actor '()' signal '()' actor`)    → 2 anchors
    // Per the jison grammar @ sequenceDiagram.jison:340-352.
    for m in &messages {
        if let Some(cc) = m.central_connection {
            let n = match cc {
                CentralConnection::AtTo | CentralConnection::AtFrom => 1,
                CentralConnection::Dual => 2,
            };
            for _ in 0..n {
                out.push_str("<g></g>");
            }
        }
    }

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

    // Notes — emitted in iteration order, BEFORE messages. Upstream
    // calls `drawNote` inline during the message loop, but message
    // shapes are batched in `messagesToDraw` and emitted later.
    //
    // Notes are also emitted BEFORE Actor-type top bodies because in
    // upstream the top `drawActors` runs AFTER the message loop —
    // `actElem = elem.append('g')` (no `.lower()`) for Actor body, so
    // it's appended at the END of `elem.children`, after the notes
    // that were appended during the message loop.
    for n in &notes {
        emit_note(&mut out, n);
    }

    // Top bodies, declaration order, only for Actor / Boundary / Control.
    for (i, a) in actors.iter().enumerate() {
        match a.actor_type {
            ActorType::Actor => {
                let (torso_id, arms_id) = stick_ids.top[i];
                emit_actor_man_body(&mut out, a, 0.0, false, torso_id, arms_id);
            }
            ActorType::Boundary => {
                let (torso_id, arms_id) = stick_ids.top[i];
                emit_actor_boundary_body(&mut out, a, 0.0, false, torso_id, arms_id);
            }
            ActorType::Control => {
                emit_actor_control_body(&mut out, a, 0.0, false, id);
            }
            _ => continue,
        }
    }

    // Messages — text + line for each, in declaration order.
    for m in &messages {
        emit_message(&mut out, id, m);
    }

    // Bottom bodies, declaration order, only for Actor / Boundary /
    // Control — and only when mirroring.
    if mirror {
        for (i, a) in actors.iter().enumerate() {
            match a.actor_type {
                ActorType::Actor => {
                    let (torso_id, arms_id) = stick_ids.bottom[i];
                    emit_actor_man_body(&mut out, a, bottom_y, true, torso_id, arms_id);
                }
                ActorType::Boundary => {
                    let (torso_id, arms_id) = stick_ids.bottom[i];
                    emit_actor_boundary_body(&mut out, a, bottom_y, true, torso_id, arms_id);
                }
                ActorType::Control => {
                    emit_actor_control_body(&mut out, a, bottom_y, true, id);
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
        emit_actor_popup(&mut out, a, rank, force_menus);
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

/// Emit one `<g data-et="note" data-id="iN">` containing a rounded
/// rect + one `<text><tspan>` per `<br>`-split line. Mirrors upstream
/// `drawNote` (`drawText` with `tspan: true`, `valign='center'`).
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
    for (i, line_text) in n.lines.iter().enumerate() {
        let y = round_js(n.text_y_first + (i as f64) * n.line_step);
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
    let lines = split_br(description);
    let n = lines.len();
    let font_size = 16.0_f64;
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
}

fn emit_actor_bottom_participant(out: &mut String, a: &ActorRender, bottom_y: f64) {
    out.push_str("<g><rect x=\"");
    push_num(out, a.x);
    out.push_str("\" y=\"");
    push_num(out, bottom_y);
    out.push_str("\" fill=\"#eaeaea\" stroke=\"#666\" width=\"");
    push_num(out, a.width);
    out.push_str("\" height=\"");
    push_num(out, a.height);
    out.push_str("\" name=\"");
    out.push_str(&xml_escape(&a.id));
    out.push_str("\" rx=\"3\" ry=\"3\" class=\"actor actor-bottom\"></rect>");
    let cx = a.x + a.width / 2.0;
    let cy = bottom_y + a.height / 2.0;
    emit_actor_box_text(out, cx, cy, &a.description);
    out.push_str("</g>");
}

fn emit_actor_top_participant(out: &mut String, a: &ActorRender, bottom_y: f64, rank: usize, popup: bool) {
    let _ = a.cnt;
    let cx = a.x + a.width / 2.0;
    let centery = a.height; // actorY=0 + actor.height
    let top_y = 0.0;
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
    out.push_str(&rank.to_string());
    out.push_str(
        "\" data-et=\"participant\" data-type=\"participant\" data-id=\"",
    );
    out.push_str(&xml_escape(&a.id));
    out.push_str("\"><rect x=\"");
    push_num(out, a.x);
    out.push_str("\" y=\"");
    push_num(out, top_y);
    out.push_str("\" fill=\"#eaeaea\" stroke=\"#666\" width=\"");
    push_num(out, a.width);
    out.push_str("\" height=\"");
    push_num(out, a.height);
    out.push_str("\" name=\"");
    out.push_str(&xml_escape(&a.id));
    out.push_str("\" rx=\"3\" ry=\"3\" class=\"actor actor-top\"></rect>");
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
fn emit_actor_popup(out: &mut String, a: &ActorRender, rank: usize, force_menus: bool) {
    let panel_x = a.x;
    let panel_y = a.height; // actor box bottom in the top group.
    let n_links = a.links.len() as f64;
    let panel_h = 20.0 + 30.0 * n_links;
    let display = if force_menus { "block !important" } else { "none" };

    out.push_str("<g id=\"actor");
    out.push_str(&rank.to_string());
    out.push_str("_popup\" class=\"actorPopupMenu\" display=\"");
    out.push_str(display);
    out.push_str("\"><rect class=\"actorPopupMenuPanel actor actor-bottom\" x=\"");
    push_num(out, panel_x);
    out.push_str("\" y=\"");
    push_num(out, panel_y);
    out.push_str("\" fill=\"#eaeaea\" stroke=\"#666\" width=\"");
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
    for (n, line_text) in m.lines.iter().enumerate() {
        let y = round_js(m.text_y_first + (n as f64) * m.line_step);
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

    // <line> or <path> next.
    if m.is_self {
        let lsx = m.self_line_start_x;
        let sx = m.self_startx;
        let lsy = m.line_start_y;
        out.push_str("<path d=\"M ");
        push_num(out, lsx);
        out.push(',');
        push_num(out, lsy);
        out.push_str(" C ");
        push_num(out, lsx + 60.0);
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

