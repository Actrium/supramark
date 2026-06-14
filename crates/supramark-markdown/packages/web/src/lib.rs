use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn parse_json(source: &str) -> Result<String, JsValue> {
    serde_json::to_string(&supramark_markdown::parse(source))
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
