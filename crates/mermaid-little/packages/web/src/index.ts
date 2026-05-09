/**
 * `@kookyleo/mermaid-little-web` — wasm-bindgen wrapper around the
 * `mermaid-little` Rust crate.
 *
 * Unlike the plantuml-little wasm wrapper, mermaid-little-web has no
 * external bridge requirements: the underlying Rust crate ships its
 * own pure-Rust dagre layout engine. Consumers simply import and call
 * {@link convert}.
 *
 * ```ts
 * import { convert } from '@kookyleo/mermaid-little-web';
 *
 * const svg = convert('graph TD; A-->B;');
 * ```
 */

// Re-export the raw wasm-bindgen API. `convert`, `convert_with_id`, and
// `version` are the public functions; everything else (`__wbg_set_wasm`
// etc.) stays internal to the generated JS.
export { convert, convert_with_id as convertWithId, version } from './wasm/mermaid_little_web.js';
