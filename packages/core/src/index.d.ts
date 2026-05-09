export * from './ast.js';
export type { SupramarkParseContext, SupramarkPlugin, SupramarkParseOptions, SupramarkPreset, } from './plugin.js';
export * from './feature.js';
export { type ContainerProcessorContext, type ContainerHookContext, type ContainerHook, registerContainerHook, extractContainerInnerText, } from './syntax/container.js';
export { type InputProcessorContext, type InputHookContext, type InputHook, registerInputHook, extractInputInnerText, } from './syntax/input.js';
export * from './container-extension.js';
export { type ContainerFeature, type ContainerWebRenderArgs, type ContainerWebRenderer, type ContainerRNRenderArgs, type ContainerRNRenderer, validateContainerFeature, } from './container-feature.js';
/**
 * 默认解析器（使用 markdown-it）
 *
 * 跨平台兼容：支持 React Native、Web、Node.js
 * 推荐用于生产环境
 * @param markdown - Markdown 源文本
 * @param options - 解析选项（可选插件）
 * @returns Supramark AST
 */
export { parseMarkdown } from './plugin.js';
/**
 * Remark 解析器（使用 unified + remark）
 *
 * 仅支持 Node.js 和 Web 环境（不支持 React Native）
 * 提供更丰富的 remark 生态集成能力
 * 体积较大，但可以使用 remark 插件
 * @param markdown - Markdown 源文本
 * @param options - 解析选项（可选插件）
 * @returns Supramark AST
 */
export { parseMarkdownWithRemark } from './remark.js';
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
