export const DEFAULT_ENGINE = 'dot';
export const DEFAULT_FORMAT = 'svg';
export const KNOWN_ENGINES = [
    'dot',
    'neato',
    'fdp',
    'sfdp',
    'circo',
    'twopi',
    'osage',
    'patchwork',
];
export const KNOWN_TEXT_FORMATS = [
    'svg',
    'json',
    'dot',
    'xdot',
    'plain',
];
export class GraphvizWebError extends Error {
    constructor(code, message, options = {}) {
        super(message);
        this.name = 'GraphvizWebError';
        this.code = code;
        this.issues = options.issues ?? [];
        if (options.cause !== undefined) {
            this.cause = options.cause;
        }
    }
}
// ---------------------------------------------------------------------------
// Embind-backed Wasm loader
// ---------------------------------------------------------------------------
//
// The Wasm module produced by scripts/build-wasm.sh uses Emscripten Embind
// and exposes a typed `CGraphviz` class. We load the module via its
// MODULARIZE factory and wrap the class in the VizWasmInstance contract
// the rest of the package expects.
//
// Engines/formats are hard-coded here because the Embind wrapper is
// compiled with a known fixed plugin set (core + dot_layout + neato_layout).
// If we later expose a dynamic query, we can replace these constants with
// runtime lookups.
const STATIC_ENGINES = [
    'dot',
    'neato',
    'fdp',
    'sfdp',
    'circo',
    'twopi',
    'osage',
    'patchwork',
];
const STATIC_FORMATS = ['svg', 'json', 'dot', 'xdot', 'plain'];
export async function loadDefaultVizWasmModule() {
    let vizModuleFactory;
    try {
        // Published-package case: resolved via the `./wasm` export.
        vizModuleFactory = (await import('@actrium/graphviz-anywhere-web/wasm'));
    }
    catch {
        try {
            // Dev / workspace case: import straight from the build output.
            vizModuleFactory = (await import('../dist/viz.js'));
        }
        catch {
            throw new Error('Wasm module not found. Build it first: npm run build (or scripts/build-wasm.sh).');
        }
    }
    const factory = vizModuleFactory.default;
    return {
        instance: async () => {
            const module = await factory();
            return wrapEmbindInstance(module);
        },
    };
}
// Adapt the Embind module to VizWasmInstance.
function wrapEmbindInstance(module) {
    const version = module.CGraphviz.version();
    const renderOne = (dot, options) => {
        const engine = options?.engine ?? DEFAULT_ENGINE;
        const format = options?.format ?? DEFAULT_FORMAT;
        const instance = new module.CGraphviz();
        try {
            const output = instance.layout(dot, format, engine);
            const lastErr = module.CGraphviz.lastError();
            if (!output) {
                return {
                    status: 'failure',
                    errors: [
                        {
                            level: 'error',
                            message: lastErr.trim() ||
                                'Graphviz produced empty output (check DOT syntax).',
                        },
                    ],
                };
            }
            const errors = lastErr.trim()
                ? [{ level: 'warning', message: lastErr.trim() }]
                : [];
            return { status: 'success', output, errors };
        }
        catch (error) {
            return {
                status: 'failure',
                errors: [
                    {
                        level: 'error',
                        message: error instanceof Error ? error.message : String(error),
                    },
                ],
            };
        }
        finally {
            instance.delete();
        }
    };
    return {
        graphvizVersion: version,
        engines: [...STATIC_ENGINES],
        formats: [...STATIC_FORMATS],
        render(input, options) {
            return renderOne(input, options);
        },
        renderFormats(input, formats, options) {
            const output = {};
            const errors = [];
            for (const format of formats) {
                const result = renderOne(input, { ...options, format });
                if (result.status === 'success' && result.output !== undefined) {
                    output[format] = result.output;
                    errors.push(...result.errors);
                }
                else {
                    return { status: 'failure', errors: result.errors };
                }
            }
            return { status: 'success', output, errors };
        },
    };
}
export function normalizeRenderOptions(options = {}) {
    return {
        ...options,
        engine: options.engine ?? DEFAULT_ENGINE,
        format: options.format ?? DEFAULT_FORMAT,
    };
}
export function snapshotCapabilities(viz) {
    return {
        graphvizVersion: viz.graphvizVersion,
        engines: [...viz.engines],
        formats: [...viz.formats],
    };
}
export function issuesToMessage(issues, fallback) {
    const messages = issues
        .map((issue) => issue.message.trim())
        .filter((message) => message.length > 0);
    return messages.length > 0 ? messages.join('; ') : fallback;
}
export function assertEngineSupported(viz, engine) {
    if (engine && !viz.engines.includes(engine)) {
        throw new GraphvizWebError('UNSUPPORTED_ENGINE', `Engine "${engine}" is not available in this Wasm build. Supported engines: ${viz.engines.join(', ')}.`);
    }
}
export function assertFormatSupported(viz, format) {
    if (format && !viz.formats.includes(format)) {
        throw new GraphvizWebError('UNSUPPORTED_FORMAT', `Format "${format}" is not available in this Wasm build. Supported formats: ${viz.formats.join(', ')}.`);
    }
}
export function assertFormatsSupported(viz, formats) {
    const unsupported = formats.filter((format) => !viz.formats.includes(format));
    if (unsupported.length > 0) {
        throw new GraphvizWebError('UNSUPPORTED_FORMAT', `Formats "${unsupported.join(', ')}" are not available in this Wasm build. Supported formats: ${viz.formats.join(', ')}.`);
    }
}
export function assertActive(disposed, surface) {
    if (disposed) {
        throw new GraphvizWebError('DISPOSED', `${surface} has already been disposed.`);
    }
}
