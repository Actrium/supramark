/**
 * `@kookyleo/d2-little-web` — wasm-bindgen wrapper around the `d2-little`
 * Rust crate.
 *
 * Unlike the plantuml-little wasm wrapper, d2-little-web has no external
 * bridge requirements: the underlying Rust crate ships its own
 * pure-Rust dagre layout engine. Consumers simply import and call
 * {@link convert}.
 *
 * ```ts
 * import { convert } from '@kookyleo/d2-little-web';
 *
 * const svg = convert('a -> b');
 * ```
 */

// Re-export the raw wasm-bindgen API. `convert` and `version` are the
// two public functions; everything else (`__wbg_set_wasm` etc.) stays
// internal to the generated JS.
export { convert, version } from './wasm/d2_little_web.js';
