/**
 * Video Feature
 *
 * A video embed container configured by a JSON body
 *
 * @packageDocumentation
 */

// Feature definition (main export)
export {
  videoFeature,
  VIDEO_CONTAINER_NAMES,
  type VideoContainerName,
  type VideoData,
} from './feature.js';

// Examples
export { videoExamples } from './examples.js';

// Renderers (for the registry to use)
export { renderVideoContainerWeb } from './runtime.web.js';
export { renderVideoContainerRN } from './runtime.rn.js';
