/**
 * `@actrium/plantuml-little-web` — wasm-bindgen wrapper around the
 * `plantuml-little` Rust crate.
 *
 * The underlying wasm calls out to Graphviz via a host-installed global
 * function `globalThis.__graphviz_anywhere_render(dot, engine, format)`.
 * Use {@link installGraphvizBridge} (or {@link setup}) with a
 * `@actrium/graphviz-anywhere-web` Graphviz instance to wire that up
 * before calling {@link convert}.
 *
 * ```ts
 * import { Graphviz } from '@actrium/graphviz-anywhere-web';
 * import { setup, convert } from '@actrium/plantuml-little-web';
 *
 * const graphviz = await Graphviz.load();
 * setup({ graphviz });
 *
 * const svg = convert('@startuml\nAlice -> Bob : hi\n@enduml');
 * ```
 */

// Re-export the raw wasm-bindgen API. `convert` and `version` are the
// two public functions; everything else (`__wbg_set_wasm` etc.) stays
// internal to the generated JS.
export { convert, version } from './wasm/plantuml_little_web.js';

/**
 * Minimal shape of the object a host must pass so the wasm can render
 * Graphviz. Any `@actrium/graphviz-anywhere-web` `Graphviz` instance
 * already satisfies this (it exposes `.layout(dot, format, engine)`).
 */
export interface GraphvizLike {
  layout(dot: string, format: string, engine: string): string;
}

/**
 * The raw JS function signature the wasm expects on
 * `globalThis.__graphviz_anywhere_render`. `(dot, engine, format) -> svg`.
 */
export type GraphvizBridge = (dot: string, engine: string, format: string) => string;

const GLOBAL_BRIDGE_KEY = '__graphviz_anywhere_render' as const;

type BridgeHost = Record<typeof GLOBAL_BRIDGE_KEY, GraphvizBridge | undefined>;

function bridgeHost(): BridgeHost {
  return globalThis as unknown as BridgeHost;
}

/**
 * Install a raw `(dot, engine, format) -> svg` bridge on `globalThis` so
 * the compiled wasm can call into Graphviz. Returns a disposer that
 * restores the previous binding (if any).
 */
export function installGraphvizBridge(bridge: GraphvizBridge): () => void {
  const host = bridgeHost();
  const previous = host[GLOBAL_BRIDGE_KEY];
  host[GLOBAL_BRIDGE_KEY] = bridge;
  return () => {
    if (bridgeHost()[GLOBAL_BRIDGE_KEY] === bridge) {
      bridgeHost()[GLOBAL_BRIDGE_KEY] = previous;
    }
  };
}

/**
 * Install a `@actrium/graphviz-anywhere-web` Graphviz instance as the
 * backing Graphviz engine for plantuml-little-web. Returns a disposer
 * that restores the previous binding (if any).
 */
export function setup(options: { graphviz: GraphvizLike }): () => void {
  const { graphviz } = options;
  return installGraphvizBridge((dot, engine, format) =>
    graphviz.layout(dot, format, engine)
  );
}

/**
 * Returns true iff a Graphviz bridge has been installed on `globalThis`.
 * Useful for hosts that want to assert wiring before calling `convert`.
 */
export function hasGraphvizBridge(): boolean {
  return typeof bridgeHost()[GLOBAL_BRIDGE_KEY] === 'function';
}
