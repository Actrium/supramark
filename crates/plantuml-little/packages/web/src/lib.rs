//! wasm-bindgen wrapper around `plantuml-little`.
//!
//! Consumers import this crate's generated JS module (via wasm-pack
//! `--target bundler`) and call [`convert`] to turn PlantUML source into
//! an SVG string.
//!
//! # Bridge contract
//!
//! This crate re-exports the lower-level `graphviz-anywhere` wasm bridge:
//! it expects a host-installed global function
//! `globalThis.__graphviz_anywhere_render(dot, engine, format) -> string`
//! that performs the Graphviz layout step. The TypeScript wrapper
//! (`index.ts` / `index.js`) installs that bridge against a
//! `@actrium/graphviz-anywhere-web` Graphviz instance.
//!
//! `version()` returns the crate version embedded at compile time so
//! hosts can assert the wasm bytes match what they bundled.

use wasm_bindgen::prelude::*;

/// Convert a PlantUML source string to an SVG string.
///
/// Errors from the underlying `plantuml-little` converter are surfaced
/// as a JavaScript `Error` with the Rust `Display` message.
#[wasm_bindgen]
pub fn convert(puml: &str) -> Result<String, JsValue> {
    plantuml_little::convert(puml).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Version of the compiled `plantuml-little-web` wasm.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
