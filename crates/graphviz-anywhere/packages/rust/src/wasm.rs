//! wasm32 bridge.
//!
//! On the `wasm32-unknown-unknown` target, the native C library is not
//! available. Instead, this module wires `GraphvizContext::render` to a
//! JavaScript function the host environment must provide.
//!
//! # Contract
//!
//! The host must install a global function named
//! `__graphviz_anywhere_render` that takes three strings — `dot`, `engine`,
//! and `format` — and returns a string containing the rendered output (for
//! `Format::Svg` this is the SVG source). On failure, it must throw a
//! JavaScript `Error`; the thrown message is surfaced back to Rust as
//! [`GraphvizError::RenderFailed`].
//!
//! # Wiring
//!
//! The `@actrium/graphviz-anywhere-web` npm package provides
//! `Graphviz.load()` and `.layout()`. A typical wire-up looks like:
//!
//! ```js
//! import { Graphviz } from "@actrium/graphviz-anywhere-web";
//!
//! const graphviz = await Graphviz.load();
//! globalThis.__graphviz_anywhere_render = (dot, engine, format) => {
//!   // graphviz-anywhere-web exposes a .layout(dot, format, engine) API.
//!   return graphviz.layout(dot, format, engine);
//! };
//! ```

use crate::{Engine, Format, GraphvizError};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = globalThis, js_name = __graphviz_anywhere_render, catch)]
    fn js_render(dot: &str, engine: &str, format: &str) -> Result<String, JsValue>;
}

fn engine_name(engine: Engine) -> &'static str {
    match engine {
        Engine::Dot => "dot",
        Engine::Neato => "neato",
        Engine::Fdp => "fdp",
        Engine::Sfdp => "sfdp",
        Engine::Circo => "circo",
        Engine::Twopi => "twopi",
        Engine::Osage => "osage",
        Engine::Patchwork => "patchwork",
    }
}

fn format_name(format: Format) -> &'static str {
    match format {
        Format::Svg => "svg",
        Format::Png => "png",
        Format::Pdf => "pdf",
        Format::Ps => "ps",
        Format::Json => "json",
        Format::DotOutput => "dot",
        Format::Xdot => "xdot",
        Format::Plain => "plain",
    }
}

pub(crate) fn render(dot: &str, engine: Engine, format: Format) -> Result<Vec<u8>, GraphvizError> {
    js_render(dot, engine_name(engine), format_name(format))
        .map(|s| s.into_bytes())
        .map_err(|_| GraphvizError::RenderFailed)
}
