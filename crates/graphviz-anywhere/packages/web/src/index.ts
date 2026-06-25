import {
  DEFAULT_ENGINE,
  type GraphvizBatchDiagnostics,
  type GraphvizCapabilities,
  type GraphvizRenderDiagnostics,
  type GraphvizRenderOptions,
  type GraphvizWebErrorCode,
  GraphvizWebError,
  type GraphvizWorkerRequest,
  type GraphvizWorkerResponse,
  type LoadVizWasmModule,
  type VizBatchRenderResult,
  type VizSingleRenderResult,
  type VizWasmInstance,
  assertActive,
  assertEngineSupported,
  assertFormatSupported,
  assertFormatsSupported,
  issuesToMessage,
  loadDefaultVizWasmModule,
  normalizeRenderOptions,
  snapshotCapabilities,
} from './shared.js';

export * from './shared.js';
export { Graphviz, createGraphviz, type GraphvizLoadOptions } from './graphviz.js';

export interface GraphvizRenderer {
  preload(): Promise<GraphvizCapabilities>;
  getCapabilities(): Promise<GraphvizCapabilities>;
  render(dot: string, options?: GraphvizRenderOptions): Promise<string>;
  renderDetailed(
    dot: string,
    options?: GraphvizRenderOptions
  ): Promise<GraphvizRenderDiagnostics>;
  renderMany(
    dot: string,
    formats: string[],
    options?: Omit<GraphvizRenderOptions, 'format'>
  ): Promise<Record<string, string>>;
  renderManyDetailed(
    dot: string,
    formats: string[],
    options?: Omit<GraphvizRenderOptions, 'format'>
  ): Promise<GraphvizBatchDiagnostics>;
  dispose(): Promise<void>;
}

export interface LazyWasmRendererOptions {
  loadModule?: LoadVizWasmModule;
  warmup?: boolean;
}

export interface ServerWasmRendererOptions {
  loadModule?: LoadVizWasmModule;
  eager?: boolean;
}

export interface WorkerWasmRendererOptions {
  worker?: Worker;
  workerFactory?: () => Worker;
  timeoutMs?: number;
  terminateOnDispose?: boolean;
}

function ensureSingleRenderSuccess(result: VizSingleRenderResult): string {
  if (result.status === 'success' && typeof result.output === 'string') {
    return result.output;
  }

  throw new GraphvizWebError(
    'RENDER_FAILED',
    issuesToMessage(result.errors, 'Graphviz Wasm render failed.'),
    { issues: result.errors }
  );
}

function ensureBatchRenderSuccess(result: VizBatchRenderResult): Record<string, string> {
  if (result.status === 'success' && result.output) {
    return result.output;
  }

  throw new GraphvizWebError(
    'RENDER_FAILED',
    issuesToMessage(result.errors, 'Graphviz Wasm multi-format render failed.'),
    { issues: result.errors }
  );
}

function createRendererFromVizFactory(
  getViz: () => Promise<VizWasmInstance>
): GraphvizRenderer {
  let disposed = false;

  const ensureViz = async (): Promise<VizWasmInstance> => {
    assertActive(disposed, 'Graphviz renderer');
    return getViz();
  };

  return {
    async preload() {
      const viz = await ensureViz();
      return snapshotCapabilities(viz);
    },

    async getCapabilities() {
      const viz = await ensureViz();
      return snapshotCapabilities(viz);
    },

    async render(dot, options) {
      const result = await this.renderDetailed(dot, options);
      return result.output;
    },

    async renderDetailed(dot, options) {
      const viz = await ensureViz();
      const normalized = normalizeRenderOptions(options);

      assertEngineSupported(viz, normalized.engine);
      assertFormatSupported(viz, normalized.format);

      const result = viz.render(dot, normalized);
      const output = ensureSingleRenderSuccess(result);

      return {
        output,
        issues: result.errors,
        capabilities: snapshotCapabilities(viz),
      };
    },

    async renderMany(dot, formats, options) {
      const result = await this.renderManyDetailed(dot, formats, options);
      return result.output;
    },

    async renderManyDetailed(dot, formats, options) {
      const viz = await ensureViz();
      const normalized = {
        ...(options ?? {}),
        engine: options?.engine ?? DEFAULT_ENGINE,
      };

      assertEngineSupported(viz, normalized.engine);
      assertFormatsSupported(viz, formats);

      const result = viz.renderFormats(dot, formats, normalized);
      const output = ensureBatchRenderSuccess(result);

      return {
        output,
        issues: result.errors,
        capabilities: snapshotCapabilities(viz),
      };
    },

    // Satisfies the `dispose(): Promise<void>` contract; the body is currently
    // synchronous but the async signature must be kept for callers that await.
    // eslint-disable-next-line @typescript-eslint/require-await
    async dispose() {
      disposed = true;
    },
  };
}

export function createLazyWasmRenderer(
  options: LazyWasmRendererOptions = {}
): GraphvizRenderer {
  const loadModule = options.loadModule ?? loadDefaultVizWasmModule;
  let vizPromise: Promise<VizWasmInstance> | undefined;

  const renderer = createRendererFromVizFactory(() => {
    if (!vizPromise) {
      vizPromise = loadModule().then((module) => module.instance());
    }

    return vizPromise;
  });

  if (options.warmup) {
    void renderer.preload().catch(() => undefined);
  }

  return renderer;
}

export function createServerWasmRenderer(
  options: ServerWasmRendererOptions = {}
): GraphvizRenderer {
  return createLazyWasmRenderer({
    loadModule: options.loadModule,
    warmup: options.eager ?? true,
  });
}

export function createDefaultGraphvizWorker(): Worker {
  if (typeof Worker === 'undefined') {
    throw new GraphvizWebError(
      'WORKER_UNAVAILABLE',
      'Worker is not available in the current runtime.'
    );
  }

  return new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });
}

function createWorkerError(
  code: GraphvizWebErrorCode,
  message: string,
  issues?: GraphvizRenderDiagnostics['issues']
): GraphvizWebError {
  return new GraphvizWebError(code, message, { issues });
}

export function createWorkerWasmRenderer(
  options: WorkerWasmRendererOptions = {}
): GraphvizRenderer {
  const worker = options.worker ?? options.workerFactory?.() ?? createDefaultGraphvizWorker();
  const ownsWorker = options.worker == null;
  const terminateOnDispose = options.terminateOnDispose ?? ownsWorker;
  const timeoutMs = options.timeoutMs;
  let disposed = false;
  let nextId = 1;

  const pending = new Map<
    number,
    {
      resolve: (value: unknown) => void;
      reject: (error: unknown) => void;
      timeoutId?: ReturnType<typeof setTimeout>;
    }
  >();

  const rejectAll = (error: unknown): void => {
    for (const { reject, timeoutId } of pending.values()) {
      if (timeoutId) {
        clearTimeout(timeoutId);
      }
      reject(error);
    }
    pending.clear();
  };

  const onMessage = (event: MessageEvent<GraphvizWorkerResponse>): void => {
    const response = event.data;
    const entry = pending.get(response.id);

    if (!entry) {
      return;
    }

    pending.delete(response.id);
    if (entry.timeoutId) {
      clearTimeout(entry.timeoutId);
    }

    if (response.ok) {
      entry.resolve(response.value);
      return;
    }

    entry.reject(
      createWorkerError(
        response.error.code,
        response.error.message,
        response.error.issues
      )
    );
  };

  const onError = (event: ErrorEvent): void => {
    rejectAll(
      new GraphvizWebError(
        'WORKER_UNAVAILABLE',
        `Graphviz worker failed: ${event.message || 'unknown worker error'}`,
        { cause: event.error }
      )
    );
  };

  worker.addEventListener('message', onMessage as EventListener);
  worker.addEventListener('error', onError as EventListener);

  const callWorker = <T>(
    action: GraphvizWorkerRequest['action'],
    payload: Omit<GraphvizWorkerRequest, 'id' | 'action'> = {}
  ): Promise<T> => {
    if (disposed) {
      return Promise.reject(
        new GraphvizWebError(
          'DISPOSED',
          'Graphviz worker renderer has already been disposed.'
        )
      );
    }

    const requestId = nextId++;
    const request: GraphvizWorkerRequest = {
      id: requestId,
      action,
      ...payload,
    };

    return new Promise<T>((resolve, reject) => {
      const timeoutId =
        typeof timeoutMs === 'number' && timeoutMs > 0
          ? setTimeout(() => {
              pending.delete(requestId);
              reject(
                new GraphvizWebError(
                  'TIMEOUT',
                  `Graphviz worker request timed out after ${timeoutMs}ms.`
                )
              );
            }, timeoutMs)
          : undefined;

      pending.set(requestId, {
        resolve: resolve as (value: unknown) => void,
        reject,
        timeoutId,
      });
      worker.postMessage(request);
    });
  };

  return {
    preload() {
      return callWorker<GraphvizCapabilities>('preload');
    },

    getCapabilities() {
      return callWorker<GraphvizCapabilities>('capabilities');
    },

    async render(dot, options) {
      const result = await this.renderDetailed(dot, options);
      return result.output;
    },

    renderDetailed(dot, options) {
      return callWorker<GraphvizRenderDiagnostics>('renderDetailed', { dot, options });
    },

    async renderMany(dot, formats, options) {
      const result = await this.renderManyDetailed(dot, formats, options);
      return result.output;
    },

    renderManyDetailed(dot, formats, options) {
      if (formats.length === 0) {
        return Promise.reject(
          new GraphvizWebError(
            'UNSUPPORTED_FORMAT',
            'At least one output format must be requested.'
          )
        );
      }

      return callWorker<GraphvizBatchDiagnostics>('renderManyDetailed', {
        dot,
        formats,
        options,
      });
    },

    // Satisfies the `dispose(): Promise<void>` contract; the body is currently
    // synchronous but the async signature must be kept for callers that await.
    // eslint-disable-next-line @typescript-eslint/require-await
    async dispose() {
      if (disposed) {
        return;
      }

      disposed = true;
      worker.removeEventListener('message', onMessage as EventListener);
      worker.removeEventListener('error', onError as EventListener);

      rejectAll(new GraphvizWebError('DISPOSED', 'Graphviz worker renderer disposed.'));

      if (terminateOnDispose) {
        worker.terminate();
      }
    },
  };
}
