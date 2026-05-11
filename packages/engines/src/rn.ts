import { createDiagramEngine } from './engine';
import {
  GRAPHVIZ_LAYOUT_ENGINES,
  pickGraphvizDiagramOptions,
} from './graphviz';
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
 *      once their `@kookyleo/supramark-<engine>-native-rn` package
 *      installs an adapter.
 *   2. **Graphviz layout adapter** (`options.graphviz.{adapter,loadAdapter}`) —
 *      defaults to `@kookyleo/graphviz-anywhere-rn`.
 *   3. **Inner engine** — for unrecognised engines, falls through to
 *      the cross-platform `createDiagramEngine` (most pure-JS engines
 *      report "unsupported on RN" here).
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
  const module = await import('@kookyleo/graphviz-anywhere-rn');

  return {
    async renderToSvg(code, rawOptions) {
      const graphvizOptions = pickGraphvizDiagramOptions(rawOptions);
      return module.renderDot(code, (graphvizOptions.layoutEngine ?? 'dot') as any, 'svg');
    },
    async getCapabilities(): Promise<GraphvizCapabilities> {
      return {
        graphvizVersion:
          typeof module.getVersion === 'function' ? await module.getVersion() : undefined,
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
