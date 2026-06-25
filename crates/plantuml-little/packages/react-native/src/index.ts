/**
 * @actrium/supramark-plantuml-native-rn
 *
 * Importing this package side-registers a `plantuml` adapter with
 * `@supramark/engines`'s React Native native-engine registry. From
 * there, `createReactNativeDiagramEngine()` discovers it and dispatches
 * PlantUML source blocks to the linked `libsupramark_plantuml_native` static lib.
 *
 * Host usage:
 *
 *   ```ts
 *   import '@actrium/supramark-plantuml-native-rn';     // side-effect register
 *   import { createReactNativeDiagramEngine } from '@supramark/engines/rn';
 *
 *   const engine = createReactNativeDiagramEngine();
 *   const svg = await engine.render('plantuml', 'a -> b -> c');
 *   ```
 */
import { NativeModules, Platform } from 'react-native';
import { registerNativeEngineAdapter } from '@supramark/engines/rn';

const LINKING_ERROR =
  `The package '@actrium/supramark-plantuml-native-rn' doesn't seem to be linked. Make sure:\n\n` +
  Platform.select({
    ios: '- You have run `pod install`\n',
    android: '',
    default: '',
  }) +
  '- You rebuilt the app after installing the package\n' +
  '- You are not using Expo Go\n';

interface NativeSupramarkPlantumlModule {
  render(source: string): Promise<string>;
  getVersion(): Promise<string>;
}

/** Shape of the codegen'd TurboModule spec module (CommonJS interop). */
interface NativeSupramarkPlantumlSpecModule {
  default?: NativeSupramarkPlantumlModule;
}

// Load the codegen'd TurboModule, tolerating its absence (old arch or a
// host that hasn't run codegen). Kept separate from the pure selection so
// the latter stays unit-testable.
function loadTurboModule(): NativeSupramarkPlantumlModule | undefined {
  try {
    return (require('./NativeSupramarkPlantuml') as NativeSupramarkPlantumlSpecModule).default ?? undefined;
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
  turbo: NativeSupramarkPlantumlModule | null | undefined,
  bridged: NativeSupramarkPlantumlModule | null | undefined
): NativeSupramarkPlantumlModule {
  if (turbo) return turbo;
  if (!bridged) {
    return new Proxy({} as NativeSupramarkPlantumlModule, {
      get() {
        throw new Error(LINKING_ERROR);
      },
    });
  }
  return bridged;
}

const native = resolveNative(
  loadTurboModule(),
  NativeModules.SupramarkPlantumlNative as NativeSupramarkPlantumlModule | undefined
);

registerNativeEngineAdapter({
  engine: 'plantuml',
  render: async (code: string) => native.render(code),
});

/** Re-exported for diagnostics (returns the linked `supramark_plantuml_version()`). */
export const getNativeVersion = (): Promise<string> => native.getVersion();
