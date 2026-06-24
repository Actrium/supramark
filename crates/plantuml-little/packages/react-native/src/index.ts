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

function resolveNative(): NativeSupramarkPlantumlModule {
  // TurboModule (new arch) first.
  try {
    const turbo = require('./NativeSupramarkPlantuml').default;
    if (turbo) return turbo as NativeSupramarkPlantumlModule;
  } catch {
    // not codegen'd or new-arch disabled — fall through
  }
  // Bridge-based fallback (old arch).
  const bridged = NativeModules.SupramarkPlantumlNative;
  if (!bridged) {
    return new Proxy({} as NativeSupramarkPlantumlModule, {
      get() {
        throw new Error(LINKING_ERROR);
      },
    });
  }
  return bridged as NativeSupramarkPlantumlModule;
}

const native = resolveNative();

registerNativeEngineAdapter({
  engine: 'plantuml',
  render: async (code: string) => native.render(code),
});

/** Re-exported for diagnostics (returns the linked `supramark_plantuml_version()`). */
export const getNativeVersion = (): Promise<string> => native.getVersion();
