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

async function loadAdapter(): Promise<GraphvizRenderAdapter> {
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

/**
 * Graphviz RN adapter — thin wrapper over `@actrium/graphviz-anywhere-rn`'s
 * native module (JSI TurboModule on new arch, NativeModule bridge on old arch).
 * First call triggers native initialization.
 */
const rnAdapter: GraphvizRenderAdapter = {
  async renderToSvg(code, options) {
    if (!cached) cached = loadAdapter();
    const adapter = await cached;
    return adapter.renderToSvg(code, options);
  },
  async getCapabilities() {
    if (!cached) cached = loadAdapter();
    const adapter = await cached;
    return adapter.getCapabilities?.() ?? { engines: [...GRAPHVIZ_LAYOUT_ENGINES], formats: ['svg'] };
  },
};

export default rnAdapter;
