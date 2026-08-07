import type {
  GraphvizCapabilities,
  GraphvizRenderAdapter,
} from '../types.js';
import {
  GRAPHVIZ_LAYOUT_ENGINES,
  pickGraphvizDiagramOptions,
  resolveGraphvizAnywhereRnExports,
} from './index.js';

let cached: Promise<GraphvizRenderAdapter> | null = null;

async function buildAdapter(): Promise<GraphvizRenderAdapter> {
  const mod = await import('@actrium/graphviz-anywhere-rn');
  // Tolerate the CJS/ESM interop shapes Metro produces for this package's
  // CommonJS main; see resolveGraphvizAnywhereRnExports.
  const { renderDot, getVersion } = resolveGraphvizAnywhereRnExports(mod);

  return {
    async renderToSvg(code, rawOptions) {
      const opt = pickGraphvizDiagramOptions(rawOptions);
      return renderDot(code, opt.layoutEngine ?? 'dot', 'svg');
    },
    async getCapabilities(): Promise<GraphvizCapabilities> {
      return {
        graphvizVersion: getVersion ? await getVersion() : undefined,
        engines: [...GRAPHVIZ_LAYOUT_ENGINES],
        formats: ['svg'],
      };
    },
  };
}

/** Resolve the cached adapter, dropping the cache on rejection so a transient
 * native-module init failure is retried on the next call. See #161. */
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
 * Graphviz RN adapter — thin wrapper over `@actrium/graphviz-anywhere-rn`'s
 * native module (JSI TurboModule on new arch, NativeModule bridge on old arch).
 * First call triggers native initialization.
 */
const rnAdapter: GraphvizRenderAdapter = {
  async renderToSvg(code, options) {
    const adapter = await getAdapter();
    return adapter.renderToSvg(code, options);
  },
  async getCapabilities() {
    const adapter = await getAdapter();
    return adapter.getCapabilities?.() ?? { engines: [...GRAPHVIZ_LAYOUT_ENGINES], formats: ['svg'] };
  },
};

export default rnAdapter;
