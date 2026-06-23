/**
 * Native parser adapter registry.
 *
 * `supramark-markdown` Rust crate 编译为三种产物：
 *   - `packages/web`   → wasm-bindgen，给 Web / Node 消费
 *   - `packages/native` → staticlib / cdylib，给 RN iOS / Android 消费
 *   - 主 crate 本身     → rlib，给其他 Rust crate 依赖
 *
 * native 产物通过 RN TurboModule / Old NativeModule 暴露给 JS，具体
 * bridge 代码在 consumer 侧 npm 包（`@supramark/markdown-native-rn`）
 * 里，因为 native module 形态是平台 / linker 相关的。
 *
 * 本文件是 **routing layer**：consumer 在启动时注册一个 parser adapter，
 * `plugin.ts` 的 `loadRustMarkdownModule()` 在 RN 下优先走 native adapter，
 * 没有注册时回退到 wasm（Web / Node 路径不变）。
 *
 * 这与 `@supramark/engines` 的 `registerNativeEngineAdapter` 模式对齐。
 */

/**
 * Native parser adapter 的一次调用。
 *
 * @param source Markdown 源文本
 * @returns      AST v2 JSON 字符串（与 `@supramark/markdown-web` 的
 *               `parse_json` 输出同 schema）。Throws on parse / FFI error.
 */
export type NativeParseJsonFn = (source: string) => Promise<string>;

export interface NativeParserAdapter {
  /** 解析 Markdown 源文本，返回 AST v2 JSON 字符串。 */
  parseJson: NativeParseJsonFn;
  /** 可选：返回 native 库版本号，用于诊断。 */
  getVersion?: () => Promise<string>;
}

const registry: NativeParserAdapter[] = [];
let installed: NativeParserAdapter | undefined;

/**
 * 注册 native parser adapter。多次注册 last-wins，便于测试 / 热替换。
 *
 * 通常由 native wrapper 包的 side-effect import 调用：
 *
 * ```ts
 * import '@supramark/markdown-native-rn';
 * ```
 */
export function registerNativeParserAdapter(adapter: NativeParserAdapter): void {
  registry.push(adapter);
  installed = adapter;
}

/** 取回当前注册的 adapter，没有则返回 `undefined`。 */
export function getNativeParserAdapter(): NativeParserAdapter | undefined {
  return installed;
}

/** 列出所有已注册的 adapter（按注册顺序）。主要给诊断用。 */
export function listNativeParserAdapters(): NativeParserAdapter[] {
  return [...registry];
}

/**
 * 通过 native adapter 解析。未注册则返回 `null`，让 caller 回退到 wasm。
 */
export async function parseViaNative(source: string): Promise<string | null> {
  const adapter = installed;
  if (!adapter) return null;
  return adapter.parseJson(source);
}

/** 测试辅助 —— 清空 registry。不从 package barrel 导出。 */
export function __resetNativeParserRegistryForTests(): void {
  registry.length = 0;
  installed = undefined;
}
