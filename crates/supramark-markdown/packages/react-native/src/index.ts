/**
 * @supramark/markdown-native-rn
 *
 * Importing this package side-registers a Markdown parser adapter with
 * `@supramark/core`'s native parser registry. From there, `parse(source)`
 * discovers it and routes Markdown source through the linked
 * `libsupramark_markdown_native` static lib instead of the wasm
 * `@supramark/markdown-web` bundle.
 *
 * Host usage:
 *
 * ```ts
 * import '@supramark/markdown-native-rn';   // side-effect register
 * import { parse } from '@supramark/core';
 *
 * const ast = await parse(markdown);
 * ```
 *
 * Metro config on the host side should stub `@supramark/markdown-web`
 * to an empty module (mirroring how `@actrium/*-web` wasm packages
 * are stubbed for the diagram engines) so the wasm bundle never loads.
 *
 * The registry API is imported from `@supramark/core/rn` (keeping the web
 * entry point clean, mirroring the pattern used by `@supramark/engines/rn`).
 */
import { NativeModules } from 'react-native';
import { registerNativeParserAdapter } from '@supramark/core/rn';
import { resolveNative, type NativeSupramarkMarkdownModule } from './resolveNative';

export { resolveNative } from './resolveNative';

/** Shape of the codegen'd TurboModule spec module (CommonJS interop). */
interface NativeSupramarkMarkdownSpecModule {
  default?: NativeSupramarkMarkdownModule;
}

/**
 * Load the codegen'd TurboModule (new arch), or `null` when codegen
 * didn't run / new-arch is disabled. Kept separate from {@link
 * resolveNative} so the selection logic stays a pure, testable function.
 */
function loadTurboModule(): NativeSupramarkMarkdownModule | null {
  try {
    const turbo = (require('./NativeSupramarkMarkdown') as NativeSupramarkMarkdownSpecModule)
      .default;
    return turbo ?? null;
  } catch {
    // not codegen'd or new-arch disabled — fall through
    return null;
  }
}

const native = resolveNative(
  loadTurboModule(),
  NativeModules.SupramarkMarkdownNative as NativeSupramarkMarkdownModule | null | undefined
);

registerNativeParserAdapter({
  parseJson: async (source: string) => native.parseJson(source),
  getVersion: async () => native.getVersion(),
});

/** Re-exported for diagnostics (returns the linked `supramark_markdown_version()`). */
export const getNativeVersion = (): Promise<string> => native.getVersion();

/** Direct access to the native parse entry, bypassing the registry. */
export const parseJsonNative = (source: string): Promise<string> => native.parseJson(source);
