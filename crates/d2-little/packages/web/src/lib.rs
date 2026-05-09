//! wasm-bindgen wrapper around `d2-little`.
//!
//! Consumers import this crate's generated JS module (via wasm-pack
//! `--target bundler`) and call [`convert`] to turn a D2 source string
//! into an SVG string. d2-little ships its own pure-Rust dagre layout,
//! so no external JS bridge is required — unlike the plantuml-little
//! wasm wrapper, which has to be wired up to a Graphviz engine.
//!
//! `version()` returns the crate version embedded at compile time so
//! hosts can assert the wasm bytes match what they bundled.

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
