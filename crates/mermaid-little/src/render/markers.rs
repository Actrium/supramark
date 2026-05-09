//! SVG `<marker>` catalog — edge decorations (arrow / diamond / cross / circle /
//! cardinality) used by flowchart, class, state, er, requirement, block, c4
//! diagrams.
//!
//! This module is a direct port of upstream mermaid-js:
//! `packages/mermaid/src/rendering-util/rendering-elements/markers.js`
//! (v11.14.0, 976 LoC). It emits byte-identical `<marker>` definitions so the
//! Wave 4 diagram renderers can embed them verbatim.
//!
//! The attribute order in every `<marker>` / child element mirrors the upstream
//! `.attr(...)` call chain exactly — re-arranging attributes would break the
//! byte-equal reference comparison.
//!
//! # Public surface
//! - [`defs`]: emits every marker a given diagram kind needs (upstream order).
//! - [`single`]: emits one marker family by name (useful for callers that know
//!   which markers they use).
//!
//! # Attribution
//! Attribute values, path `d` strings, viewBox numbers, and emit order come
//! from upstream mermaid-js (MIT, Knut Sveidqvist et al.). No non-trivial
//! geometry blocks were taken from mmdflux — mmdflux's `markers.rs` handles
//! edge-routing offsets, which is a separate concern from marker definitions.

use crate::theme::ThemeVariables;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Emit all `<marker>` defs needed by a given diagram `kind`.
///
/// `kind` is the diagram "type" string used by upstream (`"flowchart"`,
/// `"flowchart-v2"`, `"classDiagram"`, `"class"`, `"stateDiagram"`, `"er"`,
/// `"requirement"`, `"block"`, `"c4"`). `id_prefix` is the per-diagram unique
/// id (upstream calls this `diagramId` / `id`); the emitted marker ids follow
/// the pattern `{id_prefix}_{kind}-{name}` — identical to upstream.
///
/// Unknown `kind` values fall back to the flowchart marker set (point, circle,
/// cross), matching upstream's default behaviour for generic graph layouts.
pub fn defs(kind: &str, id_prefix: &str, theme: &ThemeVariables) -> String {
    let marker_names = marker_set_for(kind);
    let mut out = String::with_capacity(marker_names.len() * 512);
    for name in marker_names {
        append_family(&mut out, name, kind, id_prefix, theme);
    }
    out
}

/// Emit a single marker family by `name`. Returns `None` if the name is not
/// one of the known upstream marker families.
///
/// A "family" can emit multiple `<marker>` elements (e.g. `point` produces
/// `pointEnd`, `pointStart`, `pointEnd-margin`, `pointStart-margin`).
pub fn single(name: &str, kind: &str, id_prefix: &str, theme: &ThemeVariables) -> Option<String> {
    if !is_known_family(name) {
        return None;
    }
    let mut out = String::with_capacity(512);
    append_family(&mut out, name, kind, id_prefix, theme);
    Some(out)
}

// ---------------------------------------------------------------------------
// Diagram kind -> marker family list
// ---------------------------------------------------------------------------
//
// Source mapping (upstream file paths):
//   flowchart*         flowRenderer-v3-unified.ts      ['point', 'circle', 'cross']
//   block              blockRenderer.ts                ['point', 'circle', 'cross']
//   class*             classRenderer-v3-unified.ts     ['aggregation', 'extension', 'composition', 'dependency', 'lollipop']
//   state*             stateRenderer-v3-unified.ts     ['barb'] or ['barbNeo']
//   er                 erRenderer-unified.ts           ['only_one', 'zero_or_one', 'one_or_more', 'zero_or_more'] or neo variants
//   requirement        requirementRenderer.ts          ['requirement_contains', 'requirement_arrow'] or neo variants
//   mindmap            mindmapDb.ts                    ['point']
//   c4                 (uses flowchart renderer)       ['point', 'circle', 'cross']

fn marker_set_for(kind: &str) -> &'static [&'static str] {
    match kind {
        "flowchart" | "flowchart-v2" | "flowchart-elk" | "graph" | "c4" | "c4Diagram" | "block"
        | "blockDiagram" => &["point", "circle", "cross"],
        "classDiagram" | "class" | "classDiagram-v2" => &[
            "aggregation",
            "extension",
            "composition",
            "dependency",
            "lollipop",
        ],
        "stateDiagram" | "stateDiagram-v2" | "state" => &["barb"],
        "er" | "erDiagram" => &["only_one", "zero_or_one", "one_or_more", "zero_or_more"],
        "requirement" | "requirementDiagram" => &["requirement_contains", "requirement_arrow"],
        "mindmap" => &["point"],
        // Unknown / generic: flowchart default
        _ => &["point", "circle", "cross"],
    }
}

fn is_known_family(name: &str) -> bool {
    matches!(
        name,
        "extension"
            | "composition"
            | "aggregation"
            | "dependency"
            | "lollipop"
            | "point"
            | "circle"
            | "cross"
            | "barb"
            | "barbNeo"
            | "only_one"
            | "zero_or_one"
            | "one_or_more"
            | "zero_or_more"
            | "only_one_neo"
            | "zero_or_one_neo"
            | "one_or_more_neo"
            | "zero_or_more_neo"
            | "requirement_arrow"
            | "requirement_contains"
            | "requirement_arrow_neo"
            | "requirement_contains_neo"
    )
}

fn append_family(out: &mut String, name: &str, kind: &str, id: &str, theme: &ThemeVariables) {
    match name {
        "extension" => extension(out, kind, id),
        "composition" => composition(out, kind, id),
        "aggregation" => aggregation(out, kind, id),
        "dependency" => dependency(out, kind, id),
        "lollipop" => lollipop(out, kind, id),
        "point" => point(out, kind, id),
        "circle" => circle(out, kind, id),
        "cross" => cross(out, kind, id),
        "barb" => barb(out, kind, id),
        "barbNeo" => barb_neo(out, kind, id, theme),
        "only_one" => only_one(out, kind, id),
        "zero_or_one" => zero_or_one(out, kind, id),
        "one_or_more" => one_or_more(out, kind, id),
        "zero_or_more" => zero_or_more(out, kind, id),
        "only_one_neo" => only_one_neo(out, kind, id, theme),
        "zero_or_one_neo" => zero_or_one_neo(out, kind, id, theme),
        "one_or_more_neo" => one_or_more_neo(out, kind, id, theme),
        "zero_or_more_neo" => zero_or_more_neo(out, kind, id, theme),
        "requirement_arrow" => requirement_arrow(out, kind, id),
        "requirement_contains" => requirement_contains(out, kind, id),
        "requirement_arrow_neo" => requirement_arrow_neo(out, kind, id, theme),
        "requirement_contains_neo" => requirement_contains_neo(out, kind, id, theme),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Theme fallback for `strokeWidth`. Upstream uses `themeVariables.strokeWidth`
/// directly (a number). When missing we fall back to `"1"` which matches
/// upstream default behaviour for the stroke-width CSS property.
fn stroke_width(theme: &ThemeVariables) -> String {
    theme
        .stroke_width
        .map(|v| v.to_string())
        .unwrap_or_else(|| "1".to_string())
}

/// Theme fallback for `mainBkg`; upstream uses `mainBkg ?? 'white'`.
fn main_bkg(theme: &ThemeVariables) -> String {
    theme
        .main_bkg
        .clone()
        .unwrap_or_else(|| "white".to_string())
}

/// Theme fallback for `transitionColor`; upstream interpolates without a
/// fallback, so an empty string means the attribute is still emitted (matching
/// upstream's template-literal behaviour).
fn transition_color(theme: &ThemeVariables) -> String {
    theme.transition_color.clone().unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Class-diagram markers: extension / composition / aggregation / dependency /
// lollipop. Attribute order & values mirror upstream `markers.js` lines
// 12-313.
// ---------------------------------------------------------------------------

fn extension(out: &mut String, kind: &str, id: &str) {
    // extensionStart — note .attr('markerUnits', 'userSpaceOnUse')
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-extensionStart\" class=\"marker extension {kind}\" refX=\"18\" refY=\"7\" markerWidth=\"190\" markerHeight=\"240\" orient=\"auto\" markerUnits=\"userSpaceOnUse\"><path d=\"M 1,7 L18,13 V 1 Z\"></path></marker>"
    ));
    // extensionEnd — no markerUnits attribute
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-extensionEnd\" class=\"marker extension {kind}\" refX=\"1\" refY=\"7\" markerWidth=\"20\" markerHeight=\"28\" orient=\"auto\"><path d=\"M 1,1 V 13 L18,7 Z\"></path></marker>"
    ));
    // extensionStart-margin — polygon, viewBox on marker, inline style on polygon
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-extensionStart-margin\" class=\"marker extension {kind}\" refX=\"18\" refY=\"7\" markerWidth=\"20\" markerHeight=\"28\" orient=\"auto\" markerUnits=\"userSpaceOnUse\" viewBox=\"0 0 20 14\"><polygon points=\"10,7 18,13 18,1\" style=\"stroke-width: 2; stroke-dasharray: 0;\"></polygon></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-extensionEnd-margin\" class=\"marker extension {kind}\" refX=\"9\" refY=\"7\" markerWidth=\"20\" markerHeight=\"28\" orient=\"auto\" markerUnits=\"userSpaceOnUse\" viewBox=\"0 0 20 14\"><polygon points=\"10,1 10,13 18,7\" style=\"stroke-width: 2; stroke-dasharray: 0;\"></polygon></marker>"
    ));
}

fn composition(out: &mut String, kind: &str, id: &str) {
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-compositionStart\" class=\"marker composition {kind}\" refX=\"18\" refY=\"7\" markerWidth=\"190\" markerHeight=\"240\" orient=\"auto\"><path d=\"M 18,7 L9,13 L1,7 L9,1 Z\"></path></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-compositionEnd\" class=\"marker composition {kind}\" refX=\"1\" refY=\"7\" markerWidth=\"20\" markerHeight=\"28\" orient=\"auto\"><path d=\"M 18,7 L9,13 L1,7 L9,1 Z\"></path></marker>"
    ));
    // -Start-margin: style first (stroke-width:0), then viewBox, then d — upstream call order.
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-compositionStart-margin\" class=\"marker composition {kind}\" refX=\"15\" refY=\"7\" markerWidth=\"190\" markerHeight=\"240\" orient=\"auto\" markerUnits=\"userSpaceOnUse\"><path style=\"stroke-width: 0;\" viewBox=\"0 0 15 15\" d=\"M 18,7 L9,13 L1,7 L9,1 Z\"></path></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-compositionEnd-margin\" class=\"marker composition {kind}\" refX=\"3.5\" refY=\"7\" markerWidth=\"20\" markerHeight=\"28\" orient=\"auto\" markerUnits=\"userSpaceOnUse\"><path style=\"stroke-width: 0;\" d=\"M 18,7 L9,13 L1,7 L9,1 Z\"></path></marker>"
    ));
}

fn aggregation(out: &mut String, kind: &str, id: &str) {
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-aggregationStart\" class=\"marker aggregation {kind}\" refX=\"18\" refY=\"7\" markerWidth=\"190\" markerHeight=\"240\" orient=\"auto\"><path d=\"M 18,7 L9,13 L1,7 L9,1 Z\"></path></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-aggregationEnd\" class=\"marker aggregation {kind}\" refX=\"1\" refY=\"7\" markerWidth=\"20\" markerHeight=\"28\" orient=\"auto\"><path d=\"M 18,7 L9,13 L1,7 L9,1 Z\"></path></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-aggregationStart-margin\" class=\"marker aggregation {kind}\" refX=\"15\" refY=\"7\" markerWidth=\"190\" markerHeight=\"240\" orient=\"auto\" markerUnits=\"userSpaceOnUse\"><path style=\"stroke-width: 2;\" d=\"M 18,7 L9,13 L1,7 L9,1 Z\"></path></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-aggregationEnd-margin\" class=\"marker aggregation {kind}\" refX=\"1\" refY=\"7\" markerWidth=\"20\" markerHeight=\"28\" orient=\"auto\" markerUnits=\"userSpaceOnUse\"><path style=\"stroke-width: 2;\" d=\"M 18,7 L9,13 L1,7 L9,1 Z\"></path></marker>"
    ));
}

fn dependency(out: &mut String, kind: &str, id: &str) {
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-dependencyStart\" class=\"marker dependency {kind}\" refX=\"6\" refY=\"7\" markerWidth=\"190\" markerHeight=\"240\" orient=\"auto\"><path d=\"M 5,7 L9,13 L1,7 L9,1 Z\"></path></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-dependencyEnd\" class=\"marker dependency {kind}\" refX=\"13\" refY=\"7\" markerWidth=\"20\" markerHeight=\"28\" orient=\"auto\"><path d=\"M 18,7 L9,13 L14,7 L9,1 Z\"></path></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-dependencyStart-margin\" class=\"marker dependency {kind}\" refX=\"4\" refY=\"7\" markerWidth=\"190\" markerHeight=\"240\" orient=\"auto\" markerUnits=\"userSpaceOnUse\"><path style=\"stroke-width: 0;\" d=\"M 5,7 L9,13 L1,7 L9,1 Z\"></path></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-dependencyEnd-margin\" class=\"marker dependency {kind}\" refX=\"16\" refY=\"7\" markerWidth=\"20\" markerHeight=\"28\" orient=\"auto\" markerUnits=\"userSpaceOnUse\"><path style=\"stroke-width: 0;\" d=\"M 18,7 L9,13 L14,7 L9,1 Z\"></path></marker>"
    ));
}

fn lollipop(out: &mut String, kind: &str, id: &str) {
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-lollipopStart\" class=\"marker lollipop {kind}\" refX=\"13\" refY=\"7\" markerWidth=\"190\" markerHeight=\"240\" orient=\"auto\"><circle fill=\"transparent\" cx=\"7\" cy=\"7\" r=\"6\"></circle></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-lollipopEnd\" class=\"marker lollipop {kind}\" refX=\"1\" refY=\"7\" markerWidth=\"190\" markerHeight=\"240\" orient=\"auto\"><circle fill=\"transparent\" cx=\"7\" cy=\"7\" r=\"6\"></circle></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-lollipopStart-margin\" class=\"marker lollipop {kind}\" refX=\"13\" refY=\"7\" markerWidth=\"190\" markerHeight=\"240\" orient=\"auto\" markerUnits=\"userSpaceOnUse\"><circle fill=\"transparent\" cx=\"7\" cy=\"7\" r=\"6\" stroke-width=\"2\"></circle></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-lollipopEnd-margin\" class=\"marker lollipop {kind}\" refX=\"1\" refY=\"7\" markerWidth=\"190\" markerHeight=\"240\" orient=\"auto\" markerUnits=\"userSpaceOnUse\"><circle fill=\"transparent\" cx=\"7\" cy=\"7\" r=\"6\" stroke-width=\"2\"></circle></marker>"
    ));
}

// ---------------------------------------------------------------------------
// Flowchart markers: point / circle / cross / barb
// Upstream markers.js lines 314-569.
// ---------------------------------------------------------------------------

fn point(out: &mut String, kind: &str, id: &str) {
    // pointEnd
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-pointEnd\" class=\"marker {kind}\" viewBox=\"0 0 10 10\" refX=\"5\" refY=\"5\" markerUnits=\"userSpaceOnUse\" markerWidth=\"8\" markerHeight=\"8\" orient=\"auto\"><path d=\"M 0 0 L 10 5 L 0 10 z\" class=\"arrowMarkerPath\" style=\"stroke-width: 1; stroke-dasharray: 1,0;\"></path></marker>"
    ));
    // pointStart
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-pointStart\" class=\"marker {kind}\" viewBox=\"0 0 10 10\" refX=\"4.5\" refY=\"5\" markerUnits=\"userSpaceOnUse\" markerWidth=\"8\" markerHeight=\"8\" orient=\"auto\"><path d=\"M 0 5 L 10 10 L 10 0 z\" class=\"arrowMarkerPath\" style=\"stroke-width: 1; stroke-dasharray: 1,0;\"></path></marker>"
    ));
    // pointEnd-margin — path, stroke-width: 0
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-pointEnd-margin\" class=\"marker {kind}\" viewBox=\"0 0 11.5 14\" refX=\"11.5\" refY=\"7\" markerUnits=\"userSpaceOnUse\" markerWidth=\"10.5\" markerHeight=\"14\" orient=\"auto\"><path d=\"M 0 0 L 11.5 7 L 0 14 z\" class=\"arrowMarkerPath\" style=\"stroke-width: 0; stroke-dasharray: 1,0;\"></path></marker>"
    ));
    // pointStart-margin — polygon, stroke-width: 0
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-pointStart-margin\" class=\"marker {kind}\" viewBox=\"0 0 11.5 14\" refX=\"1\" refY=\"7\" markerUnits=\"userSpaceOnUse\" markerWidth=\"11.5\" markerHeight=\"14\" orient=\"auto\"><polygon points=\"0,7 11.5,14 11.5,0\" class=\"arrowMarkerPath\" style=\"stroke-width: 0; stroke-dasharray: 1,0;\"></polygon></marker>"
    ));
}

fn circle(out: &mut String, kind: &str, id: &str) {
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-circleEnd\" class=\"marker {kind}\" viewBox=\"0 0 10 10\" refX=\"11\" refY=\"5\" markerUnits=\"userSpaceOnUse\" markerWidth=\"11\" markerHeight=\"11\" orient=\"auto\"><circle cx=\"5\" cy=\"5\" r=\"5\" class=\"arrowMarkerPath\" style=\"stroke-width: 1; stroke-dasharray: 1,0;\"></circle></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-circleStart\" class=\"marker {kind}\" viewBox=\"0 0 10 10\" refX=\"-1\" refY=\"5\" markerUnits=\"userSpaceOnUse\" markerWidth=\"11\" markerHeight=\"11\" orient=\"auto\"><circle cx=\"5\" cy=\"5\" r=\"5\" class=\"arrowMarkerPath\" style=\"stroke-width: 1; stroke-dasharray: 1,0;\"></circle></marker>"
    ));
    // circleEnd-margin — upstream calls refY BEFORE refX; match that order.
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-circleEnd-margin\" class=\"marker {kind}\" viewBox=\"0 0 10 10\" refY=\"5\" refX=\"12.25\" markerUnits=\"userSpaceOnUse\" markerWidth=\"14\" markerHeight=\"14\" orient=\"auto\"><circle cx=\"5\" cy=\"5\" r=\"5\" class=\"arrowMarkerPath\" style=\"stroke-width: 0; stroke-dasharray: 1,0;\"></circle></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-circleStart-margin\" class=\"marker {kind}\" viewBox=\"0 0 10 10\" refX=\"-2\" refY=\"5\" markerUnits=\"userSpaceOnUse\" markerWidth=\"14\" markerHeight=\"14\" orient=\"auto\"><circle cx=\"5\" cy=\"5\" r=\"5\" class=\"arrowMarkerPath\" style=\"stroke-width: 0; stroke-dasharray: 1,0;\"></circle></marker>"
    ));
}

fn cross(out: &mut String, kind: &str, id: &str) {
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-crossEnd\" class=\"marker cross {kind}\" viewBox=\"0 0 11 11\" refX=\"12\" refY=\"5.2\" markerUnits=\"userSpaceOnUse\" markerWidth=\"11\" markerHeight=\"11\" orient=\"auto\"><path d=\"M 1,1 l 9,9 M 10,1 l -9,9\" class=\"arrowMarkerPath\" style=\"stroke-width: 2; stroke-dasharray: 1,0;\"></path></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-crossStart\" class=\"marker cross {kind}\" viewBox=\"0 0 11 11\" refX=\"-1\" refY=\"5.2\" markerUnits=\"userSpaceOnUse\" markerWidth=\"11\" markerHeight=\"11\" orient=\"auto\"><path d=\"M 1,1 l 9,9 M 10,1 l -9,9\" class=\"arrowMarkerPath\" style=\"stroke-width: 2; stroke-dasharray: 1,0;\"></path></marker>"
    ));
    // crossEnd-margin — NO stroke-dasharray (upstream omits it for the End-margin variant).
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-crossEnd-margin\" class=\"marker cross {kind}\" viewBox=\"0 0 15 15\" refX=\"17.7\" refY=\"7.5\" markerUnits=\"userSpaceOnUse\" markerWidth=\"12\" markerHeight=\"12\" orient=\"auto\"><path d=\"M 1,1 L 14,14 M 1,14 L 14,1\" class=\"arrowMarkerPath\" style=\"stroke-width: 2.5;\"></path></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-crossStart-margin\" class=\"marker cross {kind}\" viewBox=\"0 0 15 15\" refX=\"-3.5\" refY=\"7.5\" markerUnits=\"userSpaceOnUse\" markerWidth=\"12\" markerHeight=\"12\" orient=\"auto\"><path d=\"M 1,1 L 14,14 M 1,14 L 14,1\" class=\"arrowMarkerPath\" style=\"stroke-width: 2.5; stroke-dasharray: 1,0;\"></path></marker>"
    ));
}

fn barb(out: &mut String, kind: &str, id: &str) {
    // No class attribute on barb (upstream doesn't set one).
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-barbEnd\" refX=\"19\" refY=\"7\" markerWidth=\"20\" markerHeight=\"14\" markerUnits=\"userSpaceOnUse\" orient=\"auto\"><path d=\"M 19,7 L9,13 L14,7 L9,1 Z\"></path></marker>"
    ));
}

fn barb_neo(out: &mut String, kind: &str, id: &str, theme: &ThemeVariables) {
    // markerUnits="strokeWidth" for the primary barbEnd.
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-barbEnd\" refX=\"19\" refY=\"7\" markerWidth=\"20\" markerHeight=\"14\" markerUnits=\"strokeWidth\" orient=\"auto\"><path d=\"M 19,7 L11,14 L13,7 L11,0 Z\"></path></marker>"
    ));
    let fill = transition_color(theme);
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-barbEnd-margin\" refX=\"17\" refY=\"7\" markerWidth=\"20\" markerHeight=\"14\" markerUnits=\"userSpaceOnUse\" orient=\"auto\"><path d=\"M 19,7 L11,14 L13,7 L11,0 Z\" fill=\"{fill}\"></path></marker>"
    ));
}

// ---------------------------------------------------------------------------
// ER-diagram markers: only_one / zero_or_one / one_or_more / zero_or_more
// (and -neo variants). Upstream markers.js lines 570-858.
// ---------------------------------------------------------------------------

fn only_one(out: &mut String, kind: &str, id: &str) {
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-onlyOneStart\" class=\"marker onlyOne {kind}\" refX=\"0\" refY=\"9\" markerWidth=\"18\" markerHeight=\"18\" orient=\"auto\"><path d=\"M9,0 L9,18 M15,0 L15,18\"></path></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-onlyOneEnd\" class=\"marker onlyOne {kind}\" refX=\"18\" refY=\"9\" markerWidth=\"18\" markerHeight=\"18\" orient=\"auto\"><path d=\"M3,0 L3,18 M9,0 L9,18\"></path></marker>"
    ));
}

fn zero_or_one(out: &mut String, kind: &str, id: &str) {
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-zeroOrOneStart\" class=\"marker zeroOrOne {kind}\" refX=\"0\" refY=\"9\" markerWidth=\"30\" markerHeight=\"18\" orient=\"auto\"><circle fill=\"white\" cx=\"21\" cy=\"9\" r=\"6\"></circle><path d=\"M9,0 L9,18\"></path></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-zeroOrOneEnd\" class=\"marker zeroOrOne {kind}\" refX=\"30\" refY=\"9\" markerWidth=\"30\" markerHeight=\"18\" orient=\"auto\"><circle fill=\"white\" cx=\"9\" cy=\"9\" r=\"6\"></circle><path d=\"M21,0 L21,18\"></path></marker>"
    ));
}

fn one_or_more(out: &mut String, kind: &str, id: &str) {
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-oneOrMoreStart\" class=\"marker oneOrMore {kind}\" refX=\"18\" refY=\"18\" markerWidth=\"45\" markerHeight=\"36\" orient=\"auto\"><path d=\"M0,18 Q 18,0 36,18 Q 18,36 0,18 M42,9 L42,27\"></path></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-oneOrMoreEnd\" class=\"marker oneOrMore {kind}\" refX=\"27\" refY=\"18\" markerWidth=\"45\" markerHeight=\"36\" orient=\"auto\"><path d=\"M3,9 L3,27 M9,18 Q27,0 45,18 Q27,36 9,18\"></path></marker>"
    ));
}

fn zero_or_more(out: &mut String, kind: &str, id: &str) {
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-zeroOrMoreStart\" class=\"marker zeroOrMore {kind}\" refX=\"18\" refY=\"18\" markerWidth=\"57\" markerHeight=\"36\" orient=\"auto\"><circle fill=\"white\" cx=\"48\" cy=\"18\" r=\"6\"></circle><path d=\"M0,18 Q18,0 36,18 Q18,36 0,18\"></path></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-zeroOrMoreEnd\" class=\"marker zeroOrMore {kind}\" refX=\"39\" refY=\"18\" markerWidth=\"57\" markerHeight=\"36\" orient=\"auto\"><circle fill=\"white\" cx=\"9\" cy=\"18\" r=\"6\"></circle><path d=\"M21,18 Q39,0 57,18 Q39,36 21,18\"></path></marker>"
    ));
}

fn only_one_neo(out: &mut String, kind: &str, id: &str, theme: &ThemeVariables) {
    let sw = stroke_width(theme);
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-onlyOneStart\" class=\"marker onlyOne {kind}\" refX=\"0\" refY=\"9\" markerWidth=\"18\" markerHeight=\"18\" orient=\"auto\" markerUnits=\"userSpaceOnUse\"><path d=\"M9,0 L9,18 M15,0 L15,18\" stroke-width=\"{sw}\"></path></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-onlyOneEnd\" class=\"marker onlyOne {kind}\" refX=\"18\" refY=\"9\" markerWidth=\"18\" markerHeight=\"18\" orient=\"auto\" markerUnits=\"userSpaceOnUse\"><path d=\"M3,0 L3,18 M9,0 L9,18\" stroke-width=\"{sw}\"></path></marker>"
    ));
}

fn zero_or_one_neo(out: &mut String, kind: &str, id: &str, theme: &ThemeVariables) {
    let sw = stroke_width(theme);
    let bkg = main_bkg(theme);
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-zeroOrOneStart\" class=\"marker zeroOrOne {kind}\" refX=\"0\" refY=\"9\" markerWidth=\"30\" markerHeight=\"18\" orient=\"auto\" markerUnits=\"userSpaceOnUse\"><circle fill=\"{bkg}\" cx=\"21\" cy=\"9\" stroke-width=\"{sw}\" r=\"6\"></circle><path d=\"M9,0 L9,18\" stroke-width=\"{sw}\"></path></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-zeroOrOneEnd\" class=\"marker zeroOrOne {kind}\" refX=\"30\" refY=\"9\" markerWidth=\"30\" markerHeight=\"18\" markerUnits=\"userSpaceOnUse\" orient=\"auto\"><circle fill=\"{bkg}\" cx=\"9\" cy=\"9\" stroke-width=\"{sw}\" r=\"6\"></circle><path d=\"M21,0 L21,18\" stroke-width=\"{sw}\"></path></marker>"
    ));
}

fn one_or_more_neo(out: &mut String, kind: &str, id: &str, theme: &ThemeVariables) {
    let sw = stroke_width(theme);
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-oneOrMoreStart\" class=\"marker oneOrMore {kind}\" refX=\"18\" refY=\"18\" markerWidth=\"45\" markerHeight=\"36\" orient=\"auto\" markerUnits=\"userSpaceOnUse\"><path d=\"M0,18 Q 18,0 36,18 Q 18,36 0,18 M42,9 L42,27\" stroke-width=\"{sw}\"></path></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-oneOrMoreEnd\" class=\"marker oneOrMore {kind}\" refX=\"27\" refY=\"18\" markerWidth=\"45\" markerHeight=\"36\" markerUnits=\"userSpaceOnUse\" orient=\"auto\"><path d=\"M3,9 L3,27 M9,18 Q27,0 45,18 Q27,36 9,18\" stroke-width=\"{sw}\"></path></marker>"
    ));
}

fn zero_or_more_neo(out: &mut String, kind: &str, id: &str, theme: &ThemeVariables) {
    let sw = stroke_width(theme);
    let bkg = main_bkg(theme);
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-zeroOrMoreStart\" class=\"marker zeroOrMore {kind}\" refX=\"18\" refY=\"18\" markerWidth=\"57\" markerHeight=\"36\" markerUnits=\"userSpaceOnUse\" orient=\"auto\"><circle fill=\"{bkg}\" cx=\"45.5\" cy=\"18\" r=\"6\" stroke-width=\"{sw}\"></circle><path d=\"M0,18 Q18,0 36,18 Q18,36 0,18\" stroke-width=\"{sw}\"></path></marker>"
    ));
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-zeroOrMoreEnd\" class=\"marker zeroOrMore {kind}\" refX=\"39\" refY=\"18\" markerWidth=\"57\" markerHeight=\"36\" orient=\"auto\" markerUnits=\"userSpaceOnUse\"><circle fill=\"{bkg}\" cx=\"11\" cy=\"18\" r=\"6\" stroke-width=\"{sw}\"></circle><path d=\"M21,18 Q39,0 57,18 Q39,36 21,18\" stroke-width=\"{sw}\"></path></marker>"
    ));
}

// ---------------------------------------------------------------------------
// Requirement-diagram markers. Upstream markers.js lines 860-949.
// ---------------------------------------------------------------------------

fn requirement_arrow(out: &mut String, kind: &str, id: &str) {
    // Upstream uses a multiline template literal with embedded newlines+spaces;
    // reproduce byte-exact.
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-requirement_arrowEnd\" refX=\"20\" refY=\"10\" markerWidth=\"20\" markerHeight=\"20\" orient=\"auto\"><path d=\"M0,0\n      L20,10\n      M20,10\n      L0,20\"></path></marker>"
    ));
}

fn requirement_arrow_neo(out: &mut String, kind: &str, id: &str, theme: &ThemeVariables) {
    let sw = stroke_width(theme);
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-requirement_arrowEnd\" refX=\"20\" refY=\"10\" markerWidth=\"20\" markerHeight=\"20\" orient=\"auto\" markerUnits=\"userSpaceOnUse\" stroke-width=\"{sw}\" viewBox=\"0 0 25 20\"><path d=\"M0,0\n      L20,10\n      M20,10\n      L0,20\" stroke-linejoin=\"miter\"></path></marker>"
    ));
}

fn requirement_contains(out: &mut String, kind: &str, id: &str) {
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-requirement_containsStart\" refX=\"0\" refY=\"10\" markerWidth=\"20\" markerHeight=\"20\" orient=\"auto\"><g><circle cx=\"10\" cy=\"10\" r=\"9\" fill=\"none\"></circle><line x1=\"1\" x2=\"19\" y1=\"10\" y2=\"10\"></line><line y1=\"1\" y2=\"19\" x1=\"10\" x2=\"10\"></line></g></marker>"
    ));
}

fn requirement_contains_neo(out: &mut String, kind: &str, id: &str, theme: &ThemeVariables) {
    // Upstream applies stroke-width to every child via selectAll('*').
    let sw = stroke_width(theme);
    out.push_str(&format!(
        "<marker id=\"{id}_{kind}-requirement_containsStart\" refX=\"0\" refY=\"10\" markerWidth=\"20\" markerHeight=\"20\" orient=\"auto\" markerUnits=\"userSpaceOnUse\"><g><circle cx=\"10\" cy=\"10\" r=\"9\" fill=\"none\" stroke-width=\"{sw}\"></circle><line x1=\"1\" x2=\"19\" y1=\"10\" y2=\"10\" stroke-width=\"{sw}\"></line><line y1=\"1\" y2=\"19\" x1=\"10\" x2=\"10\" stroke-width=\"{sw}\"></line></g></marker>"
    ));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_theme() -> ThemeVariables {
        ThemeVariables::default()
    }

    #[test]
    fn flowchart_defs_contains_point_end() {
        let out = defs("flowchart", "my-id", &empty_theme());
        assert!(
            out.contains(r#"<marker id="my-id_flowchart-pointEnd""#),
            "missing pointEnd marker: {out}"
        );
        assert!(out.contains(r#"viewBox="0 0 10 10""#));
        assert!(out.contains(r#"<path d="M 0 0 L 10 5 L 0 10 z""#));
    }

    #[test]
    fn flowchart_byte_exact_point_end() {
        // Byte-exact comparison against the upstream reference snippet
        // (extracted from tests/reference/fixtures/flowchart/01.svg).
        let out = defs("flowchart-v2", "ref-fixtures-flowchart-01", &empty_theme());
        let expected = r##"<marker id="ref-fixtures-flowchart-01_flowchart-v2-pointEnd" class="marker flowchart-v2" viewBox="0 0 10 10" refX="5" refY="5" markerUnits="userSpaceOnUse" markerWidth="8" markerHeight="8" orient="auto"><path d="M 0 0 L 10 5 L 0 10 z" class="arrowMarkerPath" style="stroke-width: 1; stroke-dasharray: 1,0;"></path></marker>"##;
        assert!(
            out.contains(expected),
            "byte-exact pointEnd mismatch\nactual: {out}"
        );
    }

    #[test]
    fn flowchart_byte_exact_cross_end_margin() {
        let out = defs("flowchart-v2", "ref-fixtures-flowchart-01", &empty_theme());
        let expected = r##"<marker id="ref-fixtures-flowchart-01_flowchart-v2-crossEnd-margin" class="marker cross flowchart-v2" viewBox="0 0 15 15" refX="17.7" refY="7.5" markerUnits="userSpaceOnUse" markerWidth="12" markerHeight="12" orient="auto"><path d="M 1,1 L 14,14 M 1,14 L 14,1" class="arrowMarkerPath" style="stroke-width: 2.5;"></path></marker>"##;
        assert!(
            out.contains(expected),
            "byte-exact crossEnd-margin mismatch\nactual: {out}"
        );
    }

    #[test]
    fn flowchart_byte_exact_circle_end_margin_attr_order() {
        // Upstream calls refY BEFORE refX for circleEnd-margin — ensure we
        // preserve that quirky order.
        let out = defs("flowchart-v2", "ref-fixtures-flowchart-01", &empty_theme());
        let expected = r##"<marker id="ref-fixtures-flowchart-01_flowchart-v2-circleEnd-margin" class="marker flowchart-v2" viewBox="0 0 10 10" refY="5" refX="12.25" markerUnits="userSpaceOnUse" markerWidth="14" markerHeight="14" orient="auto"><circle cx="5" cy="5" r="5" class="arrowMarkerPath" style="stroke-width: 0; stroke-dasharray: 1,0;"></circle></marker>"##;
        assert!(
            out.contains(expected),
            "byte-exact circleEnd-margin mismatch (attr order matters)\nactual: {out}"
        );
    }

    #[test]
    fn class_defs_byte_exact_aggregation_start() {
        let out = defs("class", "ref-ext-fixtures-demos-class-01", &empty_theme());
        let expected = r##"<marker id="ref-ext-fixtures-demos-class-01_class-aggregationStart" class="marker aggregation class" refX="18" refY="7" markerWidth="190" markerHeight="240" orient="auto"><path d="M 18,7 L9,13 L1,7 L9,1 Z"></path></marker>"##;
        assert!(
            out.contains(expected),
            "class aggregationStart mismatch\nactual: {out}"
        );
    }

    #[test]
    fn class_defs_byte_exact_composition_start_margin() {
        // Upstream's compositionStart-margin has unusual attr order inside path:
        // style → viewBox → d.
        let out = defs("class", "ref-ext-fixtures-demos-class-01", &empty_theme());
        let expected = r##"<marker id="ref-ext-fixtures-demos-class-01_class-compositionStart-margin" class="marker composition class" refX="15" refY="7" markerWidth="190" markerHeight="240" orient="auto" markerUnits="userSpaceOnUse"><path style="stroke-width: 0;" viewBox="0 0 15 15" d="M 18,7 L9,13 L1,7 L9,1 Z"></path></marker>"##;
        assert!(
            out.contains(expected),
            "compositionStart-margin mismatch\nactual: {out}"
        );
    }

    #[test]
    fn state_barb_end_byte_exact() {
        let out = defs(
            "stateDiagram",
            "ref-ext-fixtures-demos-state-07",
            &empty_theme(),
        );
        let expected = r##"<marker id="ref-ext-fixtures-demos-state-07_stateDiagram-barbEnd" refX="19" refY="7" markerWidth="20" markerHeight="14" markerUnits="userSpaceOnUse" orient="auto"><path d="M 19,7 L9,13 L14,7 L9,1 Z"></path></marker>"##;
        assert_eq!(
            out, expected,
            "state barbEnd must be exactly this one marker"
        );
    }

    #[test]
    fn er_only_one_byte_exact() {
        let out = defs("er", "ref-ext-fixtures-demos-er-01", &empty_theme());
        let expected_start = r##"<marker id="ref-ext-fixtures-demos-er-01_er-onlyOneStart" class="marker onlyOne er" refX="0" refY="9" markerWidth="18" markerHeight="18" orient="auto"><path d="M9,0 L9,18 M15,0 L15,18"></path></marker>"##;
        assert!(
            out.contains(expected_start),
            "er onlyOneStart mismatch\n{out}"
        );
    }

    #[test]
    fn er_zero_or_more_byte_exact() {
        let out = defs("er", "ref-ext-fixtures-demos-er-01", &empty_theme());
        let expected = r##"<marker id="ref-ext-fixtures-demos-er-01_er-zeroOrMoreStart" class="marker zeroOrMore er" refX="18" refY="18" markerWidth="57" markerHeight="36" orient="auto"><circle fill="white" cx="48" cy="18" r="6"></circle><path d="M0,18 Q18,0 36,18 Q18,36 0,18"></path></marker>"##;
        assert!(out.contains(expected), "er zeroOrMoreStart mismatch\n{out}");
    }

    #[test]
    fn single_returns_none_for_unknown() {
        assert!(single("nonexistent", "flowchart", "x", &empty_theme()).is_none());
    }

    #[test]
    fn single_returns_one_family() {
        let out = single("cross", "flowchart", "x", &empty_theme()).unwrap();
        assert!(out.contains(r#"id="x_flowchart-crossEnd""#));
        assert!(out.contains(r#"id="x_flowchart-crossStart""#));
        assert!(out.contains(r#"id="x_flowchart-crossEnd-margin""#));
        assert!(out.contains(r#"id="x_flowchart-crossStart-margin""#));
    }

    #[test]
    fn class_kind_emits_all_five_families() {
        let out = defs("class", "id", &empty_theme());
        for fam in [
            "aggregation",
            "extension",
            "composition",
            "dependency",
            "lollipop",
        ] {
            assert!(out.contains(&format!("{fam}Start")), "missing {fam}Start");
            assert!(out.contains(&format!("{fam}End")), "missing {fam}End");
        }
    }

    #[test]
    fn unknown_kind_falls_back_to_flowchart() {
        let out = defs("totally-unknown", "id", &empty_theme());
        assert!(out.contains("id_totally-unknown-pointEnd"));
        assert!(out.contains("id_totally-unknown-circleEnd"));
        assert!(out.contains("id_totally-unknown-crossEnd"));
    }

    #[test]
    fn neo_markers_use_theme_stroke_width() {
        let mut theme = empty_theme();
        theme.stroke_width = Some(2);
        theme.main_bkg = Some("#fff".to_string());
        let out = single("zero_or_one_neo", "er", "id", &theme).unwrap();
        assert!(out.contains(r#"stroke-width="2""#));
        assert!(out.contains(r##"fill="#fff""##));
    }

    #[test]
    fn barb_neo_uses_transition_color() {
        let mut theme = empty_theme();
        theme.transition_color = Some("#123".to_string());
        let out = single("barbNeo", "stateDiagram", "id", &theme).unwrap();
        assert!(out.contains(r##"fill="#123""##));
        assert!(out.contains(r#"markerUnits="strokeWidth""#));
    }
}
