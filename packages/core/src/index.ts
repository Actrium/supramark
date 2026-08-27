// AST type definitions
export * from './ast.js';

// Shared diagram streaming state used by Web and React Native renderers.
export * from './diagram-render-state.js';

// Plugin system types
export type {
  SupramarkParseContext,
  SupramarkPlugin,
  SupramarkParseOptions,
  SupramarkPreset,
} from './plugin.js';

// Feature Interface - the feature extension interface system
export * from './feature.js';

// Diagram Feature factory (defineDiagramFeature spec helper)
export * from './diagram-feature.js';

// Syntax family runtime hooks (for use by Features)
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

// Container extension spec (manifest + params parsing)
export * from './container-extension.js';

// The unified ContainerFeature interface (lean version)
export {
  type ContainerFeature,
  type ContainerWebRenderArgs,
  type ContainerWebRenderer,
  type ContainerRNRenderArgs,
  type SupramarkVideoPressEvent,
  type ContainerRNRenderer,
  validateContainerFeature,
} from './container-feature.js';

/**
 * AST v2 parser facade.
 *
 * Internally uses the Rust `supramark-markdown` parser; the public contract is
 * `source -> SupramarkRootNode`.
 *
 * @param source - the Markdown source text
 * @param options - parse options (optional AST post-processing plugins)
 * @returns Supramark AST v2
 */
export { parse, expandOpaqueContainers } from './plugin.js';

/**
 * Presets.
 *
 * A preset is a pre-configured bundle of options, used to quickly set up a common
 * parsing configuration.
 *
 * @param markdown - the Markdown source text
 * @param options - parse options (optional plugins)
 * @returns the Supramark AST
 */
export { presetDefault, presetGFM } from './plugin.js';

/**
 * Cache utilities.
 *
 * Provides an LRU cache implementation, used to cache the results of
 * compute-intensive operations such as diagram rendering.
 *
 * @param maxSize - the maximum number of cache entries
 * @param ttl - the expiration time (milliseconds)
 * @returns an LRU cache instance
 */
export { LRUCache, createCacheKey, simpleHash, type LRUCacheOptions } from './cache.js';

export type { SupramarkNode } from './ast.js';
export { validateFeature as coreValidateFeature } from './feature.js';
