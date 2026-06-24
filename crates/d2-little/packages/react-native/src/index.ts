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

function resolveNative(): NativeSupramarkD2Module {
  // TurboModule (new arch) first.
  try {
    const turbo = require('./NativeSupramarkD2').default;
    if (turbo) return turbo as NativeSupramarkD2Module;
  } catch {
    // not codegen'd or new-arch disabled — fall through
  }
  // Bridge-based fallback (old arch).
  const bridged = NativeModules.SupramarkD2Native;
  if (!bridged) {
    return new Proxy({} as NativeSupramarkD2Module, {
      get() {
        throw new Error(LINKING_ERROR);
      },
    });
  }
  return bridged as NativeSupramarkD2Module;
}

const native = resolveNative();

registerNativeEngineAdapter({
  engine: 'd2',
  render: async (code: string) => native.render(code),
});

/** Re-exported for diagnostics (returns the linked `supramark_d2_version()`). */
export const getNativeVersion = (): Promise<string> => native.getVersion();
