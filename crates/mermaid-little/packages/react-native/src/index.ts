/**
 * @actrium/supramark-mermaid-native-rn
 *
 * Importing this package side-registers a `mermaid` adapter with
 * `@supramark/engines`'s React Native native-engine registry. From
 * there, `createReactNativeDiagramEngine()` discovers it and dispatches
 * Mermaid source blocks to the linked `libsupramark_mermaid_native` static lib.
 *
 * Host usage:
 *
 *   ```ts
 *   import '@actrium/supramark-mermaid-native-rn';     // side-effect register
 *   import { createReactNativeDiagramEngine } from '@supramark/engines/rn';
 *
 *   const engine = createReactNativeDiagramEngine();
 *   const svg = await engine.render('mermaid', 'a -> b -> c');
 *   ```
 */
import { NativeModules, Platform } from 'react-native';
import { registerNativeEngineAdapter } from '@supramark/engines/rn';

const LINKING_ERROR =
  `The package '@actrium/supramark-mermaid-native-rn' doesn't seem to be linked. Make sure:\n\n` +
  Platform.select({
    ios: '- You have run `pod install`\n',
    android: '',
    default: '',
  }) +
  '- You rebuilt the app after installing the package\n' +
  '- You are not using Expo Go\n';

interface NativeSupramarkMermaidModule {
  render(source: string): Promise<string>;
  getVersion(): Promise<string>;
}

/** Shape of the codegen'd TurboModule spec module (CommonJS interop). */
interface NativeSupramarkMermaidSpecModule {
  default?: NativeSupramarkMermaidModule;
}

// Load the codegen'd TurboModule, tolerating its absence (old arch or a
// host that hasn't run codegen). Kept separate from the pure selection so
// the latter stays unit-testable.
function loadTurboModule(): NativeSupramarkMermaidModule | undefined {
  try {
    return (require('./NativeSupramarkMermaid') as NativeSupramarkMermaidSpecModule).default ?? undefined;
  } catch {
    // not codegen'd or new-arch disabled — fall through
    return undefined;
  }
}

/**
 * Pick the native module: TurboModule (new arch) first, then the bridge-based
 * NativeModules entry (old arch). When neither is linked, return a Proxy that
 * throws an actionable error on first use rather than at import time. Kept a
 * pure function of its inputs so the fallback order is unit-testable.
 */
export function resolveNative(
  turbo: NativeSupramarkMermaidModule | null | undefined,
  bridged: NativeSupramarkMermaidModule | null | undefined
): NativeSupramarkMermaidModule {
  if (turbo) return turbo;
  if (!bridged) {
    return new Proxy({} as NativeSupramarkMermaidModule, {
      get() {
        throw new Error(LINKING_ERROR);
      },
    });
  }
  return bridged;
}

const native = resolveNative(
  loadTurboModule(),
  NativeModules.SupramarkMermaidNative as NativeSupramarkMermaidModule | undefined
);

registerNativeEngineAdapter({
  engine: 'mermaid',
  render: async (code: string) => native.render(code),
});

/** Re-exported for diagnostics (returns the linked `supramark_mermaid_version()`). */
export const getNativeVersion = (): Promise<string> => native.getVersion();
