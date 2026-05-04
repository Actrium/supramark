//! Run cytoscape + cose-bilkent layout inside an embedded QuickJS runtime.
//!
//! Replicates mermaid's `createCytoscapeInstance` / `extractPositionedNodes`
//! / `extractPositionedEdges` flow, but with `headless: true` instead of a
//! DOM container. The spike at `/tmp/spike-cytoscape` proved this works
//! with only a small no-op shim for `console` and the timer functions.
//!
//! ## RNG seeding
//!
//! Mermaid's reference renderer (`tests/support/generate_ref.mjs`) replaces
//! `Math.random` with a `mulberry32(0x12345678)` PRNG and re-seeds the state
//! to `0x12345678` before each render. Because cose-bilkent reads `Math.random`
//! during its tiling fallback (and arguably during `randomize`-style
//! initialisation when not disabled), we apply the same seeding here so our
//! layout is byte-exact with the upstream Node test harness.

use rquickjs::{CatchResultExt, Context, Runtime};

const CYTOSCAPE_JS: &str = include_str!("vendor/cytoscape.umd.js");
const LAYOUT_BASE_JS: &str = include_str!("vendor/layout-base.js");
const COSE_BASE_JS: &str = include_str!("vendor/cose-base.js");
const COSE_BILKENT_JS: &str = include_str!("vendor/cytoscape-cose-bilkent.js");

/// Minimal host shim. cytoscape's UMD reads `console.warn` / `console.log`
/// at the top level (silenced) and starts a `requestAnimationFrame` loop in
/// styleEnabled mode that falls through to `setTimeout(fn, 1000/60)` — we
/// install no-op stubs so the call sites don't throw.
///
/// Mermaid's `generate_ref.mjs` re-seeds `Math.random` with mulberry32 (state
/// = 0x12345678) before each render. We replicate that here so RNG-driven
/// portions of cose-bilkent (tiling padding fallback, etc.) match upstream.
const HOST_SHIM: &str = r#"
globalThis.console = {
    log: function () {}, warn: function () {}, error: function () {},
    trace: function () {}, info: function () {}, debug: function () {},
};
globalThis.setTimeout    = function (fn, ms) { return 0; };
globalThis.clearTimeout  = function (id) {};
globalThis.setInterval   = function (fn, ms) { return 0; };
globalThis.clearInterval = function (id) {};

// Deterministic Math.random — mulberry32 seeded with 0x12345678. Matches
// `tests/support/generate_ref.mjs` which patches Math.random per render.
(function () {
    var __rngState = 0x12345678;
    Math.random = function () {
        __rngState = (__rngState + 0x6d2b79f5) | 0;
        var t = __rngState;
        t = Math.imul(t ^ (t >>> 15), 1 | t);
        t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
        return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
    globalThis.__resetRng = function () { __rngState = 0x12345678; };
})();
"#;

/// Input node spec — mirrors mermaid's `node` object passed to `addNodes`.
#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub width: f64,
    pub height: f64,
    pub padding: f64,
}

/// Input edge spec — mirrors mermaid's `edge` object passed to `addEdges`.
#[derive(Debug, Clone)]
pub struct Edge {
    pub id: String,
    pub source: String,
    pub target: String,
}

/// Input graph passed to [`layout`].
#[derive(Debug, Clone, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// Output node — `cytoscape` `node.position()` after layout, plus the
/// original input attrs (`labelText`, `width`, `height`, `padding`).
#[derive(Debug, Clone)]
pub struct PositionedNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub label: String,
    pub width: f64,
    pub height: f64,
    pub padding: f64,
}

/// Output edge — `cytoscape` `edge._private.rscratch.{start,mid,end}{X,Y}`
/// after layout. In headless mode these may be unset; consumers should
/// handle `None` and fall back to straight-line geometry.
#[derive(Debug, Clone)]
pub struct PositionedEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub start_x: Option<f64>,
    pub start_y: Option<f64>,
    pub mid_x: Option<f64>,
    pub mid_y: Option<f64>,
    pub end_x: Option<f64>,
    pub end_y: Option<f64>,
}

/// Output of a layout run.
#[derive(Debug, Clone, Default)]
pub struct LayoutOutput {
    pub nodes: Vec<PositionedNode>,
    pub edges: Vec<PositionedEdge>,
}

#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("quickjs runtime error: {0}")]
    Runtime(String),
    #[error("cytoscape layout error: {0}")]
    Layout(String),
    #[error("invalid layout output: {0}")]
    Output(String),
}

/// Build a tiny JSON snippet representing the input graph. We feed this to
/// the QuickJS context as a string and parse it on the JS side — this avoids
/// the rquickjs object-conversion machinery (which carries a full Object
/// type round-trip per field).
fn graph_to_json(g: &Graph) -> String {
    fn esc(s: &str) -> String {
        let mut out = String::with_capacity(s.len() + 2);
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out
    }
    let mut s = String::new();
    s.push_str("{\"nodes\":[");
    for (i, n) in g.nodes.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "{{\"id\":\"{}\",\"label\":\"{}\",\"width\":{},\"height\":{},\"padding\":{}}}",
            esc(&n.id),
            esc(&n.label),
            n.width,
            n.height,
            n.padding
        ));
    }
    s.push_str("],\"edges\":[");
    for (i, e) in g.edges.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "{{\"id\":\"{}\",\"source\":\"{}\",\"target\":\"{}\"}}",
            esc(&e.id),
            esc(&e.source),
            esc(&e.target)
        ));
    }
    s.push_str("]}");
    s
}

/// Lay out the input graph using cytoscape + cose-bilkent. Returns absolute
/// node positions and (when populated) edge bezier control points.
///
/// Runs on a dedicated thread with an 8 MiB stack — cose-bilkent's recursive
/// coarsening can blow QuickJS's 256 KiB default.
pub fn layout(graph: &Graph) -> Result<LayoutOutput, LayoutError> {
    let json = graph_to_json(graph);
    // Spawn on a thread with a generous stack — cose-bilkent recursion can
    // be deep on real fixtures (~30 nodes is typical).
    let handle = std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || run_in_quickjs(&json))
        .map_err(|e| LayoutError::Runtime(format!("spawn: {}", e)))?;
    handle
        .join()
        .map_err(|_| LayoutError::Runtime("join".into()))?
}

fn run_in_quickjs(graph_json: &str) -> Result<LayoutOutput, LayoutError> {
    let rt = Runtime::new().map_err(|e| LayoutError::Runtime(format!("rt: {}", e)))?;
    rt.set_max_stack_size(0); // unlimited within thread stack
    let ctx = Context::full(&rt).map_err(|e| LayoutError::Runtime(format!("ctx: {}", e)))?;

    ctx.with(|ctx| {
        ctx.eval::<(), _>(HOST_SHIM)
            .catch(&ctx)
            .map_err(|e| LayoutError::Runtime(format!("shim: {:?}", e)))?;
        ctx.eval::<(), _>(CYTOSCAPE_JS)
            .catch(&ctx)
            .map_err(|e| LayoutError::Runtime(format!("load cytoscape: {:?}", e)))?;
        ctx.eval::<(), _>(LAYOUT_BASE_JS)
            .catch(&ctx)
            .map_err(|e| LayoutError::Runtime(format!("load layout-base: {:?}", e)))?;
        ctx.eval::<(), _>(COSE_BASE_JS)
            .catch(&ctx)
            .map_err(|e| LayoutError::Runtime(format!("load cose-base: {:?}", e)))?;
        ctx.eval::<(), _>(COSE_BILKENT_JS)
            .catch(&ctx)
            .map_err(|e| LayoutError::Runtime(format!("load cose-bilkent: {:?}", e)))?;

        ctx.eval::<(), _>(
            r#"
            cytoscape.use(globalThis.cytoscapeCoseBilkent);
            globalThis.__runLayout = function (graphJson) {
                // Reset RNG — match generate_ref.mjs per-render seeding.
                if (globalThis.__resetRng) globalThis.__resetRng();
                var input = JSON.parse(graphJson);
                var cy = cytoscape({ headless: true, styleEnabled: true });
                input.nodes.forEach(function (n) {
                    cy.add({
                        group: 'nodes',
                        data: {
                            id: n.id,
                            labelText: n.label,
                            height: n.height,
                            width: n.width,
                            padding: n.padding,
                        },
                        position: { x: 0, y: 0 },
                    });
                });
                input.edges.forEach(function (e) {
                    cy.add({
                        group: 'edges',
                        data: { id: e.id, source: e.source, target: e.target },
                    });
                });
                cy.nodes().forEach(function (n) {
                    n.layoutDimensions = function () {
                        var d = n.data();
                        return { w: d.width, h: d.height };
                    };
                });
                cy.layout({
                    name: 'cose-bilkent',
                    quality: 'proof',
                    styleEnabled: false,
                    animate: false,
                }).run();
                var nodesOut = [];
                cy.nodes().forEach(function (n) {
                    var d = n.data();
                    var p = n.position();
                    nodesOut.push({
                        id: d.id,
                        x: p.x,
                        y: p.y,
                        label: d.labelText || '',
                        width: d.width,
                        height: d.height,
                        padding: d.padding || 0,
                    });
                });
                var edgesOut = [];
                cy.edges().forEach(function (e) {
                    var d = e.data();
                    var rs = (e._private && e._private.rscratch) || {};
                    edgesOut.push({
                        id: d.id,
                        source: d.source,
                        target: d.target,
                        startX: rs.startX, startY: rs.startY,
                        midX: rs.midX,     midY: rs.midY,
                        endX: rs.endX,     endY: rs.endY,
                    });
                });
                return JSON.stringify({ nodes: nodesOut, edges: edgesOut });
            };
            "#,
        )
        .catch(&ctx)
        .map_err(|e| LayoutError::Runtime(format!("install runner: {:?}", e)))?;

        // Stash the input on the global, then call the runner. Avoids
        // routing the long string through rquickjs Function::call's
        // argument boxing (which has size limits in some versions).
        ctx.globals()
            .set("__graphJson", graph_json)
            .map_err(|e| LayoutError::Runtime(format!("set graph: {}", e)))?;
        let result_json: String = ctx
            .eval(r#"globalThis.__runLayout(globalThis.__graphJson)"#)
            .catch(&ctx)
            .map_err(|e| LayoutError::Layout(format!("{:?}", e)))?;

        parse_layout_output(&result_json)
    })
}

fn parse_layout_output(json: &str) -> Result<LayoutOutput, LayoutError> {
    // Tiny ad-hoc JSON parser tailored to the shape we emit. Avoids
    // pulling serde_json's full deserialise machinery into the hot path —
    // we control the producer.
    use serde_json::Value;
    let v: Value = serde_json::from_str(json)
        .map_err(|e| LayoutError::Output(format!("parse layout json: {}", e)))?;
    let obj = v
        .as_object()
        .ok_or_else(|| LayoutError::Output("top-level not object".into()))?;
    let nodes_v = obj
        .get("nodes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| LayoutError::Output("nodes not array".into()))?;
    let edges_v = obj
        .get("edges")
        .and_then(|v| v.as_array())
        .ok_or_else(|| LayoutError::Output("edges not array".into()))?;

    let mut nodes = Vec::with_capacity(nodes_v.len());
    for n in nodes_v {
        nodes.push(PositionedNode {
            id: n.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            x: n.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
            y: n.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
            label: n
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            width: n.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0),
            height: n.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0),
            padding: n.get("padding").and_then(|v| v.as_f64()).unwrap_or(0.0),
        });
    }
    let mut edges = Vec::with_capacity(edges_v.len());
    let f = |v: Option<&Value>| -> Option<f64> { v.and_then(|x| x.as_f64()) };
    for e in edges_v {
        edges.push(PositionedEdge {
            id: e.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            source: e
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            target: e
                .get("target")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            start_x: f(e.get("startX")),
            start_y: f(e.get("startY")),
            mid_x: f(e.get("midX")),
            mid_y: f(e.get("midY")),
            end_x: f(e.get("endX")),
            end_y: f(e.get("endY")),
        });
    }
    Ok(LayoutOutput { nodes, edges })
}
