/**
 * Native parser adapter registry.
 *
 * The `supramark-markdown` Rust crate is compiled into three artifacts:
 *   - `packages/web`    → wasm-bindgen, consumed by Web / Node
 *   - `packages/native` → staticlib / cdylib, consumed by RN iOS / Android
 *   - the main crate itself → rlib, depended on by other Rust crates
 *
 * The native artifact is exposed to JS via an RN TurboModule / legacy NativeModule;
 * the actual bridge code lives in the consumer-side npm package
 * (`@supramark/markdown-native-rn`), since the native module shape is
 * platform/linker-specific.
 *
 * This file is the **routing layer**: the consumer registers a parser adapter at
 * startup, and `loadRustMarkdownModule()` in `plugin.ts` prefers the native adapter
 * on RN, falling back to wasm if none is registered (the Web / Node path is
 * unchanged).
 *
 * This mirrors the `registerNativeEngineAdapter` pattern used by `@supramark/engines`.
 */

/**
 * A single call to the native parser adapter.
 *
 * @param source the Markdown source text
 * @returns      an AST v2 JSON string (same schema as the `parse_json` output of
 *               `@supramark/markdown-web`). Throws on parse / FFI error.
 */
export type NativeParseJsonFn = (source: string) => Promise<string>;

export type NativeParserOptions = {
  wikilink?: boolean;
};

export type NativeParseJsonWithOptionsFn = (
  source: string,
  options: NativeParserOptions
) => Promise<string>;

export interface NativeParserAdapter {
  /** Parse the Markdown source text and return an AST v2 JSON string. */
  parseJson: NativeParseJsonFn;
  /**
   * Optional: parse with parser flags (e.g. `wikilink`). Adapters backed by a
   * native build without this entry cannot honor parser options; `parse()` in
   * `plugin.ts` then throws instead of silently dropping the syntax.
   */
  parseJsonWithOptions?: NativeParseJsonWithOptionsFn;
  /** Optional: return the native library version, for diagnostics. */
  getVersion?: () => Promise<string>;
}

const registry: NativeParserAdapter[] = [];
let installed: NativeParserAdapter | undefined;

/**
 * Register a native parser adapter. Registering more than once is last-wins, which
 * makes testing / hot-swapping easier.
 *
 * Usually invoked by a native wrapper package's side-effect import:
 *
 * ```ts
 * import '@supramark/markdown-native-rn';
 * ```
 */
export function registerNativeParserAdapter(adapter: NativeParserAdapter): void {
  registry.push(adapter);
  installed = adapter;
}

/** Retrieve the currently registered adapter, or `undefined` if none. */
export function getNativeParserAdapter(): NativeParserAdapter | undefined {
  return installed;
}

/** List all registered adapters (in registration order). Mainly for diagnostics. */
export function listNativeParserAdapters(): NativeParserAdapter[] {
  return [...registry];
}

/**
 * Parse via the native adapter. Returns `null` if none is registered, letting the
 * caller fall back to wasm.
 */
export async function parseViaNative(source: string): Promise<string | null> {
  const adapter = installed;
  if (!adapter) return null;
  return adapter.parseJson(source);
}

/** Test helper —— clears the registry. Not exported from the package barrel. */
export function __resetNativeParserRegistryForTests(): void {
  registry.length = 0;
  installed = undefined;
}
