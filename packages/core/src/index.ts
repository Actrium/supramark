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

// Diagram Feature factory (defineDiagramFeature spec helper)
export * from './diagram-feature.js';

// 语法家族运行时 hook（供 Feature 使用）
export {
  type ContainerProcessorContext,
  type ContainerHookContext,
  type ContainerHook,
  registerContainerHook,
  extractContainerInnerText,
} from './syntax/container.js';

export {
  type InputProcessorContext,
  type InputHookContext,
  type InputHook,
  registerInputHook,
  extractInputInnerText,
} from './syntax/input.js';

// container 扩展规范（manifest + params parsing）
export * from './container-extension.js';

// ContainerFeature 统一接口（精简版）
export {
  type ContainerFeature,
  type ContainerWebRenderArgs,
  type ContainerWebRenderer,
  type ContainerRNRenderArgs,
  type ContainerRNRenderer,
  validateContainerFeature,
} from './container-feature.js';

/**
 * AST v2 parser facade.
 *
 * 内部使用 Rust `supramark-markdown` parser，公开合同为
 * `source -> SupramarkRootNode`。
 *
 * @param source - Markdown 源文本
 * @param options - 解析选项（可选 AST 后处理插件）
 * @returns Supramark AST v2
 */
export { parse } from './plugin.js';

/**
 * 预设（Presets）
 *
 * 预设是预配置的选项组合，用于快速设置常见的解析配置。
 *
 * @param markdown - Markdown 源文本
 * @param options - 解析选项（可选插件）
 * @returns Supramark AST
 */
export { presetDefault, presetGFM } from './plugin.js';

/**
 * 缓存工具
 *
 * 提供 LRU 缓存实现，用于缓存图表渲染结果等计算密集型操作的结果。
 *
 * @param maxSize - 最大缓存条目数
 * @param ttl - 过期时间（毫秒）
 * @returns LRU 缓存实例
 */
export { LRUCache, createCacheKey, simpleHash, type LRUCacheOptions } from './cache.js';

export type { SupramarkNode } from './ast.js';
export { validateFeature as coreValidateFeature } from './feature.js';
