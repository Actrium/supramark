import type { GraphvizRenderAdapter } from '../types.js';
import { pickGraphvizDiagramOptions } from './index.js';

let cached: Promise<GraphvizRenderAdapter> | null = null;

async function loadAdapter(): Promise<GraphvizRenderAdapter> {
  const { Graphviz } = await import('@kookyleo/graphviz-anywhere-web');
  const graphviz = await Graphviz.load();

  return {
    async renderToSvg(code, rawOptions) {
      const opt = pickGraphvizDiagramOptions(rawOptions);
      return graphviz.layout(code, 'svg', opt.layoutEngine ?? 'dot');
    },
    async getCapabilities() {
      return {
        graphvizVersion: graphviz.version(),
        engines: ['dot', 'neato', 'fdp', 'sfdp', 'circo', 'twopi', 'osage', 'patchwork'],
        formats: ['svg'],
      };
    },
  };
}

/**
 * Graphviz web adapter — lazy-loads the Embind wasm module on first use.
 * Each render allocates a fresh `CGraphviz` instance because the underlying
 * Graphviz context holds global state per render.
 */
const webAdapter: GraphvizRenderAdapter = {
  async renderToSvg(code, options) {
    if (!cached) cached = loadAdapter();
    const adapter = await cached;
    return adapter.renderToSvg(code, options);
  },
  async getCapabilities() {
    if (!cached) cached = loadAdapter();
    const adapter = await cached;
    return adapter.getCapabilities?.() ?? { engines: [], formats: ['svg'] };
  },
};

export default webAdapter;
