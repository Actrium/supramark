import { Platform } from 'react-native';

const LINKING_ERROR =
  `The package '@supramark/markdown-native-rn' doesn't seem to be linked. Make sure:\n\n` +
  Platform.select({
    ios: '- You have run `pod install`\n',
    android: '',
    windows: '',
    macos: '- You have run `pod install`\n',
    default: '',
  }) +
  '- You rebuilt the app after installing the package\n' +
  '- You are not using Expo Go\n';

export interface NativeSupramarkMarkdownModule {
  parseJson(source: string): Promise<string>;
  getVersion(): Promise<string>;
}

/**
 * Pick the native module implementation, preferring the New Architecture
 * TurboModule over the legacy bridge. When neither is available the package
 * was not linked, so defer an actionable error until first use.
 */
export function resolveNative(
  turbo: NativeSupramarkMarkdownModule | null | undefined,
  bridged: NativeSupramarkMarkdownModule | null | undefined
): NativeSupramarkMarkdownModule {
  if (turbo) return turbo;
  if (bridged) return bridged;

  return new Proxy({} as NativeSupramarkMarkdownModule, {
    get() {
      throw new Error(LINKING_ERROR);
    },
  });
}
