import { isGraphvizDiagramEngine, renderGraphvizSvg } from './graphviz';
import { renderMathJaxSvg } from './mathjax';
import { renderMermaidSvg } from './mermaid';
import type {
  DiagramErrorInfo,
  DiagramEngineOptions,
  DiagramEngineType,
  DiagramRenderFn,
  DiagramRenderResult,
  DiagramRenderService,
  GraphvizRenderAdapter,
} from './types';

class LocalDiagramEngine implements DiagramRenderService {
  private nextId = 0;
  private graphvizAdapterPromise: Promise<GraphvizRenderAdapter> | null = null;
  private echartsRenderPromise: Promise<DiagramRenderFn> | null = null;
  private vegaLiteRenderPromise: Promise<DiagramRenderFn> | null = null;
  private plantumlRenderPromise: Promise<DiagramRenderFn> | null = null;
  private d2RenderPromise: Promise<DiagramRenderFn> | null = null;

  constructor(private readonly options: DiagramEngineOptions = {}) {}

  async render(params: {
    engine: DiagramEngineType;
    code: string;
    options?: Record<string, unknown>;
  }): Promise<DiagramRenderResult> {
    const id = `de_${Date.now()}_${this.nextId++}`;
    const normalizedEngine = String(params.engine || '').toLowerCase();
    const timeoutMs = readRenderTimeout(params.options);

    const run = () => this.renderInner(params, id, normalizedEngine);

    try {
      if (timeoutMs > 0) {
        return await raceWithTimeout(run(), timeoutMs, normalizedEngine);
      }
      return await run();
    } catch (error) {
      const isTimeout = error instanceof RenderTimeoutError;
      const message = error instanceof Error ? error.message : String(error);
      return this.error(
        id,
        normalizedEngine,
        message,
        isTimeout ? 'render_timeout' : 'render_error',
        isTimeout
          ? `${params.engine} render timed out`
          : `${params.engine} rendering failed`,
        message
      );
    }
  }

  private async renderInner(
    params: {
      engine: DiagramEngineType;
      code: string;
      options?: Record<string, unknown>;
    },
    id: string,
    normalizedEngine: string
  ): Promise<DiagramRenderResult> {
    switch (normalizedEngine) {
      case 'mermaid': {
        const payload = await renderMermaidSvg(params.code, params.options);
        return this.svg(id, normalizedEngine, payload);
      }
      case 'math': {
        const payload = await renderMathJaxSvg(params.code, {
          displayMode: params.options?.displayMode === true,
        });
        return this.svg(id, normalizedEngine, payload);
      }
      case 'echarts': {
        const render = await this.getEchartsRender();
        if (!render) return this.unsupported(id, normalizedEngine, params.engine);
        const payload = await render(params.code, params.options);
        return this.svg(id, normalizedEngine, payload);
      }
      case 'vega-lite':
      case 'vegalite':
      case 'chart':
      case 'chartjs':
      case 'vega': {
        const render = await this.getVegaLiteRender();
        if (!render) return this.unsupported(id, normalizedEngine, params.engine);
        const opts =
          normalizedEngine === 'vega'
            ? { ...(params.options ?? {}), dialect: 'vega' as const }
            : params.options;
        const payload = await render(params.code, opts);
        return this.svg(id, normalizedEngine, payload);
      }
      case 'plantuml': {
        const render = await this.getPlantumlRender();
        if (!render) return this.unsupported(id, normalizedEngine, params.engine);
        const payload = await render(params.code, params.options);
        return this.svg(id, normalizedEngine, payload);
      }
      case 'd2': {
        const render = await this.getD2Render();
        if (!render) return this.unsupported(id, normalizedEngine, params.engine);
        const payload = await render(params.code, params.options);
        return this.svg(id, normalizedEngine, payload);
      }
      default: {
        if (isGraphvizDiagramEngine(normalizedEngine)) {
          const adapter = await this.getGraphvizAdapter();
          if (!adapter) {
            return this.error(
              id,
              normalizedEngine,
              'Graphviz adapter is not configured for @supramark/engines.',
              'unsupported_engine',
              `${params.engine} requires a Graphviz adapter`,
              'Use @supramark/engines/web or @supramark/engines/rn to create the engine.'
            );
          }

          const payload = await renderGraphvizSvg(params.code, params.options, adapter);
          return this.svg(id, normalizedEngine, payload);
        }

        return this.error(
          id,
          normalizedEngine,
          `Unsupported diagram engine: ${params.engine}`,
          'unsupported_engine',
          `${params.engine} is not supported by @supramark/engines`
        );
      }
    }
  }

  private async getGraphvizAdapter() {
    if (this.options.graphviz?.adapter) {
      return this.options.graphviz.adapter;
    }

    if (!this.options.graphviz?.loadAdapter) {
      return null;
    }

    if (!this.graphvizAdapterPromise) {
      this.graphvizAdapterPromise = this.options.graphviz.loadAdapter();
    }

    return this.graphvizAdapterPromise;
  }

  private async getEchartsRender(): Promise<DiagramRenderFn | null> {
    if (this.options.echarts?.render) return this.options.echarts.render;
    if (!this.options.echarts?.loadRender) return null;
    if (!this.echartsRenderPromise) {
      this.echartsRenderPromise = this.options.echarts.loadRender();
    }
    return this.echartsRenderPromise;
  }

  private async getVegaLiteRender(): Promise<DiagramRenderFn | null> {
    if (this.options.vegaLite?.render) return this.options.vegaLite.render;
    if (!this.options.vegaLite?.loadRender) return null;
    if (!this.vegaLiteRenderPromise) {
      this.vegaLiteRenderPromise = this.options.vegaLite.loadRender();
    }
    return this.vegaLiteRenderPromise;
  }

  private async getPlantumlRender(): Promise<DiagramRenderFn | null> {
    if (this.options.plantuml?.render) return this.options.plantuml.render;
    if (!this.options.plantuml?.loadRender) return null;
    if (!this.plantumlRenderPromise) {
      this.plantumlRenderPromise = this.options.plantuml.loadRender();
    }
    return this.plantumlRenderPromise;
  }

  private async getD2Render(): Promise<DiagramRenderFn | null> {
    if (this.options.d2?.render) return this.options.d2.render;
    if (!this.options.d2?.loadRender) return null;
    if (!this.d2RenderPromise) {
      this.d2RenderPromise = this.options.d2.loadRender();
    }
    return this.d2RenderPromise;
  }

  private svg(id: string, engine: string, payload: string): DiagramRenderResult {
    return {
      id,
      engine,
      success: true,
      format: 'svg',
      payload,
    };
  }

  private error(
    id: string,
    engine: string,
    payload: string,
    code: DiagramErrorInfo['code'],
    message: string,
    details?: string
  ): DiagramRenderResult {
    return {
      id,
      engine,
      success: false,
      format: 'error',
      payload,
      error: {
        code,
        message,
        details,
      },
    };
  }

  private unsupported(
    id: string,
    normalized: string,
    original: DiagramEngineType
  ): DiagramRenderResult {
    return this.error(
      id,
      normalized,
      `Unsupported diagram engine: ${original}`,
      'unsupported_engine',
      `${original} runtime not configured for @supramark/engines`,
      `Pass \`${normalized}: { render, loadRender }\` to createDiagramEngine() or ensure the peer dependency is installed.`
    );
  }
}

/**
 * Read the per-render timeout (ms) from forwarded render options.
 *
 * Renderers forward `engineConfig.timeoutMs` as `options.timeout` (see
 * `buildDiagramRenderOptions` / `buildRenderOptions` in web + rn). Both
 * `timeout` (the forwarded contract) and `timeoutMs` (a direct alias) are
 * accepted. Returns 0 when no positive timeout is set, meaning no race.
 */
function readRenderTimeout(options?: Record<string, unknown>): number {
  const raw = options?.timeout ?? options?.timeoutMs;
  return typeof raw === 'number' && raw > 0 && Number.isFinite(raw) ? raw : 0;
}

/** Thrown when a render exceeds its configured timeout. */
class RenderTimeoutError extends Error {
  constructor(engine: string, timeoutMs: number) {
    super(`${engine} render timed out after ${timeoutMs}ms`);
    this.name = 'RenderTimeoutError';
  }
}

/**
 * Race `work` against a timer. On timeout the work promise is left pending
 * (wasm renders can't be cancelled) but the race rejects with a
 * `RenderTimeoutError` so the caller can surface a `render_error` result
 * instead of waiting forever. The timer is cleared once `work` settles.
 */
function raceWithTimeout<T>(work: Promise<T>, timeoutMs: number, engine: string): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new RenderTimeoutError(engine, timeoutMs)), timeoutMs);
  });
  return Promise.race([work, timeout]).finally(() => {
    if (timer) clearTimeout(timer);
  });
}

export function createDiagramEngine(options?: DiagramEngineOptions): DiagramRenderService {
  return new LocalDiagramEngine(options);
}
