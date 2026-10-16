/**
 * The Rust markdown module loader for React Native.
 *
 * On RN, markdown parsing goes through native FFI (the adapter registered by
 * `@supramark/markdown-native-rn`); wasm is never loaded. This file **does not
 * import** `@supramark/markdown-web`, so Metro never scans any wasm-related code
 * while bundling the RN bundle, avoiding an "unknown module" error caused by a
 * conflict between lazy bundling and static require.
 *
 * This file is referenced only by the RN entry point of `@supramark/core`
 * (`index.rn.ts`).
 */

import { getNativeParserAdapter } from './parser-native-adapter.js';

type RustMarkdownModule = {
  parse?: (source: string) => unknown;
  parseJson?: (source: string) => string | Promise<string>;
  parseWithOptions?: (source: string, options: { wikilink?: boolean }) => unknown;
  parseJsonWithOptions?: (source: string, options: { wikilink?: boolean }) => string | Promise<string>;
};

// Must share the async Promise<RustMarkdownModule> contract with the web loader
// variant (plugin-loader-web.ts) that the bundler swaps in; callers await it.
// eslint-disable-next-line @typescript-eslint/require-await -- contract parity with web loader
export async function loadRustMarkdownModule(): Promise<RustMarkdownModule> {
  // On RN, the native adapter must already be registered (triggered by the
  // `@supramark/markdown-native-rn` side-effect import). If it's not registered,
  // throw a clear error instead of silently falling back to wasm.
  const nativeAdapter = getNativeParserAdapter();
  if (nativeAdapter) {
    return {
      parseJson: nativeAdapter.parseJson,
      ...(nativeAdapter.parseJsonWithOptions
        ? { parseJsonWithOptions: nativeAdapter.parseJsonWithOptions }
        : {}),
    };
  }

  throw new Error(
    "RN runtime requires native markdown parser adapter. " +
      "Add `import '@supramark/markdown-native-rn'` at app entry to register it."
  );
}
