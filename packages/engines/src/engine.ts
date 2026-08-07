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

    try {
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
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return this.error(
        id,
        normalizedEngine,
        message,
        'render_error',
        `${params.engine} rendering failed`,
        message
      );
    }
  }

  /**
   * Cache a lazily-loaded promise, dropping it on rejection so a transient
   * load failure (chunk 404, wasm init error) is retried on the next render
   * instead of permanently bricking the engine — `defaultDiagramEngine` is a
   * module-level singleton, so a pinned rejection is unrecoverable without a
   * full page reload. The `current() === promise` guard is a no-op invariant
   * today — the `.catch` below is the only code that nulls the slot and it
   * fires exactly once, so no retry can replace the slot before it runs. It
   * stays as a load-bearing check for a future `dispose()`/`reset()` that
   * could null the slot out from under a still-pending rejection. See #161.
   */
  private cacheRetryableLoad<T>(
    current: () => Promise<T> | null,
    store: (p: Promise<T> | null) => void,
    load: () => Promise<T>
  ): Promise<T> {
    const cached = current();
    if (cached) return cached;
    const promise = load();
    promise.catch(() => {
      if (current() === promise) store(null);
    });
    store(promise);
    return promise;
  }

  private async getGraphvizAdapter() {
    const adapter = this.options.graphviz?.adapter;
    if (adapter) return adapter;
    const loadAdapter = this.options.graphviz?.loadAdapter;
    if (!loadAdapter) return null;
    return this.cacheRetryableLoad(
      () => this.graphvizAdapterPromise,
      p => {
        this.graphvizAdapterPromise = p;
      },
      loadAdapter
    );
  }

  private async getEchartsRender(): Promise<DiagramRenderFn | null> {
    const render = this.options.echarts?.render;
    if (render) return render;
    const loadRender = this.options.echarts?.loadRender;
    if (!loadRender) return null;
    return this.cacheRetryableLoad(
      () => this.echartsRenderPromise,
      p => {
        this.echartsRenderPromise = p;
      },
      loadRender
    );
  }

  private async getVegaLiteRender(): Promise<DiagramRenderFn | null> {
    const render = this.options.vegaLite?.render;
    if (render) return render;
    const loadRender = this.options.vegaLite?.loadRender;
    if (!loadRender) return null;
    return this.cacheRetryableLoad(
      () => this.vegaLiteRenderPromise,
      p => {
        this.vegaLiteRenderPromise = p;
      },
      loadRender
    );
  }

  private async getPlantumlRender(): Promise<DiagramRenderFn | null> {
    const render = this.options.plantuml?.render;
    if (render) return render;
    const loadRender = this.options.plantuml?.loadRender;
    if (!loadRender) return null;
    return this.cacheRetryableLoad(
      () => this.plantumlRenderPromise,
      p => {
        this.plantumlRenderPromise = p;
      },
      loadRender
    );
  }

  private async getD2Render(): Promise<DiagramRenderFn | null> {
    const render = this.options.d2?.render;
    if (render) return render;
    const loadRender = this.options.d2?.loadRender;
    if (!loadRender) return null;
    return this.cacheRetryableLoad(
      () => this.d2RenderPromise,
      p => {
        this.d2RenderPromise = p;
      },
      loadRender
    );
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

export function createDiagramEngine(options?: DiagramEngineOptions): DiagramRenderService {
  return new LocalDiagramEngine(options);
}
