import { DEFAULT_ENGINE, GraphvizWebError, assertActive, assertEngineSupported, assertFormatSupported, assertFormatsSupported, issuesToMessage, loadDefaultVizWasmModule, normalizeRenderOptions, snapshotCapabilities, } from './shared.js';
export * from './shared.js';
export { Graphviz, createGraphviz } from './graphviz.js';
function ensureSingleRenderSuccess(result) {
    if (result.status === 'success' && typeof result.output === 'string') {
        return result.output;
    }
    throw new GraphvizWebError('RENDER_FAILED', issuesToMessage(result.errors, 'Graphviz Wasm render failed.'), { issues: result.errors });
}
function ensureBatchRenderSuccess(result) {
    if (result.status === 'success' && result.output) {
        return result.output;
    }
    throw new GraphvizWebError('RENDER_FAILED', issuesToMessage(result.errors, 'Graphviz Wasm multi-format render failed.'), { issues: result.errors });
}
function createRendererFromVizFactory(getViz) {
    let disposed = false;
    const ensureViz = async () => {
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
export function createLazyWasmRenderer(options = {}) {
    const loadModule = options.loadModule ?? loadDefaultVizWasmModule;
    let vizPromise;
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
export function createServerWasmRenderer(options = {}) {
    return createLazyWasmRenderer({
        loadModule: options.loadModule,
        warmup: options.eager ?? true,
    });
}
export function createDefaultGraphvizWorker() {
    if (typeof Worker === 'undefined') {
        throw new GraphvizWebError('WORKER_UNAVAILABLE', 'Worker is not available in the current runtime.');
    }
    return new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });
}
function createWorkerError(code, message, issues) {
    return new GraphvizWebError(code, message, { issues });
}
export function createWorkerWasmRenderer(options = {}) {
    const worker = options.worker ?? options.workerFactory?.() ?? createDefaultGraphvizWorker();
    const ownsWorker = options.worker == null;
    const terminateOnDispose = options.terminateOnDispose ?? ownsWorker;
    const timeoutMs = options.timeoutMs;
    let disposed = false;
    let nextId = 1;
    const pending = new Map();
    const rejectAll = (error) => {
        for (const { reject, timeoutId } of pending.values()) {
            if (timeoutId) {
                clearTimeout(timeoutId);
            }
            reject(error);
        }
        pending.clear();
    };
    const onMessage = (event) => {
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
        entry.reject(createWorkerError(response.error.code, response.error.message, response.error.issues));
    };
    const onError = (event) => {
        rejectAll(new GraphvizWebError('WORKER_UNAVAILABLE', `Graphviz worker failed: ${event.message || 'unknown worker error'}`, { cause: event.error }));
    };
    worker.addEventListener('message', onMessage);
    worker.addEventListener('error', onError);
    const callWorker = (action, payload = {}) => {
        if (disposed) {
            return Promise.reject(new GraphvizWebError('DISPOSED', 'Graphviz worker renderer has already been disposed.'));
        }
        const requestId = nextId++;
        const request = {
            id: requestId,
            action,
            ...payload,
        };
        return new Promise((resolve, reject) => {
            const timeoutId = typeof timeoutMs === 'number' && timeoutMs > 0
                ? setTimeout(() => {
                    pending.delete(requestId);
                    reject(new GraphvizWebError('TIMEOUT', `Graphviz worker request timed out after ${timeoutMs}ms.`));
                }, timeoutMs)
                : undefined;
            pending.set(requestId, {
                resolve: resolve,
                reject,
                timeoutId,
            });
            worker.postMessage(request);
        });
    };
    return {
        preload() {
            return callWorker('preload');
        },
        getCapabilities() {
            return callWorker('capabilities');
        },
        async render(dot, options) {
            const result = await this.renderDetailed(dot, options);
            return result.output;
        },
        renderDetailed(dot, options) {
            return callWorker('renderDetailed', { dot, options });
        },
        async renderMany(dot, formats, options) {
            const result = await this.renderManyDetailed(dot, formats, options);
            return result.output;
        },
        renderManyDetailed(dot, formats, options) {
            if (formats.length === 0) {
                return Promise.reject(new GraphvizWebError('UNSUPPORTED_FORMAT', 'At least one output format must be requested.'));
            }
            return callWorker('renderManyDetailed', {
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
            worker.removeEventListener('message', onMessage);
            worker.removeEventListener('error', onError);
            rejectAll(new GraphvizWebError('DISPOSED', 'Graphviz worker renderer disposed.'));
            if (terminateOnDispose) {
                worker.terminate();
            }
        },
    };
}
