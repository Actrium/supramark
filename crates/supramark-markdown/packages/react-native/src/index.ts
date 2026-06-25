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
 * registry API 从 `@supramark/core/rn` 导入（不污染 web 入口，模式对齐
 * `@supramark/engines/rn`）。
 */
import { NativeModules, Platform } from 'react-native';
import { registerNativeParserAdapter } from '@supramark/core/rn';

const LINKING_ERROR =
  `The package '@supramark/markdown-native-rn' doesn't seem to be linked. Make sure:\n\n` +
  Platform.select({
    ios: '- You have run `pod install`\n',
    android: '',
    default: '',
  }) +
  '- You rebuilt the app after installing the package\n' +
  '- You are not using Expo Go\n';

interface NativeSupramarkMarkdownModule {
  parseJson(source: string): Promise<string>;
  getVersion(): Promise<string>;
}

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

/**
 * Pick the native module implementation, preferring the New Architecture
 * TurboModule over the legacy bridge. When neither is available the
 * package wasn't linked, so return a Proxy that throws an actionable
 * error on first use (rather than crashing at import time).
 *
 * Exported for unit tests — pass the candidates explicitly so the
 * fallback order can be exercised without relying on import-time module
 * resolution.
 */
export function resolveNative(
  turbo: NativeSupramarkMarkdownModule | null | undefined,
  bridged: NativeSupramarkMarkdownModule | null | undefined
): NativeSupramarkMarkdownModule {
  // TurboModule (new arch) first.
  if (turbo) return turbo;
  // Bridge-based fallback (old arch).
  if (!bridged) {
    return new Proxy({} as NativeSupramarkMarkdownModule, {
      get() {
        throw new Error(LINKING_ERROR);
      },
    });
  }
  return bridged;
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
