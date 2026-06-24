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
] as const;

export const KNOWN_TEXT_FORMATS = [
  'svg',
  'json',
  'dot',
  'xdot',
  'plain',
] as const;

type CustomStringLiteral = string & Record<never, never>;

export type GraphvizEngine = (typeof KNOWN_ENGINES)[number] | CustomStringLiteral;
export type GraphvizFormat = (typeof KNOWN_TEXT_FORMATS)[number] | CustomStringLiteral;
export type GraphvizErrorLevel = 'error' | 'warning';
export type GraphvizAttributeValue = string | number | boolean | { html: string };

export interface GraphvizImageSize {
  name: string;
  width: string | number;
  height: string | number;
}

export interface GraphvizRenderOptions {
  engine?: GraphvizEngine;
  format?: GraphvizFormat;
  yInvert?: boolean;
  reduce?: boolean;
  graphAttributes?: Record<string, GraphvizAttributeValue>;
  nodeAttributes?: Record<string, GraphvizAttributeValue>;
  edgeAttributes?: Record<string, GraphvizAttributeValue>;
  images?: GraphvizImageSize[];
}

export interface GraphvizRenderIssue {
  level?: GraphvizErrorLevel;
  message: string;
}

export interface GraphvizCapabilities {
  graphvizVersion: string;
  engines: string[];
  formats: string[];
}

export interface GraphvizRenderDiagnostics {
  output: string;
  issues: GraphvizRenderIssue[];
  capabilities: GraphvizCapabilities;
}

export interface GraphvizBatchDiagnostics {
  output: Record<string, string>;
  issues: GraphvizRenderIssue[];
  capabilities: GraphvizCapabilities;
}

export type GraphvizWebErrorCode =
  | 'UNSUPPORTED_ENGINE'
  | 'UNSUPPORTED_FORMAT'
  | 'RENDER_FAILED'
  | 'WORKER_UNAVAILABLE'
  | 'TIMEOUT'
  | 'DISPOSED';

export class GraphvizWebError extends Error {
  readonly code: GraphvizWebErrorCode;
  readonly issues: GraphvizRenderIssue[];

  constructor(
    code: GraphvizWebErrorCode,
    message: string,
    options: { issues?: GraphvizRenderIssue[]; cause?: unknown } = {}
  ) {
    super(message);
    this.name = 'GraphvizWebError';
    this.code = code;
    this.issues = options.issues ?? [];

    if (options.cause !== undefined) {
      (this as Error & { cause?: unknown }).cause = options.cause;
    }
  }
}

export interface VizSingleRenderResult {
  status: 'success' | 'failure';
  output?: string;
  errors: GraphvizRenderIssue[];
}

export interface VizBatchRenderResult {
  status: 'success' | 'failure';
  output?: Record<string, string>;
  errors: GraphvizRenderIssue[];
}

export interface VizWasmInstance {
  readonly graphvizVersion: string;
  readonly engines: string[];
  readonly formats: string[];
  render(input: string, options?: GraphvizRenderOptions): VizSingleRenderResult;
  renderFormats(
    input: string,
    formats: string[],
    options?: GraphvizRenderOptions
  ): VizBatchRenderResult;
}

export interface VizWasmModule {
  instance(): Promise<VizWasmInstance>;
}

export type LoadVizWasmModule = () => Promise<VizWasmModule>;

export interface GraphvizWorkerRequest {
  id: number;
  action:
    | 'preload'
    | 'capabilities'
    | 'render'
    | 'renderDetailed'
    | 'renderMany'
    | 'renderManyDetailed'
    | 'dispose';
  dot?: string;
  formats?: string[];
  options?: GraphvizRenderOptions;
}

export interface GraphvizWorkerErrorPayload {
  code: GraphvizWebErrorCode;
  message: string;
  issues?: GraphvizRenderIssue[];
}

export type GraphvizWorkerResponse =
  | {
      id: number;
      ok: true;
      value: unknown;
    }
  | {
      id: number;
      ok: false;
      error: GraphvizWorkerErrorPayload;
    };

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

interface EmbindCGraphviz {
  layout(dot: string, format: string, engine: string): string;
  delete(): void;
}

interface EmbindCGraphvizCtor {
  new (): EmbindCGraphviz;
  version(): string;
  lastError(): string;
}

interface EmbindModule {
  CGraphviz: EmbindCGraphvizCtor;
}

type VizModuleFactory = (config?: {
  wasmBinary?: ArrayBuffer | Uint8Array;
  locateFile?: (path: string, prefix: string) => string;
}) => Promise<EmbindModule>;

export async function loadDefaultVizWasmModule(): Promise<VizWasmModule> {
  let vizModuleFactory: { default: VizModuleFactory };
  try {
    // Published-package case: resolved via the `./wasm` export.
    vizModuleFactory = (await import(
      '@actrium/graphviz-anywhere-web/wasm'
    )) as unknown as { default: VizModuleFactory };
  } catch {
    try {
      // Dev / workspace case: import straight from the build output.
      vizModuleFactory = (await import('../dist/viz.js')) as unknown as {
        default: VizModuleFactory;
      };
    } catch {
      throw new Error(
        'Wasm module not found. Build it first: npm run build (or scripts/build-wasm.sh).'
      );
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
function wrapEmbindInstance(module: EmbindModule): VizWasmInstance {
  const version = module.CGraphviz.version();

  const renderOne = (
    dot: string,
    options?: GraphvizRenderOptions
  ): VizSingleRenderResult => {
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
              message:
                lastErr.trim() ||
                'Graphviz produced empty output (check DOT syntax).',
            },
          ],
        };
      }

      const errors: GraphvizRenderIssue[] = lastErr.trim()
        ? [{ level: 'warning', message: lastErr.trim() }]
        : [];

      return { status: 'success', output, errors };
    } catch (error) {
      return {
        status: 'failure',
        errors: [
          {
            level: 'error',
            message: error instanceof Error ? error.message : String(error),
          },
        ],
      };
    } finally {
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
      const output: Record<string, string> = {};
      const errors: GraphvizRenderIssue[] = [];

      for (const format of formats) {
        const result = renderOne(input, { ...options, format });
        if (result.status === 'success' && result.output !== undefined) {
          output[format] = result.output;
          errors.push(...result.errors);
        } else {
          return { status: 'failure', errors: result.errors };
        }
      }

      return { status: 'success', output, errors };
    },
  };
}

export function normalizeRenderOptions(
  options: GraphvizRenderOptions = {}
): GraphvizRenderOptions {
  return {
    ...options,
    engine: options.engine ?? DEFAULT_ENGINE,
    format: options.format ?? DEFAULT_FORMAT,
  };
}

export function snapshotCapabilities(viz: VizWasmInstance): GraphvizCapabilities {
  return {
    graphvizVersion: viz.graphvizVersion,
    engines: [...viz.engines],
    formats: [...viz.formats],
  };
}

export function issuesToMessage(
  issues: GraphvizRenderIssue[],
  fallback: string
): string {
  const messages = issues
    .map((issue) => issue.message.trim())
    .filter((message) => message.length > 0);

  return messages.length > 0 ? messages.join('; ') : fallback;
}

export function assertEngineSupported(
  viz: VizWasmInstance,
  engine: string | undefined
): void {
  if (engine && !viz.engines.includes(engine)) {
    throw new GraphvizWebError(
      'UNSUPPORTED_ENGINE',
      `Engine "${engine}" is not available in this Wasm build. Supported engines: ${viz.engines.join(', ')}.`
    );
  }
}

export function assertFormatSupported(
  viz: VizWasmInstance,
  format: string | undefined
): void {
  if (format && !viz.formats.includes(format)) {
    throw new GraphvizWebError(
      'UNSUPPORTED_FORMAT',
      `Format "${format}" is not available in this Wasm build. Supported formats: ${viz.formats.join(', ')}.`
    );
  }
}

export function assertFormatsSupported(
  viz: VizWasmInstance,
  formats: string[]
): void {
  const unsupported = formats.filter((format) => !viz.formats.includes(format));

  if (unsupported.length > 0) {
    throw new GraphvizWebError(
      'UNSUPPORTED_FORMAT',
      `Formats "${unsupported.join(', ')}" are not available in this Wasm build. Supported formats: ${viz.formats.join(', ')}.`
    );
  }
}

export function assertActive(disposed: boolean, surface: string): void {
  if (disposed) {
    throw new GraphvizWebError('DISPOSED', `${surface} has already been disposed.`);
  }
}
