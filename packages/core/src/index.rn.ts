/**
 * @supramark/core - React Native-only entry point.
 *
 * This entry point only exposes the AST v2 parser facade and cross-platform types.
 */

// AST type definitions
export * from './ast.js';

// Shared diagram streaming state used by React Native renderers.
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

// Diagram Feature factory (diagram features register via defineDiagramFeature(...);
// cross-platform, kept consistent with the web entry point)
export * from './diagram-feature.js';

// Container extension interface (features/containers/* use the :::container syntax
// on both web and RN)
export * from './container-extension.js';

// Syntax family runtime hooks (for use by Features)
export {
  type ContainerProcessorContext,
  type ContainerHookContext,
  type ContainerHook,
  registerContainerHook,
  extractContainerInnerText,
} from './syntax/container.js';

// ContainerFeature contract (ContainerRNRenderArgs.onVideoPress etc.) —
// the RN entry re-exports the same container-feature surface as the web
// entry so RN hosts resolve identical types under the metro mapping.
export {
  validateContainerFeature,
  type ContainerFeature,
  type ContainerWebRenderArgs,
  type ContainerWebRenderer,
  type ContainerRNRenderArgs,
  type ContainerRNRenderer,
  type SupramarkVideoPressEvent,
  type ExampleDefinition,
} from './container-feature.js';

// Native parser adapter registry —— for RN native wrapper packages
// (e.g. `@supramark/markdown-native-rn`) to register via a side effect.
// Web / Node never register one; plugin.ts falls back to wasm automatically.
// Exported only from the RN entry point (kept out of the web entry point), mirroring
// the `@supramark/engines/rn` pattern.
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
 * Internally uses the Rust `supramark-markdown` parser. The RN production entry
 * point may later wire in a native/TurboModule binding; the public contract stays
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
 */
export { presetDefault, presetGFM } from './plugin.js';

/**
 * Cache utilities.
 *
 * The RN renderer loads this entry point via Metro's react-native condition, so it
 * must keep the same cache public API as the default entry point.
 */
export { LRUCache, createCacheKey, simpleHash, type LRUCacheOptions } from './cache.js';

/**
 * Feature-related utility functions.
 */
export {
  isFeatureEnabled,
  getFeatureOptionsAs,
  getDiagramFeatureFamily,
  getDiagramFeatureIdsForEngine,
  isFeatureGroupEnabled,
  isDiagramFeatureEnabled,
} from './feature.js';
