import type { GraphvizRenderAdapter } from '../types.js';
import { pickGraphvizDiagramOptions } from './index.js';

let cached: Promise<GraphvizRenderAdapter> | null = null;

async function buildAdapter(): Promise<GraphvizRenderAdapter> {
  const { Graphviz } = await import('@actrium/graphviz-anywhere-web');
  const graphviz = await Graphviz.load();

  return {
    renderToSvg(code, rawOptions) {
      const opt = pickGraphvizDiagramOptions(rawOptions);
      return Promise.resolve(graphviz.layout(code, 'svg', opt.layoutEngine ?? 'dot'));
    },
    getCapabilities() {
      return Promise.resolve({
        graphvizVersion: graphviz.version(),
        engines: ['dot', 'neato', 'fdp', 'sfdp', 'circo', 'twopi', 'osage', 'patchwork'],
        formats: ['svg'] as Array<'svg'>,
      });
    },
  };
}

/** Resolve the cached adapter, dropping the cache on rejection so a transient
 * wasm load failure is retried on the next call. See #161. */
function getAdapter(): Promise<GraphvizRenderAdapter> {
  if (!cached) {
    const promise = buildAdapter();
    promise.catch(() => {
      if (cached === promise) cached = null;
    });
    cached = promise;
  }
  return cached;
}

/**
 * Graphviz web adapter — lazy-loads the Embind wasm module on first use.
 * Each render allocates a fresh `CGraphviz` instance because the underlying
 * Graphviz context holds global state per render.
 */
const webAdapter: GraphvizRenderAdapter = {
  async renderToSvg(code, options) {
    const adapter = await getAdapter();
    return adapter.renderToSvg(code, options);
  },
  async getCapabilities() {
    const adapter = await getAdapter();
    return adapter.getCapabilities?.() ?? { engines: [], formats: ['svg'] };
  },
};

export default webAdapter;
