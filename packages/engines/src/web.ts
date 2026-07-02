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

/**
 * Minimal shape of a wasm-bindgen ESM module probed defensively for an
 * init entry and a sync convert/render entry.
 */
/** wasm-bindgen init entry: may take a wasm URL/bytes, returns sync or async. */
type WasmInitFn = (...args: unknown[]) => unknown;
/** wasm-bindgen convert/render entry: `(code) => svg`, sync or async. */
type WasmConvertFn = (code: string) => string | Promise<string>;

interface WasmRenderModule {
  default?: unknown;
  init?: unknown;
  convert?: unknown;
  render?: unknown;
  renderSvg?: unknown;
}

/** A loaded `@actrium/graphviz-anywhere-web` Graphviz instance. */
interface GraphvizInstance {
  layout(dot: string, format: string, engine: string): string;
  version(): string;
}

/** Minimal surface of the `@actrium/graphviz-anywhere-web` ESM module. */
interface GraphvizWebModule {
  Graphviz: { load(): Promise<GraphvizInstance> };
}

const GRAPHVIZ_WEB_SPEC = '@actrium/graphviz-anywhere-web';

/** Probe a wasm-bindgen module for its optional `default`/`init` entry. */
function pickWasmInit(mod: WasmRenderModule): WasmInitFn | null {
  if (typeof mod.default === 'function') return mod.default as WasmInitFn;
  if (typeof mod.init === 'function') return mod.init as WasmInitFn;
  return null;
}

/** Probe a wasm-bindgen module for a `convert`/`render`/`renderSvg` entry. */
function pickWasmConvert(mod: WasmRenderModule): WasmConvertFn | null {
  if (typeof mod.convert === 'function') return mod.convert as WasmConvertFn;
  if (typeof mod.render === 'function') return mod.render as WasmConvertFn;
  if (typeof mod.renderSvg === 'function') return mod.renderSvg as WasmConvertFn;
  return null;
}

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
 * Loads `@actrium/plantuml-little-web` (Rust → wasm) on first use and
 * returns a `RenderFn`. The wasm binary initialises as a side effect of the
 * ES-module import (`import * as wasm from "./plantuml_little_web_bg.wasm"`).
 *
 * Graphviz bridge contract (see `packages/web/src/lib.rs`):
 *
 *   globalThis.__graphviz_anywhere_render(dot, engine, format) -> string
 *
 * `plantuml-little-web` delegates layout for component / activity / state /
 * use-case diagrams to Graphviz via this global. We install a synchronous
 * wrapper backed by `@actrium/graphviz-anywhere-web` (pre-loaded) so the
 * wasm call site can invoke it without returning to the JS event loop.
 */
// loadRender contract returns Promise<DiagramRenderFn>; this loader only wires
// up closures and resolves the render fn lazily, so no top-level await is used.
// eslint-disable-next-line @typescript-eslint/require-await
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
        const spec: string = GRAPHVIZ_WEB_SPEC;
        const { Graphviz } = (await import(spec)) as GraphvizWebModule;
        const graphviz = await Graphviz.load();

        const g = globalThis as unknown as {
          __graphviz_anywhere_render?: (dot: string, engine?: string, format?: string) => string;
        };
        if (typeof g.__graphviz_anywhere_render !== 'function') {
          g.__graphviz_anywhere_render = (
            dot: string,
            engine?: string,
            format?: string
          ): string => {
            return graphviz.layout(dot, format ?? 'svg', engine ?? 'dot');
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
        const puml = (await import(
          '@actrium/plantuml-little-web' as string
        )) as WasmRenderModule;

        const init = pickWasmInit(puml);
        if (init) {
          try {
            await init();
          } catch {
            // Already initialised via the module-import side effect — ignore.
          }
        }

        const convert = pickWasmConvert(puml);
        if (!convert) {
          throw new Error(
            '`@actrium/plantuml-little-web` is missing a convert / render entry. Expected one of: convert, render, renderSvg.'
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
 * Loads `@actrium/d2-little-web` (Rust → wasm) on first use and returns a
 * `RenderFn`. Unlike plantuml-little-web, d2-little ships a pure-Rust layout
 * engine so there is no Graphviz bridge to wire — this loader is a thin
 * adapter over the wasm module's `convert(code) -> svg` entry.
 *
 * The wasm binary initialises as a side effect of the ES-module import
 * (`import * as wasm from "./d2_little_web_bg.wasm"`). Some wasm-bindgen builds
 * still ship a default `init()` — we probe defensively and `await` it if
 * present, swallowing errors caused by re-init.
 */
async function loadWebD2Render(): Promise<DiagramRenderFn> {
  // d2 wasm 通过 globalThis.supramark.measureText 量字宽；bridge 未安装时
  // wasm-bindgen catch 路径 fallback 到 size*0.6 启发式，导致 layout 偏差。
  installHostMetricsBridge();

  const d2 = (await import('@actrium/d2-little-web' as string)) as WasmRenderModule;

  const init = pickWasmInit(d2);
  if (init) {
    try {
      await init();
    } catch {
      // Already initialised via the module-import side effect — ignore.
    }
  }

  const convert = pickWasmConvert(d2);
  if (!convert) {
    throw new Error(
      '`@actrium/d2-little-web` is missing a convert / render entry. Expected one of: convert, render, renderSvg.'
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

export { GRAPHVIZ_LAYOUT_ENGINES };
export { loadWebPlantumlRender };
export { loadWebD2Render };
