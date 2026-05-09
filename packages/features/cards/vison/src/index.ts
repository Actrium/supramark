/**
 * @supramark/feature-card-vison — render :::vison container blocks as
 * Vison cards (JSON visual description spec for AI chat UIs).
 */

export {
  visonFeature,
  type VisonSpec,
  type VisonContainerData,
  type SupramarkVisonContainerNode,
  type VisonFeatureOptions,
  type VisonFeatureConfig,
  createVisonFeatureConfig,
  getVisonFeatureOptions,
} from './feature.js';
export { visonExamples } from './examples.js';

// Runtime: register the :::vison container hook on import.
import './runtime.js';
