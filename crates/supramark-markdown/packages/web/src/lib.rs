use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// `Reflect.get(target, key)` — declared inline so the binding does not
    /// need a js-sys dependency just for options field extraction.
    #[wasm_bindgen(catch, js_namespace = Reflect)]
    fn get(target: &JsValue, key: &JsValue) -> Result<JsValue, JsValue>;
}

fn bool_option(options: &JsValue, key: &str) -> Result<Option<bool>, JsValue> {
    let value = get(options, &JsValue::from_str(key))
        .map_err(|error| JsValue::from_str(&format!("failed to read options.{key}: {error:?}")))?;
    if value.is_undefined() {
        return Ok(None);
    }
    value
        .as_bool()
        .map(Some)
        .ok_or_else(|| JsValue::from_str(&format!("options.{key} must be a boolean")))
}

#[wasm_bindgen]
pub fn parse_json(source: &str) -> Result<String, JsValue> {
    serde_json::to_string(&supramark_markdown::parse(source))
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn parse_json_with_options(source: &str, options: &JsValue) -> Result<String, JsValue> {
    // Unknown keys are ignored so the options object can grow without a
    // breaking wasm rebuild.
    let mut parse_options = supramark_markdown::ParseOptions::default();
    if !options.is_null() {
        if let Some(wikilink) = bool_option(options, "wikilink")? {
            parse_options.wikilink = wikilink;
        }
    }
    serde_json::to_string(&supramark_markdown::parse_with_options(
        source,
        parse_options,
    ))
    .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
