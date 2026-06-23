/**
 * React Native 的 Rust markdown module 加载器。
 *
 * RN 下 markdown 解析走 native FFI（`@supramark/markdown-native-rn` 注册的
 * adapter），不加载 wasm。此文件**不 import** `@supramark/markdown-web`，
 * 因此 metro 打包 RN bundle 时不会扫描到 wasm 相关代码，避免 lazy bundle
 * 与静态 require 冲突导致的 "unknown module" 错误。
 *
 * 此文件只被 `@supramark/core` 的 RN 入口（`index.rn.ts`）引用。
 */

import { getNativeParserAdapter } from './parser-native-adapter.js';

type RustMarkdownModule = {
  parse?: (source: string) => unknown;
  parseJson?: (source: string) => string | Promise<string>;
};

export async function loadRustMarkdownModule(): Promise<RustMarkdownModule> {
  // RN 下 native adapter 必须已注册（由 `@supramark/markdown-native-rn`
  // side-effect import 触发）。未注册时抛明确错误，而不是回退到 wasm。
  const nativeAdapter = getNativeParserAdapter();
  if (nativeAdapter) {
    return { parseJson: nativeAdapter.parseJson };
  }

  throw new Error(
    "RN runtime requires native markdown parser adapter. " +
      "Add `import '@supramark/markdown-native-rn'` at app entry to register it."
  );
}
