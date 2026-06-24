import { createDiagramEngine } from './engine';
import {
  GRAPHVIZ_LAYOUT_ENGINES,
  pickGraphvizDiagramOptions,
  resolveGraphvizAnywhereRnExports,
} from './graphviz';
import { loadEchartsSvgRender, loadVegaLiteSvgRender } from './js-chart-loaders';
import { getNativeEngineAdapter, renderViaNative } from './rn-native-adapter';
import type {
  DiagramEngineOptions,
  DiagramRenderResult,
  DiagramRenderService,
  GraphvizCapabilities,
  GraphvizRenderAdapter,
} from './types';

export interface ReactNativeGraphvizAdapterOptions {
  adapter?: GraphvizRenderAdapter;
  loadAdapter?: () => Promise<GraphvizRenderAdapter>;
}

export interface ReactNativeDiagramEngineOptions extends DiagramEngineOptions {
  graphviz?: ReactNativeGraphvizAdapterOptions;
}

/**
 * Construct a React Native diagram engine.
 *
 * Routing precedence per render call:
 *   1. **Native engine adapter** (`registerNativeEngineAdapter(...)`) —
 *      installed by the host with the engine's native FFI module.
 *      This is the path for d2 / mermaid / plantuml on iOS / Android
 *      once their `@actrium/supramark-<engine>-native-rn` package
 *      installs an adapter.
 *   2. **Graphviz layout adapter** (`options.graphviz.{adapter,loadAdapter}`) —
 *      defaults to `@actrium/graphviz-anywhere-rn`.
 *   3. **JS SVG engines** — ECharts and Vega/Vega-Lite use the same
 *      source -> SVG-string route as Web.
 *   4. **Inner engine** — for unrecognised engines, falls through to the
 *      cross-platform `createDiagramEngine` unsupported response.
 */
export function createReactNativeDiagramEngine(
  options: ReactNativeDiagramEngineOptions = {}
): DiagramRenderService {
  const graphviz = options.graphviz ?? {};
  const inner = createDiagramEngine({
    ...options,
    graphviz: {
      adapter: graphviz.adapter,
      loadAdapter: graphviz.loadAdapter ?? createReactNativeGraphvizAdapterLoader(),
    },
    echarts: {
      render: options.echarts?.render,
      loadRender: options.echarts?.loadRender ?? loadEchartsSvgRender,
    },
    vegaLite: {
      render: options.vegaLite?.render,
      loadRender: options.vegaLite?.loadRender ?? loadVegaLiteSvgRender,
    },
  });

  let nextId = 0;

  return {
    async render(params): Promise<DiagramRenderResult> {
      const engine = String(params.engine || '').toLowerCase();
      const adapter = getNativeEngineAdapter(engine);

      if (adapter) {
        const id = `rn_${Date.now()}_${nextId++}`;
        try {
          const payload = await renderViaNative(engine, params.code, params.options);
          if (payload == null) {
            // Shouldn't happen — getNativeEngineAdapter returned an
            // adapter so renderViaNative must succeed. Defensive only.
            return inner.render(params);
          }
          return { id, engine, success: true, format: 'svg', payload };
        } catch (err) {
          return {
            id,
            engine,
            success: false,
            format: 'error',
            payload: err instanceof Error ? err.message : String(err),
            error: {
              code: 'render_error',
              message:
                err instanceof Error
                  ? err.message
                  : `Native engine adapter for "${engine}" threw a non-Error value`,
              details: `engine=${engine} via registered native FFI adapter`,
            },
          };
        }
      }

      return inner.render(params);
    },
  };
}

function createReactNativeGraphvizAdapterLoader(): () => Promise<GraphvizRenderAdapter> {
  let adapterPromise: Promise<GraphvizRenderAdapter> | null = null;

  return () => {
    if (!adapterPromise) {
      adapterPromise = loadReactNativeGraphvizAdapter();
    }
    return adapterPromise;
  };
}

async function loadReactNativeGraphvizAdapter(): Promise<GraphvizRenderAdapter> {
  const mod = await import('@actrium/graphviz-anywhere-rn');
  // Tolerate the CJS/ESM interop shapes Metro produces for this package's
  // CommonJS main; see resolveGraphvizAnywhereRnExports.
  const { renderDot, getVersion } = resolveGraphvizAnywhereRnExports(mod);

  return {
    async renderToSvg(code, rawOptions) {
      const graphvizOptions = pickGraphvizDiagramOptions(rawOptions);
      return renderDot(code, graphvizOptions.layoutEngine ?? 'dot', 'svg');
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

export { GRAPHVIZ_LAYOUT_ENGINES };
export {
  registerNativeEngineAdapter,
  getNativeEngineAdapter,
  listNativeEngines,
  renderViaNative,
  type NativeEngineAdapter,
  type NativeRenderFn,
} from './rn-native-adapter';
