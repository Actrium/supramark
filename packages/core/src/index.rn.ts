/**
 * @supramark/core - React Native 专用入口
 *
 * 此入口只暴露 AST v2 parser facade 与跨平台类型。
 */

// AST 类型定义
export * from './ast.js';

// 插件系统类型
export type {
  SupramarkParseContext,
  SupramarkPlugin,
  SupramarkParseOptions,
  SupramarkPreset,
} from './plugin.js';

// Feature Interface - 功能扩展接口系统
export * from './feature.js';

// Diagram Feature 工厂(diagram features 用 defineDiagramFeature(...) 注册;
// 跨平台,与 web 入口保持一致)
export * from './diagram-feature.js';

// Container 扩展接口(features/containers/* 在 web + RN 都用 :::container 语法)
export * from './container-extension.js';

// 语法家族运行时 hook(供 Feature 使用)
export {
  type ContainerProcessorContext,
  type ContainerHookContext,
  type ContainerHook,
  registerContainerHook,
} from './syntax/container.js';

// Native parser adapter registry —— 供 RN native wrapper 包
// (如 `@supramark/markdown-native-rn`) side-effect 注册。
// Web / Node 不会注册，plugin.ts 自动回退到 wasm。
// 仅从 RN 入口导出（不污染 web 入口），模式对齐 `@supramark/engines/rn`。
export {
  type NativeParseJsonFn,
  type NativeParserAdapter,
  registerNativeParserAdapter,
  getNativeParserAdapter,
  listNativeParserAdapters,
  parseViaNative,
} from './parser-native-adapter.js';

/**
 * AST v2 parser facade.
 *
 * 内部使用 Rust `supramark-markdown` parser。RN 生产入口后续可接 native/TurboModule
 * binding，公开合同保持 `source -> SupramarkRootNode`。
 *
 * @param source - Markdown 源文本
 * @param options - 解析选项(可选 AST 后处理插件)
 * @returns Supramark AST v2
 */
export { parse } from './plugin.js';

/**
 * 预设(Presets)
 *
 * 预设是预配置的选项组合,用于快速设置常见的解析配置。
 */
export { presetDefault, presetGFM } from './plugin.js';

/**
 * Feature 相关工具函数
 */
export {
  isFeatureEnabled,
  getFeatureOptionsAs,
  getDiagramFeatureFamily,
  getDiagramFeatureIdsForEngine,
  isFeatureGroupEnabled,
  isDiagramFeatureEnabled,
} from './feature.js';
