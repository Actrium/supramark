export type DiagramEngineType = 'mermaid' | 'math' | 'dot' | 'graphviz' | string;

/** Public render result format. Successful renders are always SVG. */
export type DiagramRenderFormat = 'svg' | 'error';

export interface DiagramErrorInfo {
  code: 'render_error' | 'unsupported_engine';
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
// v0.2 — 纯函数 engine + config-driven codegen（见 docs/architecture/ENGINES_AND_CLI_PLAN.md）
// ============================================================================

/**
 * 所有 engine 都识别的共同渲染选项。
 * 各 engine 的 Options 通过 extends 扩展自家字段。
 */
export interface RenderOptions {
  /** 允许 host 取消长任务，engine 在关键点调 `signal.throwIfAborted()` */
  signal?: AbortSignal;
  /** 建议的输出宽度（CSS px）；engine 可忽略或自适应 */
  width?: number;
  /** 建议的输出高度（CSS px） */
  height?: number;
  /** light / dark / 自定义主题名；engine 各自映射到自家 theme 体系 */
  theme?: 'light' | 'dark' | string;
}

/** 渲染错误的离散类别，host 用 `e.code` 做统一分流。 */
export type ErrorCode =
  | 'parse_error' // 输入格式非法（JSON/DOT/YAML 等解析失败）
  | 'render_error' // engine 运行期失败
  | 'engine_unavailable' // 依赖未装 / 运行环境不支持
  | 'aborted' // 被 AbortSignal 取消
  | 'unsupported'; // engine 不认识该 code 种类（比如 echarts 未注册对应 chart type）

/**
 * 所有 engine 失败时抛的统一错误类型。
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
    // tsconfig target = ES2019，Error 构造还没 { cause } 入参，手动挂。
    if (init.cause !== undefined) {
      (this as { cause?: unknown }).cause = init.cause;
    }
  }
}

/** 统一的 render 函数签名：`(code, options?) => Promise<svgString>`。 */
export type RenderFn<O extends RenderOptions = RenderOptions> = (
  code: string,
  options?: O
) => Promise<string>;

/**
 * 统一的 engine 工厂签名。
 *
 * - 每个 engine 的默认导出都符合这个形状；
 * - `modules` 为装配期依赖（chart type / adapter / vega runtime 等），可选数组；
 * - 返回一个 `RenderFn`，host 存进 engines map 供 Supramark 消费。
 */
export type EngineFactory<
  P = unknown[] | undefined,
  O extends RenderOptions = RenderOptions,
> = (modules?: P) => RenderFn<O>;
