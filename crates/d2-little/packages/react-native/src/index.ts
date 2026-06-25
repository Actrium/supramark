/**
 * @actrium/supramark-d2-native-rn
 *
 * Importing this package side-registers a `d2` adapter with
 * `@supramark/engines`'s React Native native-engine registry. From
 * there, `createReactNativeDiagramEngine()` discovers it and dispatches
 * D2 source blocks to the linked `libsupramark_d2_native` static lib.
 *
 * Host usage:
 *
 *   ```ts
 *   import '@actrium/supramark-d2-native-rn';     // side-effect register
 *   import { createReactNativeDiagramEngine } from '@supramark/engines/rn';
 *
 *   const engine = createReactNativeDiagramEngine();
 *   const svg = await engine.render('d2', 'a -> b -> c');
 *   ```
 */
import { NativeModules, Platform } from 'react-native';
import { registerNativeEngineAdapter } from '@supramark/engines/rn';

const LINKING_ERROR =
  `The package '@actrium/supramark-d2-native-rn' doesn't seem to be linked. Make sure:\n\n` +
  Platform.select({
    ios: '- You have run `pod install`\n',
    android: '',
    default: '',
  }) +
  '- You rebuilt the app after installing the package\n' +
  '- You are not using Expo Go\n';

interface NativeSupramarkD2Module {
  render(source: string): Promise<string>;
  getVersion(): Promise<string>;
}

/** Shape of the codegen'd TurboModule spec module (CommonJS interop). */
interface NativeSupramarkD2SpecModule {
  default?: NativeSupramarkD2Module;
}

// Load the codegen'd TurboModule, tolerating its absence (old arch or a
// host that hasn't run codegen). Kept separate from the pure selection so
// the latter stays unit-testable.
function loadTurboModule(): NativeSupramarkD2Module | undefined {
  try {
    return (require('./NativeSupramarkD2') as NativeSupramarkD2SpecModule).default ?? undefined;
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
  turbo: NativeSupramarkD2Module | null | undefined,
  bridged: NativeSupramarkD2Module | null | undefined
): NativeSupramarkD2Module {
  if (turbo) return turbo;
  if (!bridged) {
    return new Proxy({} as NativeSupramarkD2Module, {
      get() {
        throw new Error(LINKING_ERROR);
      },
    });
  }
  return bridged;
}

const native = resolveNative(
  loadTurboModule(),
  NativeModules.SupramarkD2Native as NativeSupramarkD2Module | undefined
);

registerNativeEngineAdapter({
  engine: 'd2',
  render: async (code: string) => native.render(code),
});

/** Re-exported for diagnostics (returns the linked `supramark_d2_version()`). */
export const getNativeVersion = (): Promise<string> => native.getVersion();
