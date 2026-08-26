/**
 * WikiLink Feature
 *
 * @packageDocumentation
 */

export {
  wikilinkFeature,
  type WikiLinkFeatureOptions,
  type WikiLinkFeatureConfig,
  createWikilinkFeatureConfig,
  getWikilinkFeatureOptions,
} from './feature.js';
export { wikilinkExamples } from './examples.js';

// Re-export core types (for user convenience)
export type { SupramarkWikiLinkNode } from '@supramark/core';
