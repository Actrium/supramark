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
use crate::model::sequence::{ActorType, ArrowType, DiagramItem, SequenceDiagram};
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
}

/// Information collected per-message for the render pass.
#[derive(Debug, Clone)]
struct MsgRender {
    from: String,
    to: String,
    text: String,
    arrow: ArrowType,
    starty: f64,
    line_start_y: f64,
    text_x: f64,
    text_y: f64,
    line_x1: f64,
    line_x2: f64,
    /// 0-based message index — upstream uses `i0`, `i1`, … as the
    /// `data-id` value.
    idx: usize,
}

const FONT_FAMILY: &str = "\"trebuchet ms\", verdana, arial";
const ACTOR_FONT_FAMILY: &str = "\"trebuchet ms\", verdana, arial";

pub fn render(
    d: &SequenceDiagram,
    _l: &SequenceLayout,
    _theme: &Theme,
    id: &str,
) -> Result<String> {
    // ── Eligibility gate ────────────────────────────────────────────
    //
    // Byte-exact path covers ONLY the simplest scaffold: every actor is
    // `Participant` (rectangle), every item is a single solid-arrow
    // message (`->>` → SolidArrow). Anything else is too feature-rich
    // for this skeleton and falls back to a placeholder SVG that is not
    // byte-exact (the fixture stays in `tests/known_ignored.txt`).
    if !d
        .actors
        .iter()
        .all(|a| matches!(a.actor_type, ActorType::Participant))
    {
        return Ok(placeholder(d, id));
    }
    // Reject any non-message item or non-SolidArrow message.
    fn only_solid_msgs(items: &[DiagramItem]) -> bool {
        items.iter().all(|it| match it {
            DiagramItem::Message(m) => matches!(m.arrow, Some(ArrowType::SolidArrow)),
            _ => false,
        })
    }
    if !only_solid_msgs(&d.items) {
        return Ok(placeholder(d, id));
    }
    // No `box`, no created/destroyed actors.
    if !d.boxes.is_empty() {
        return Ok(placeholder(d, id));
    }
    if d.actors.iter().any(|a| a.created || a.destroyed) {
        return Ok(placeholder(d, id));
    }
    // Need at least one actor and at least one message.
    if d.actors.is_empty() || d.items.is_empty() {
        return Ok(placeholder(d, id));
    }

    // ── Layout (mirrors upstream addActorRenderingData + boundMessage)
    let cfg = &d.config;
    let actor_w = cfg.width;
    let actor_h = cfg.height;
    let actor_margin = cfg.actor_margin;
    let box_margin = cfg.box_margin;
    let bottom_margin_adj = cfg.bottom_margin_adj;
    let dia_margin_x = cfg.diagram_margin_x;
    let dia_margin_y = cfg.diagram_margin_y;

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
        let DiagramItem::Message(m) = it else { continue };
        let (Some(&from_i), Some(&to_i)) = (
            actor_id_to_index.get(m.from.as_str()),
            actor_id_to_index.get(m.to.as_str()),
        ) else {
            continue;
        };
        let msg_text_width = crate::font_metrics::text_width(
            &m.text,
            "sans-serif",
            cfg.message_font_size as f64,
            false,
            false,
        )
        .round();
        let message_width = msg_text_width + 2.0 * cfg.wrap_padding;

        if from_i == to_i {
            // self-message — both halves
            let half = message_width / 2.0;
            if max_msg_width_per_actor[from_i] < half {
                max_msg_width_per_actor[from_i] = half;
            }
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
        let half_self = actor_w / 2.0;
        let m = if let Some(_n) = next_actor_of[i] {
            mw + actor_margin - half_self - actor_w / 2.0
        } else {
            mw + actor_margin - half_self
        };
        actor_margins[i] = m.max(actor_margin);
    }

    // X positions: x_0 = 0; x_{i+1} = x_i + actor_w + actor.margin_i.
    let mut xs: Vec<f64> = Vec::with_capacity(n_actors);
    {
        let mut cursor = 0.0_f64;
        for am in actor_margins.iter().take(n_actors) {
            xs.push(cursor);
            cursor += actor_w + am;
        }
    }
    let actors: Vec<ActorRender> = d
        .actors
        .iter()
        .enumerate()
        .map(|(i, a)| ActorRender {
            id: a.id.clone(),
            description: a.description.clone(),
            actor_type: a.actor_type.clone(),
            x: xs[i],
            width: actor_w,
            height: actor_h,
            cnt: i + 1,
        })
        .collect();

    // Vertical pass: emulate boundMessage on each message.
    // Initial: vertical = 0, then bumpVerticalPos(actor_h) → vertical = actor_h.
    let mut vertical = actor_h;
    let line_height = compute_message_line_height(cfg.message_font_size as f64);

    let mut messages: Vec<MsgRender> = Vec::new();
    for (idx, item) in d.items.iter().enumerate() {
        let m = match item {
            DiagramItem::Message(m) => m,
            _ => continue,
        };
        // boundMessage:
        //   bumpVerticalPos(10)
        //   bumpVerticalPos(lineHeight)
        //   totalOffset = (lineHeight - 10) + boxMargin       (non-self)
        //                = lineHeight (when boxMargin == 10)
        //   lineStartY = vertical + totalOffset
        //   bumpVerticalPos(totalOffset)
        let starty_for_msg = vertical;
        vertical += 10.0;
        vertical += line_height;
        let total_offset = (line_height - 10.0) + box_margin;
        let line_start_y = vertical + total_offset;
        vertical += total_offset;

        // startx / stopx: standard left→right for SolidArrow → arrow_end shrinks by 3.
        let from_actor = actors.iter().find(|a| a.id == m.from);
        let to_actor = actors.iter().find(|a| a.id == m.to);
        let (Some(fa), Some(ta)) = (from_actor, to_actor) else {
            return Ok(placeholder(d, id));
        };
        let from_left = fa.x + fa.width / 2.0 - 1.0;
        let from_right = fa.x + fa.width / 2.0 + 1.0;
        let to_left = ta.x + ta.width / 2.0 - 1.0;
        let to_right = ta.x + ta.width / 2.0 + 1.0;
        let is_arrow_to_right = from_left <= to_left;
        let startx = if is_arrow_to_right {
            from_right
        } else {
            from_left
        };
        let mut stopx = if is_arrow_to_right { to_left } else { to_right };
        // Solid filled-arrow: shorten end by 3 in the arrow's direction.
        if is_arrow_to_right {
            stopx -= 3.0;
        } else {
            stopx += 3.0;
        }
        // Self-message — upstream sets stopx = startx.
        if m.from == m.to {
            stopx = startx;
        }

        // Text positioning (upstream drawText with anchor='center', valign='center', textMargin=10):
        //   x' = round(textObj.x + textObj.width / 2) where textObj.x = startx, textObj.width = stopx - startx
        //   y' = round(textObj.y + (0 + 0 + 10) / 2) = round((starty + 10) + 5)
        //   In d3 valign='center' uses prevTextHeight + textHeight + textMargin / 2
        //   = (0 + 0 + 10) / 2 = 5. So y' = round(starty + 10 + 5) = round(starty + 15).
        let text_x = round_js((startx + stopx) / 2.0);
        let text_y = round_js(starty_for_msg + 10.0 + 5.0);

        let line_x1 = startx;
        let line_x2 = stopx;

        messages.push(MsgRender {
            from: m.from.clone(),
            to: m.to.clone(),
            text: m.text.clone(),
            arrow: m.arrow.unwrap_or(ArrowType::SolidArrow),
            starty: starty_for_msg,
            line_start_y,
            text_x,
            text_y,
            line_x1,
            line_x2,
            idx,
        });
        // (height/stopy bookkeeping not needed since we only use vertical)
    }
    let _ = bottom_margin_adj;
    let _ = box_margin;

    // After last message: drawActors(true) preamble → bumpVerticalPos(boxMargin*2)
    vertical += box_margin * 2.0;
    let bottom_y = vertical;

    // bumpVerticalPos(maxHeight + boxMargin) — feeds box.stopy.
    let box_stopy = bottom_y + actor_h + box_margin;

    // ── viewBox + size ──────────────────────────────────────────────
    // upstream:
    //   width = boxWidth + 2 * diagramMarginX
    //   height = boxHeight + 2 * diagramMarginY - boxMargin + bottomMarginAdj   (mirrorActors=true)
    // boxWidth = box.stopx - box.startx; with no boxes this is `last actor's
    // right edge - 0` = (n - 1) * (actor_w + actor_margin) + actor_w.
    let last_actor_x = actors.last().map(|a| a.x).unwrap_or(0.0);
    let box_width = last_actor_x + actor_w;
    let svg_width = box_width + 2.0 * dia_margin_x;
    let svg_height = (box_stopy - 0.0) + 2.0 * dia_margin_y - box_margin + bottom_margin_adj;
    let vb_x = -dia_margin_x;
    let vb_y = -dia_margin_y;

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
    push_num(&mut out, svg_height);
    out.push_str(
        "\" role=\"graphics-document document\" aria-roledescription=\"sequence\">",
    );

    // Bottom actor rects + text — REVERSE iteration (`.lower()` semantics).
    for a in actors.iter().rev() {
        emit_actor_bottom(&mut out, a, bottom_y);
    }
    // Top actor groups (lifeline + rect + text) — REVERSE iteration so
    // the LAST-declared actor reaches the DOM first (mirroring
    // upstream's `.lower()` semantics). Reference SVGs are then
    // post-processed by `generate_ref.mjs:normaliseSvg`, which renumbers
    // every `actorN` / `root-N` id by FIRST DOM-APPEARANCE — meaning
    // the actor we emit first gets renamed to `actor0`. So feed the
    // emit pass an enumerated rank in reverse iteration order.
    for (rank, a) in actors.iter().rev().enumerate() {
        emit_actor_top(&mut out, a, bottom_y, rank);
    }

    // Style + empty <g> placeholder.
    out.push_str("<style>");
    out.push_str(&consts::SEQUENCE_STYLE.replace("__ID__", id));
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

    // Messages — text + line for each, in declaration order.
    for m in &messages {
        emit_message(&mut out, id, m);
    }

    out.push_str("</svg>");
    Ok(out)
}

fn emit_actor_bottom(out: &mut String, a: &ActorRender, bottom_y: f64) {
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
    out.push_str(
        "\" rx=\"3\" ry=\"3\" class=\"actor actor-bottom\"></rect><text x=\"",
    );
    push_num(out, a.x + a.width / 2.0);
    out.push_str("\" y=\"");
    push_num(out, bottom_y + a.height / 2.0);
    out.push_str(
        "\" style=\"text-anchor: middle; font-size: 16px; font-weight: 400; font-family: ",
    );
    out.push_str(&attr_escape(ACTOR_FONT_FAMILY));
    out.push_str(
        ";\" dominant-baseline=\"central\" alignment-baseline=\"central\" class=\"actor actor-box\"><tspan x=\"",
    );
    push_num(out, a.x + a.width / 2.0);
    out.push_str("\" dy=\"0\">");
    out.push_str(&xml_escape(&a.description));
    out.push_str("</tspan></text></g>");
}

fn emit_actor_top(out: &mut String, a: &ActorRender, bottom_y: f64, rank: usize) {
    let _ = a.cnt;
    let cx = a.x + a.width / 2.0;
    let centery = a.height; // actorY=0 + actor.height
    let top_y = 0.0;
    out.push_str("<g><line id=\"actor");
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
    out.push_str(
        "\" rx=\"3\" ry=\"3\" class=\"actor actor-top\"></rect><text x=\"",
    );
    push_num(out, cx);
    out.push_str("\" y=\"");
    push_num(out, top_y + a.height / 2.0);
    out.push_str(
        "\" style=\"text-anchor: middle; font-size: 16px; font-weight: 400; font-family: ",
    );
    out.push_str(&attr_escape(ACTOR_FONT_FAMILY));
    out.push_str(
        ";\" dominant-baseline=\"central\" alignment-baseline=\"central\" class=\"actor actor-box\"><tspan x=\"",
    );
    push_num(out, cx);
    out.push_str("\" dy=\"0\">");
    out.push_str(&xml_escape(&a.description));
    out.push_str("</tspan></text></g></g>");
}

fn emit_message(out: &mut String, id: &str, m: &MsgRender) {
    // <text> first
    out.push_str("<text x=\"");
    push_num(out, m.text_x);
    out.push_str("\" y=\"");
    push_num(out, m.text_y);
    out.push_str(
        "\" text-anchor=\"middle\" dominant-baseline=\"middle\" alignment-baseline=\"middle\" style=\"font-family: ",
    );
    out.push_str(&attr_escape(FONT_FAMILY));
    out.push_str("; font-size: 16px; font-weight: 400;\" class=\"messageText\" dy=\"1em\">");
    out.push_str(&xml_escape(&m.text));
    out.push_str("</text>");

    // <line> next
    out.push_str("<line x1=\"");
    push_num(out, m.line_x1);
    out.push_str("\" y1=\"");
    push_num(out, m.line_start_y);
    out.push_str("\" x2=\"");
    push_num(out, m.line_x2);
    out.push_str("\" y2=\"");
    push_num(out, m.line_start_y);
    out.push_str("\" class=\"messageLine0\" data-et=\"message\" data-id=\"i");
    out.push_str(&m.idx.to_string());
    out.push_str("\" data-from=\"");
    out.push_str(&attr_escape(&m.from));
    out.push_str("\" data-to=\"");
    out.push_str(&attr_escape(&m.to));
    out.push_str(
        "\" stroke-width=\"2\" stroke=\"none\" style=\"fill: none;\" marker-end=\"url(#",
    );
    out.push_str(id);
    let marker = match m.arrow {
        ArrowType::SolidArrow | ArrowType::DottedArrow => "-arrowhead",
        _ => "-arrowhead",
    };
    out.push_str(marker);
    out.push_str(")\"></line>");
    let _ = m.starty;
}

/// Compute the bbox.height of a single line in the messageFont. Upstream's
/// `calculateTextDimensions` uses jsdom's `getBBox()`, which returns
/// `Math.round(line_height_px)` for a single ASCII line. `line_height_px`
/// comes from DejaVu Sans metrics: `(ascender + |descender|) / units_per_em
/// * font_size` — see [`crate::font_metrics::line_height`].
fn compute_message_line_height(font_size: f64) -> f64 {
    crate::font_metrics::line_height("sans-serif", font_size, false, false).round()
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

/// JS-compatible `Math.round` — rounds half-up (toward +∞ for halves of
/// positive numbers). Rust's `f64::round()` rounds half-away-from-zero,
/// which differs from JS for negative halves. Sequence diagrams have no
/// negative-half values in practice; use the simple form.
fn round_js(v: f64) -> f64 {
    // JS Math.round rounds .5 toward positive infinity; for the values
    // we touch here this matches `(v + 0.5).floor()` for non-negative
    // numbers.
    if v >= 0.0 {
        (v + 0.5).floor()
    } else {
        -((-v + 0.5).floor() - 1.0).max(0.0)
    }
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

fn attr_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
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

