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

function resolveNative(): NativeSupramarkMermaidModule {
  // TurboModule (new arch) first.
  try {
    const turbo = (require('./NativeSupramarkMermaid') as NativeSupramarkMermaidSpecModule)
      .default;
    if (turbo) return turbo;
  } catch {
    // not codegen'd or new-arch disabled — fall through
  }
  // Bridge-based fallback (old arch).
  const bridged = NativeModules.SupramarkMermaidNative as NativeSupramarkMermaidModule | undefined;
  if (!bridged) {
    return new Proxy({} as NativeSupramarkMermaidModule, {
      get() {
        throw new Error(LINKING_ERROR);
      },
    });
  }
  return bridged;
}

const native = resolveNative();

registerNativeEngineAdapter({
  engine: 'mermaid',
  render: async (code: string) => native.render(code),
});

/** Re-exported for diagnostics (returns the linked `supramark_mermaid_version()`). */
export const getNativeVersion = (): Promise<string> => native.getVersion();
