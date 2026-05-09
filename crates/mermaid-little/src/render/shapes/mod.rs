//! Per-shape drawers — Rust port of
//! upstream `packages/mermaid/src/rendering-util/rendering-elements/shapes/*.ts`.
//!
//! # Organisation
//!
//! * [`types`] — shared helpers (numeric formatting, path builders,
//!   polygon emitter, label measurement).
//! * One `pub mod` per upstream shape, named in Rust snake_case.
//!   Each exports a `draw(node: &Node, theme: &ThemeVariables) ->
//!   Result<String>` that emits a full `<g class="node ..."><…/>…</g>`
//!   block using the node's post-layout `width`/`height`/`x`/`y`.
//! * The [`draw`] dispatcher accepts a shape ID string (as used in
//!   upstream `node.shape`) and routes to the right module. Unknown
//!   IDs return [`crate::error::MermaidError::Unsupported`].
//!
//! # Byte-exactness
//!
//! Every shape that's ported produces its SVG with the same
//! attribute order and numeric formatting as upstream's D3 → SVG
//! serialisation. `types::fmt_num` handles the JS `Number.toString`
//! convention (integers without `.0`, fractions shortest-decimal).
//!
//! # Wave 4 coverage
//!
//! The following ~25 shapes are implemented (the rest return
//! `Unsupported` for now and will be filled in as Wave 5+ diagrams
//! arrive):
//!
//! * Flowchart: rect, round, rect_left_inv_arrow, lean_right, lean_left,
//!   trapezoid, inv_trapezoid, hexagon, stadium, cylinder,
//!   rect_with_title, doublecircle, ellipse, diamond, circle,
//!   subroutine, note.
//! * Class: classBox.
//! * State: state (= rounded rect), stateStart, stateEnd, forkJoin,
//!   choice, note.
//! * ER: erBox.
//! * Requirement: requirementBox.
//! * Block: basic, labelRect.
//!
//! Hand-drawn ("handDrawn" / `rough.js`) variants are deferred — those
//! use RNG jitter which can't be byte-compatible without porting
//! roughjs's PRNG + vertex-walk.

pub mod clusters;
pub mod types;

// ─── Implemented shapes ────────────────────────────────────────────
pub mod basic;
pub mod choice;
pub mod circle;
pub mod classbox;
pub mod cylinder;
pub mod diamond;
pub mod doublecircle;
pub mod ellipse;
pub mod erbox;
pub mod fork_join;
pub mod hexagon;
pub mod icon;
pub mod inv_trapezoid;
pub mod label_rect;
pub mod lean_left;
pub mod lean_right;
pub mod note;
pub mod rect;
pub mod rect_left_inv_arrow;
pub mod rect_with_title;
pub mod requirementbox;
pub mod round;
pub mod stadium;
pub mod state_end;
pub mod state_start;
pub mod subroutine;
pub mod trapezoid;

// Alias modules — same entry names as upstream; delegate to primary impl.
/// `state` diagram-family's generic node shape: rounded rect with `rx=5`.
pub mod state {
    pub fn draw(
        node: &crate::layout::unified::types::Node,
        theme: &crate::theme::ThemeVariables,
    ) -> crate::error::Result<String> {
        super::round::draw(node, theme)
    }
}

/// `squareRect` — synonym for plain rect.
pub mod square_rect {
    pub fn draw(
        node: &crate::layout::unified::types::Node,
        theme: &crate::theme::ThemeVariables,
    ) -> crate::error::Result<String> {
        super::rect::draw(node, theme)
    }
}

use crate::error::{MermaidError, Result};
use crate::layout::unified::types::Node;
use crate::theme::ThemeVariables;

/// Shape registry. Maps the string ID used in `Node::shape` to the
/// per-module draw function.
///
/// The ID values mirror upstream `rendering-util/rendering-elements/shapes.ts`'s
/// `shapes` record keys — e.g. `"rect"`, `"rounded"`, `"stadium"`, ….
/// Some common synonyms are accepted too (e.g. `"circle"` / `"circ"`).
pub fn draw(shape_id: &str, node: &Node, theme: &ThemeVariables) -> Result<String> {
    match shape_id {
        // Rectangles.
        "rect" | "square" | "squareRect" => rect::draw(node, theme),
        "round" | "rounded" => round::draw(node, theme),
        "rect_left_inv_arrow" | "lean-right-arrow" => rect_left_inv_arrow::draw(node, theme),
        "rect_with_title" | "rectWithTitle" => rect_with_title::draw(node, theme),
        "labelRect" | "label_rect" | "label" => label_rect::draw(node, theme),
        "basic" => basic::draw(node, theme),

        // Parallelograms / trapezoids.
        "lean_right" | "lean-right" => lean_right::draw(node, theme),
        "lean_left" | "lean-left" => lean_left::draw(node, theme),
        "trapezoid" | "trap" => trapezoid::draw(node, theme),
        "inv_trapezoid" | "invertedTrapezoid" | "invtrap" => inv_trapezoid::draw(node, theme),

        // Polygons.
        "hexagon" | "hex" => hexagon::draw(node, theme),
        "diamond" | "question" | "decision" => diamond::draw(node, theme),
        "stadium" | "pill" => stadium::draw(node, theme),

        // Curves.
        "cylinder" | "cyl" => cylinder::draw(node, theme),
        "ellipse" => ellipse::draw(node, theme),
        "circle" | "circ" => circle::draw(node, theme),
        "doublecircle" | "doubleCircle" => doublecircle::draw(node, theme),
        "icon" => icon::draw(node, theme),

        // Subroutine / note.
        "subroutine" => subroutine::draw(node, theme),
        "note" => note::draw(node, theme),

        // State diagram.
        "state" => state::draw(node, theme),
        "stateStart" | "state_start" | "start" => state_start::draw(node, theme),
        "stateEnd" | "state_end" | "end" => state_end::draw(node, theme),
        "forkJoin" | "fork_join" | "fork" | "join" => fork_join::draw(node, theme),
        "choice" => choice::draw(node, theme),

        // Class diagram.
        "classBox" | "class_box" | "class" => classbox::draw(node, theme),

        // ER diagram.
        "erBox" | "er_box" | "entity" => erbox::draw(node, theme),

        // Requirement diagram.
        "requirementBox" | "requirement_box" | "requirement" => requirementbox::draw(node, theme),

        // Upstream shapes deferred to later waves — explicit list so
        // the error message differentiates "known but not yet
        // implemented" from "unknown".
        "anchor"
        | "bang"
        | "bowTieRect"
        | "card"
        | "cloud"
        | "crossedCircle"
        | "curlyBraceLeft"
        | "curlyBraceRight"
        | "curlyBraces"
        | "curvedTrapezoid"
        | "defaultMindmapNode"
        | "dividedRect"
        | "document"
        | "filledCircle"
        | "flippedTriangle"
        | "halfRoundedRectangle"
        | "hourglass"
        | "iconCircle"
        | "iconRounded"
        | "iconSquare"
        | "imageSquare"
        | "kanbanItem"
        | "lightningBolt"
        | "linedCylinder"
        | "linedWaveEdgedRect"
        | "mindmapCircle"
        | "multiRect"
        | "multiWaveEdgedRectangle"
        | "shadedProcess"
        | "slopedRect"
        | "taggedRect"
        | "taggedWaveEdgedRectangle"
        | "text"
        | "tiltedCylinder"
        | "trapezoidalPentagon"
        | "triangle"
        | "waveEdgedRectangle"
        | "waveRectangle"
        | "windowPane" => Err(MermaidError::Unsupported(format!(
            "shape '{}' (deferred)",
            shape_id
        ))),

        _ => Err(MermaidError::Unsupported(format!("shape '{}'", shape_id))),
    }
}

/// Check whether a shape ID is known to the registry — implemented or deferred.
/// Returns `(known, implemented)`.
pub fn status(shape_id: &str) -> (bool, bool) {
    match draw(shape_id, &Node::default(), &ThemeVariables::default()) {
        Ok(_) => (true, true),
        Err(MermaidError::Unsupported(msg)) => (msg.contains("(deferred)"), false),
        Err(_) => (false, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_rejects_unknown_shapes() {
        let n = Node::default();
        let theme = ThemeVariables::default();
        assert!(matches!(
            draw("nope-not-a-shape", &n, &theme),
            Err(MermaidError::Unsupported(_))
        ));
    }

    #[test]
    fn registry_marks_deferred_shapes() {
        let n = Node::default();
        let theme = ThemeVariables::default();
        let err = draw("cloud", &n, &theme).unwrap_err();
        if let MermaidError::Unsupported(msg) = err {
            assert!(msg.contains("deferred"));
        } else {
            panic!("expected Unsupported");
        }
    }

    #[test]
    fn registry_dispatches_implemented_shapes() {
        let mut n = Node::default();
        n.id = "n".into();
        n.width = Some(40.0);
        n.height = Some(20.0);
        let theme = ThemeVariables::default();
        assert!(draw("rect", &n, &theme).is_ok());
        assert!(draw("stadium", &n, &theme).is_ok());
        assert!(draw("hexagon", &n, &theme).is_ok());
        assert!(draw("diamond", &n, &theme).is_ok());
    }

    #[test]
    fn status_reports_deferred_correctly() {
        assert_eq!(status("rect"), (true, true));
        assert_eq!(status("cloud"), (true, false));
        assert_eq!(status("nope"), (false, false));
    }
}
