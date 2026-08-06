/**
 * Engine identifier. The named members ('mermaid' | 'math' | 'dot' |
 * 'graphviz') are documentation hints; any string is accepted so hosts can
 * register custom engines. Kept widened to `string` intentionally.
 */
export type DiagramEngineType = string;

/** Public render result format. Successful renders are always SVG. */
export type DiagramRenderFormat = 'svg' | 'error';

export interface DiagramErrorInfo {
  code: 'render_error' | 'render_timeout' | 'unsupported_engine';
  message: string;
  details?: string;
}

export interface DiagramRenderResult {
  id: string;
  engine: DiagramEngineType;
  success: boolean;
  /** `success: true` returns `svg`; failures return `error`. */
  format: DiagramRenderFormat;
  payload: string;
  error?: DiagramErrorInfo;
}

export interface DiagramRenderService {
  render(params: {
    engine: DiagramEngineType;
    code: string;
    options?: Record<string, unknown>;
  }): Promise<DiagramRenderResult>;
}

export type GraphvizAttributeValue = string | number | boolean | { html: string };

export interface GraphvizImageSize {
  name: string;
  width: string | number;
  height: string | number;
}

export interface GraphvizDiagramOptions {
  engine?: string;
  layoutEngine?: string;
  graphvizEngine?: string;
  layout?: string;
  yInvert?: boolean;
  reduce?: boolean;
  graphAttributes?: Record<string, GraphvizAttributeValue>;
  nodeAttributes?: Record<string, GraphvizAttributeValue>;
  edgeAttributes?: Record<string, GraphvizAttributeValue>;
  images?: GraphvizImageSize[];
}

export interface GraphvizCapabilities {
  graphvizVersion?: string;
  engines?: string[];
  formats?: Array<'svg'>;
}

export interface GraphvizRenderAdapter {
  renderToSvg(code: string, options?: GraphvizDiagramOptions): Promise<string>;
  getCapabilities?(): Promise<GraphvizCapabilities>;
}

/**
 * A generic async render function: `(code, options?) => Promise<svgString>`.
 * Produced by engine factories (e.g. `echarts([core, SVGRenderer, ...])`).
 */
export type DiagramRenderFn = (
  code: string,
  options?: Record<string, unknown>
) => Promise<string>;

export interface DiagramEngineOptions {
  graphviz?: {
    adapter?: GraphvizRenderAdapter;
    loadAdapter?: () => Promise<GraphvizRenderAdapter>;
  };
  echarts?: {
    render?: DiagramRenderFn;
    loadRender?: () => Promise<DiagramRenderFn>;
  };
  vegaLite?: {
    render?: DiagramRenderFn;
    loadRender?: () => Promise<DiagramRenderFn>;
  };
  plantuml?: {
    render?: DiagramRenderFn;
    loadRender?: () => Promise<DiagramRenderFn>;
  };
  d2?: {
    render?: DiagramRenderFn;
    loadRender?: () => Promise<DiagramRenderFn>;
  };
}

// ============================================================================
// v0.2 — pure-function engine + config-driven codegen (see docs/architecture/ENGINES_AND_CLI_PLAN.zh.md)
// ============================================================================

/**
 * Common render options every engine recognizes.
 * Each engine's own Options type extends this with its own fields.
 */
export interface RenderOptions {
  /** Lets the host cancel long-running tasks; the engine calls `signal.throwIfAborted()` at key points */
  signal?: AbortSignal;
  /** Suggested output width (CSS px); the engine may ignore it or adapt */
  width?: number;
  /** Suggested output height (CSS px) */
  height?: number;
  /** Theme name; each engine maps it to its own theme system. 'light' / 'dark' are common values, but any string is accepted. */
  theme?: string;
}

/** Discrete category of render error, so a host can branch uniformly on `e.code`. */
export type ErrorCode =
  | 'parse_error' // input format is invalid (JSON/DOT/YAML etc. failed to parse)
  | 'render_error' // engine failed at runtime
  | 'engine_unavailable' // dependency not installed / runtime environment unsupported
  | 'aborted' // cancelled via AbortSignal
  | 'unsupported'; // the engine doesn't recognize this code kind (e.g. echarts has no matching chart type registered)

/**
 * The unified error type every engine throws on failure.
 *
 * @example
 * ```ts
 * try {
 *   await render(code);
 * } catch (e) {
 *   if (e instanceof DiagramRenderError) {
 *     // e.engine / e.code / e.input / e.cause
 *   }
 * }
 * ```
 */
export class DiagramRenderError extends Error {
  readonly engine: string;
  readonly code: ErrorCode;
  readonly input?: string;

  constructor(
    message: string,
    init: { engine: string; code: ErrorCode; input?: string; cause?: unknown }
  ) {
    super(message);
    this.name = 'DiagramRenderError';
    this.engine = init.engine;
    this.code = init.code;
    this.input = init.input;
    // tsconfig target = ES2019, Error's constructor doesn't yet take a { cause } param, so attach it manually.
    if (init.cause !== undefined) {
      (this as { cause?: unknown }).cause = init.cause;
    }
  }
}

/** Unified render function signature: `(code, options?) => Promise<svgString>`. */
export type RenderFn<O extends RenderOptions = RenderOptions> = (
  code: string,
  options?: O
) => Promise<string>;

/**
 * Unified engine factory signature.
 *
 * - Every engine's default export matches this shape;
 * - `modules` are assembly-time dependencies (chart type / adapter / vega runtime,
 *   etc.), an optional array;
 * - Returns a `RenderFn`, which the host stores in its engines map for Supramark
 *   to consume.
 */
export type EngineFactory<
  P = unknown[] | undefined,
  O extends RenderOptions = RenderOptions,
> = (modules?: P) => RenderFn<O>;
