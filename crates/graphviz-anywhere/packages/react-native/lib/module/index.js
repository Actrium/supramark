"use strict";

import { NativeModules, Platform } from 'react-native';
const LINKING_ERROR = `The package '@actrium/graphviz-anywhere-rn' doesn't seem to be linked. Make sure:\n\n` + Platform.select({
  ios: '- You have run `pod install`\n',
  macos: '- You have run `pod install`\n',
  android: '',
  windows: '',
  default: ''
}) + '- You rebuilt the app after installing the package\n' + '- You are not using Expo Go\n';

/**
 * Error codes returned by the native Graphviz module.
 */
export const GraphvizErrorCode = {
  NULL_INPUT: 'NULL_INPUT',
  INVALID_DOT: 'INVALID_DOT',
  LAYOUT_FAILED: 'LAYOUT_FAILED',
  RENDER_FAILED: 'RENDER_FAILED',
  INVALID_ENGINE: 'INVALID_ENGINE',
  INVALID_FORMAT: 'INVALID_FORMAT',
  OUT_OF_MEMORY: 'OUT_OF_MEMORY',
  NOT_INITIALIZED: 'NOT_INITIALIZED',
  UNKNOWN: 'UNKNOWN'
};

/**
 * Layout engines supported by Graphviz.
 */

/**
 * Output formats supported by Graphviz.
 */

/**
 * Resolve the native module, preferring TurboModules (new arch) with
 * fallback to the bridge-based NativeModules (old arch).
 */
function getNativeModule() {
  // Try TurboModule first (new architecture)
  try {
    const turbo = require('./NativeGraphviz').default;
    if (turbo) {
      return turbo;
    }
  } catch {
    // TurboModules not available, fall through
  }

  // Fallback to old architecture NativeModules
  const nativeModule = NativeModules.GraphvizNative;
  if (!nativeModule) {
    throw new Error(LINKING_ERROR);
  }
  return nativeModule;
}
const GraphvizNative = getNativeModule();

/**
 * Render a DOT language string into the specified output format.
 *
 * All rendering is performed on a background thread and the result
 * is delivered asynchronously via a Promise.
 *
 * @param dot - DOT language string describing the graph
 * @param engine - Layout engine to use (default: "dot")
 * @param format - Output format (default: "svg")
 * @returns Promise resolving to the rendered output string.
 *          For text formats (svg, json, dot, xdot, plain) the raw text is returned.
 *          For binary formats (png, pdf, ps) the output is base64-encoded.
 */
export async function renderDot(dot, engine = 'dot', format = 'svg') {
  return GraphvizNative.renderDot(dot, engine, format);
}

/**
 * Get the Graphviz library version string.
 *
 * @returns Promise resolving to the version string (e.g. "12.2.1")
 */
export async function getVersion() {
  return GraphvizNative.getVersion();
}
export default {
  renderDot,
  getVersion,
  GraphvizErrorCode
};
//# sourceMappingURL=index.js.map