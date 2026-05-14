/**
 * @supramark/core - React Native 专用入口
 *
 * 此入口只包含跨平台兼容的功能,不包含 unified/remark 相关代码
 * 避免在 React Native 环境中引入 Node.js 专用依赖
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

/**
 * 默认解析器(使用 markdown-it)
 *
 * - 跨平台兼容:支持 React Native、Web、Node.js
 * - 推荐用于生产环境
 * - 性能较好,体积较小
 *
 * @param markdown - Markdown 源文本
 * @param options - 解析选项(可选插件)
 * @returns Supramark AST
 */
export { parseMarkdown } from './plugin.js';

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
