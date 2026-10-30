//! wasm-bindgen wrapper around `d2-little`.
//!
//! Consumers import this crate's generated JS module (via wasm-pack
//! `--target bundler`) and call [`convert`] to turn a D2 source string
//! into an SVG string. d2-little ships its own pure-Rust dagre layout,
//! so no external JS bridge is required — unlike the plantuml-little
//! wasm wrapper, which has to be wired up to a Graphviz engine.
//!
//! ## elk layout bridge
//!
//! For D2 sources that request `vars.d2-config.layout-engine: elk`, the
//! host can render via the elkjs bridge: call [`prepare`] to obtain a
//! layout-request JSON + handle, run elkjs `elk.layout` on the request,
//! then call [`render`] with the result. [`convert`] is unchanged and
//! remains the dagre-only path. The engines layer (`@supramark/engines`)
//! orchestrates the prepare → elkjs → render sequence and falls back to
//! [`convert`] when `prepare` reports an unsupported feature (sequence /
//! grid / containers / `near:` / multi-board).
//!
//! `version()` returns the crate version embedded at compile time so
//! hosts can assert the wasm bytes match what they bundled.

use std::cell::Cell;
use std::collections::HashMap;

use wasm_bindgen::prelude::*;

/// Convert a D2 source string to an SVG string.
///
/// Errors from the underlying `d2-little` converter are surfaced as a
/// JavaScript `Error` with the Rust error message.
#[wasm_bindgen]
pub fn convert(input: &str) -> Result<String, JsValue> {
    d2_little::d2_to_svg(input)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map_err(|e| JsValue::from_str(&e))
}

/// Version of the compiled `d2-little-web` wasm.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ---------------------------------------------------------------------------
// prepare / render: external-layout bridge (elkjs)
// ---------------------------------------------------------------------------

thread_local! {
    /// Prepared graphs awaiting a host-supplied layout. Keyed by the handle
    /// returned from `prepare`; consumed (removed) by `render`. wasm is
    /// single-threaded so this needs no locking.
    static PREPARED: std::cell::RefCell<HashMap<u32, d2_little::PreparedLayout>> =
        std::cell::RefCell::new(HashMap::new());
    static NEXT_HANDLE: Cell<u32> = const { Cell::new(1) };
}

/// Result of [`prepare`]: a handle to the prepared graph state held inside
/// the wasm instance, plus a `request` JSON describing what the host must
/// lay out. Pass the handle and a result JSON to [`render`] to finish.
#[wasm_bindgen]
pub struct PrepareResult {
    handle: u32,
    /// [`d2_little::layout_bridge::LayoutRequest`] serialised to JSON.
    request: String,
}

#[wasm_bindgen]
impl PrepareResult {
    #[wasm_bindgen(getter)]
    pub fn handle(&self) -> u32 {
        self.handle
    }

    #[wasm_bindgen(getter)]
    pub fn request(&self) -> String {
        self.request.clone()
    }
}

/// Parse + compile + measure a D2 source WITHOUT running the built-in
/// dagre layout. Returns a handle plus a layout-request JSON the host
/// feeds to its external layout engine (elkjs). The host falls back to
/// [`convert`] when `request.multi_board === true` or any feature flag
/// (`has_sequence` / `has_grid` / `has_near`) is set.
///
/// Contracts:
/// - Each `prepare` allocates a handle; the host MUST pair it with a
///   `render` call to free the stashed graph, or call [`drop_prepared`].
/// - Single active prepare is the supported model; handles leak until
///   consumed.
#[wasm_bindgen]
pub fn prepare(input: &str) -> Result<PrepareResult, JsValue> {
    let opts = d2_little::CompileOptions {
        pad: Some(0),
        ..d2_little::CompileOptions::default()
    };
    let (prepared, request) =
        d2_little::prepare_for_external_layout(input, &opts).map_err(|e| JsValue::from_str(&e))?;
    let request_json = serde_json::to_string(&request)
        .map_err(|e| JsValue::from_str(&format!("layout request serialize: {e}")))?;

    let handle = NEXT_HANDLE.with(|c| {
        let h = c.get();
        c.set(h.wrapping_add(1));
        h
    });
    PREPARED.with(|p| p.borrow_mut().insert(handle, prepared));

    Ok(PrepareResult {
        handle,
        request: request_json,
    })
}

/// Apply a host-computed layout to a previously-prepared graph and render
/// it to SVG. `layout_json` is a [`d2_little::layout_bridge::LayoutResult`]
/// serialised to JSON (one board, matching `prepare`'s request). Consumes
/// and drops the handle.
#[wasm_bindgen]
pub fn render(handle: u32, layout_json: &str) -> Result<String, JsValue> {
    let prepared = PREPARED
        .with(|p| p.borrow_mut().remove(&handle))
        .ok_or_else(|| JsValue::from_str(&format!("unknown prepare handle: {handle}")))?;

    let result: d2_little::layout_bridge::LayoutResult = serde_json::from_str(layout_json)
        .map_err(|e| JsValue::from_str(&format!("layout result deserialize: {e}")))?;

    d2_little::render_with_external_layout(prepared, &result)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map_err(|e| JsValue::from_str(&e))
}

/// Drop a prepared-graph handle without rendering (frees wasm memory).
/// Idempotent — a missing or already-consumed handle is a no-op.
#[wasm_bindgen]
pub fn drop_prepared(handle: u32) {
    PREPARED.with(|p| {
        p.borrow_mut().remove(&handle);
    });
}
