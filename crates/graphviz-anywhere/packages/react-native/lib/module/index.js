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

/** Shape of the codegen'd TurboModule spec module (CommonJS interop). */

// Load the codegen'd TurboModule, tolerating its absence (old arch or a
// host that hasn't run codegen). Kept separate from the pure selection so
// the latter stays unit-testable.
function loadTurboModule() {
  try {
    return require('./NativeGraphviz').default ?? undefined;
  } catch {
    // TurboModules not available, fall through
    return undefined;
  }
}

/**
 * Resolve the native module, preferring TurboModules (new arch) with fallback
 * to the bridge-based NativeModules (old arch). When neither is linked, return
 * a Proxy that throws an actionable error on first use rather than at import
 * time. Kept a pure function of its inputs so the fallback order is testable.
 */
export function resolveNative(turbo, bridged) {
  if (turbo) return turbo;
  if (!bridged) {
    return new Proxy({}, {
      get() {
        throw new Error(LINKING_ERROR);
      }
    });
  }
  return bridged;
}
const GraphvizNative = resolveNative(loadTurboModule(), NativeModules.GraphvizNative);

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