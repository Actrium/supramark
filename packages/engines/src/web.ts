import { createDiagramEngine } from './engine';
import { GRAPHVIZ_LAYOUT_ENGINES, pickGraphvizDiagramOptions } from './graphviz';
import { installHostMetricsBridge } from './host-bridge.js';
import { loadEchartsSvgRender, loadVegaLiteSvgRender } from './js-chart-loaders';
import type {
  DiagramEngineOptions,
  DiagramRenderFn,
  DiagramRenderService,
  GraphvizRenderAdapter,
} from './types';

export interface WebGraphvizAdapterOptions {
  adapter?: GraphvizRenderAdapter;
  loadAdapter?: () => Promise<GraphvizRenderAdapter>;
}

export interface WebDiagramEngineOptions extends DiagramEngineOptions {
  graphviz?: WebGraphvizAdapterOptions;
}

export function createWebDiagramEngine(
  options: WebDiagramEngineOptions = {}
): DiagramRenderService {
  const graphviz = options.graphviz ?? {};

  return createDiagramEngine({
    ...options,
    graphviz: {
      adapter: graphviz.adapter,
      loadAdapter: graphviz.loadAdapter ?? createWebGraphvizAdapterLoader(),
    },
    echarts: {
      render: options.echarts?.render,
      loadRender: options.echarts?.loadRender ?? loadEchartsSvgRender,
    },
    vegaLite: {
      render: options.vegaLite?.render,
      loadRender: options.vegaLite?.loadRender ?? loadVegaLiteSvgRender,
    },
    plantuml: {
      render: options.plantuml?.render,
      loadRender: options.plantuml?.loadRender ?? loadWebPlantumlRender,
    },
    d2: {
      render: options.d2?.render,
      loadRender: options.d2?.loadRender ?? loadWebD2Render,
    },
  });
}

/**
 * Default web-side lazy loader for PlantUML.
 *
 * Loads `@kookyleo/plantuml-little-web` (Rust → wasm) on first use and
 * returns a `RenderFn`. The wasm binary initialises as a side effect of the
 * ES-module import (`import * as wasm from "./plantuml_little_web_bg.wasm"`).
 *
 * Graphviz bridge contract (see `packages/web/src/lib.rs`):
 *
 *   globalThis.__graphviz_anywhere_render(dot, engine, format) -> string
 *
 * `plantuml-little-web` delegates layout for component / activity / state /
 * use-case diagrams to Graphviz via this global. We install a synchronous
 * wrapper backed by `@kookyleo/graphviz-anywhere-web` (pre-loaded) so the
 * wasm call site can invoke it without returning to the JS event loop.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function loadWebPlantumlRender(): Promise<DiagramRenderFn> {
  // Install the host text-metrics bridge before loading the wasm so the
  // wasm's metrics-host-callback impl can resolve `supramark.measureText`
  // on first render. Idempotent.
  installHostMetricsBridge();

  let plantumlPromise: Promise<DiagramRenderFn> | null = null;
  let graphvizBridgePromise: Promise<void> | null = null;

  const ensureGraphvizBridge = async () => {
    if (!graphvizBridgePromise) {
      graphvizBridgePromise = (async () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const { Graphviz } = await import('@kookyleo/graphviz-anywhere-web' as string);
        const graphviz = await Graphviz.load();

        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const g = globalThis as any;
        if (typeof g.__graphviz_anywhere_render !== 'function') {
          g.__graphviz_anywhere_render = (
            dot: string,
            engine?: string,
            format?: string
          ): string => {
            return graphviz.layout(dot, (format ?? 'svg') as string, (engine ?? 'dot') as string);
          };
        }
      })();
    }

    return graphvizBridgePromise;
  };

  const loadPlantuml = async (): Promise<DiagramRenderFn> => {
    if (!plantumlPromise) {
      plantumlPromise = (async () => {
        // Load the wasm module. wasm-bindgen's ESM-wasm build initialises via
        // the `import * from '*.wasm'` side effect, so no separate init call is
        // needed. Some builds still ship a default `init()` — probe defensively.
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const puml: any = await import('@kookyleo/plantuml-little-web' as string);

        const init =
          (typeof puml.default === 'function' && puml.default) ||
          (typeof puml.init === 'function' && puml.init) ||
          null;
        if (init) {
          try {
            await init();
          } catch {
            // Already initialised via the module-import side effect — ignore.
          }
        }

        const convert =
          (typeof puml.convert === 'function' && puml.convert) ||
          (typeof puml.render === 'function' && puml.render) ||
          (typeof puml.renderSvg === 'function' && puml.renderSvg) ||
          null;
        if (!convert) {
          throw new Error(
            '`@kookyleo/plantuml-little-web` is missing a convert / render entry. Expected one of: convert, render, renderSvg.'
          );
        }

        return async (code: string): Promise<string> => {
          // `convert` is synchronous (wasm-bindgen-generated) but `await` handles
          // both sync and async return shapes uniformly.
          const svg = await convert(code);
          const normalized = String(svg ?? '');
          if (!normalized.includes('<svg')) {
            throw new Error('PlantUML renderer did not return SVG output.');
          }
          return normalized;
        };
      })();
    }
    return plantumlPromise;
  };

  return async (code: string): Promise<string> => {
    if (plantumlNeedsGraphviz(code)) {
      await ensureGraphvizBridge();
    }
    const render = await loadPlantuml();
    return render(code);
  };
}

function plantumlNeedsGraphviz(code: string): boolean {
  const normalized = code.toLowerCase();
  const hasSequenceArrow = /(^|\n)\s*[\w.$"'[\] -]+\s*(?:--?|==?|\.\.)[>x]/.test(normalized);
  const hasSequenceKeyword =
    /(^|\n)\s*(actor|participant|boundary|control|entity|queue|collections?)\b/.test(normalized);
  const hasGraphLayoutKeyword =
    /(^|\n)\s*(abstract\s+class|class|interface|enum|annotation|component|state|usecase|object|package|node|artifact|folder|frame|cloud|database|rectangle|storage|agent|card)\b/.test(
      normalized
    );
  const hasActivityKeyword =
    /(^|\n)\s*(start|stop|if\s*\(|while\s*\(|repeat\b|fork\b|partition\b|:[^;\n]+;)/.test(
      normalized
    );

  if (hasGraphLayoutKeyword || hasActivityKeyword) return true;
  if (hasSequenceArrow || hasSequenceKeyword) return false;
  return true;
}

/**
 * Default web-side lazy loader for D2.
 *
 * Loads `@kookyleo/d2-little-web` (Rust → wasm) on first use and returns a
 * `RenderFn`. Unlike plantuml-little-web, d2-little ships a pure-Rust layout
 * engine so there is no Graphviz bridge to wire — this loader is a thin
 * adapter over the wasm module's `convert(code) -> svg` entry.
 *
 * The wasm binary initialises as a side effect of the ES-module import
 * (`import * as wasm from "./d2_little_web_bg.wasm"`). Some wasm-bindgen builds
 * still ship a default `init()` — we probe defensively and `await` it if
 * present, swallowing errors caused by re-init.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function loadWebD2Render(): Promise<DiagramRenderFn> {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const d2: any = await import('@kookyleo/d2-little-web' as string);

  const init =
    (typeof d2.default === 'function' && d2.default) ||
    (typeof d2.init === 'function' && d2.init) ||
    null;
  if (init) {
    try {
      await init();
    } catch {
      // Already initialised via the module-import side effect — ignore.
    }
  }

  const convert =
    (typeof d2.convert === 'function' && d2.convert) ||
    (typeof d2.render === 'function' && d2.render) ||
    (typeof d2.renderSvg === 'function' && d2.renderSvg) ||
    null;
  if (!convert) {
    throw new Error(
      '`@kookyleo/d2-little-web` is missing a convert / render entry. Expected one of: convert, render, renderSvg.'
    );
  }

  return async (code: string): Promise<string> => {
    // `convert` is synchronous (wasm-bindgen-generated) but `await` handles
    // both sync and async return shapes uniformly.
    const svg = await convert(code);
    const normalized = String(svg ?? '');
    if (!normalized.includes('<svg')) {
      throw new Error('D2 renderer did not return SVG output.');
    }
    // d2-little ships an SVG with only `viewBox`, no width/height (upstream
    // design: meant to fit any container when used as a standalone file).
    // When embedded as inline SVG via dangerouslySetInnerHTML, browsers
    // stretch width to fill the parent and scale height by aspect ratio,
    // which blows up extreme viewBoxes. Inject width/height from viewBox
    // so the SVG renders at its intrinsic size (CSS can shrink it if needed).
    return injectD2Dimensions(normalized);
  };
}

function injectD2Dimensions(svg: string): string {
  const openTag = svg.match(/<svg\b[^>]*>/);
  if (!openTag) return svg;
  const tag = openTag[0];
  if (/\swidth=/.test(tag) || /\sheight=/.test(tag)) return svg;
  const vb = tag.match(/viewBox="([^"]+)"/);
  if (!vb) return svg;
  const parts = vb[1].trim().split(/\s+/).map(Number);
  if (parts.length !== 4 || parts.some(n => !Number.isFinite(n))) return svg;
  const [, , w, h] = parts;
  if (w <= 0 || h <= 0) return svg;
  const replaced = tag.replace('<svg', `<svg width="${w}" height="${h}"`);
  return svg.replace(tag, replaced);
}

function createWebGraphvizAdapterLoader(): () => Promise<GraphvizRenderAdapter> {
  let adapterPromise: Promise<GraphvizRenderAdapter> | null = null;

  return () => {
    if (!adapterPromise) {
      adapterPromise = loadWebGraphvizAdapter();
    }
    return adapterPromise;
  };
}

async function loadWebGraphvizAdapter(): Promise<GraphvizRenderAdapter> {
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

export { GRAPHVIZ_LAYOUT_ENGINES };
export { loadWebPlantumlRender };
export { loadWebD2Render };
