// High-level Graphviz wrapper matching the hpcc-js/wasm convention.
//
// This is the canonical API for 0.1.3 and later. It consumes the Embind
// `CGraphviz` class exposed by `dist/viz.js` directly, without the legacy
// `VizWasmInstance` abstraction. The older `createLazyWasmRenderer` /
// `createServerWasmRenderer` / `createWorkerWasmRenderer` factories are
// retained in `./index.ts` as a compatibility shim and ultimately resolve
// to the same underlying Embind entry points.

import {
  DEFAULT_ENGINE,
  DEFAULT_FORMAT,
  GraphvizWebError,
  type GraphvizRenderOptions,
} from './shared.js';

// Minimal shape of the Embind module as produced by scripts/build-wasm.sh.
// Kept local so we don't rely on ambient types from the generated d.ts.
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

export interface GraphvizLoadOptions {
  /**
   * Custom Embind factory. If omitted, the default `./viz.js` sibling
   * produced by scripts/build-wasm.sh is used.
   */
  factory?: VizModuleFactory;
  /**
   * Passed through to the Embind factory; useful when you want to feed the
   * wasm bytes from an explicit fetch rather than let the loader locate the
   * file itself.
   */
  wasmBinary?: ArrayBuffer | Uint8Array;
  /**
   * Passed through to the Embind factory. Custom resolver for
   * `viz.wasm` siblings, matches Emscripten's MODULARIZE contract.
   */
  locateFile?: (path: string, prefix: string) => string;
}

async function loadDefaultFactory(): Promise<VizModuleFactory> {
  try {
    const mod = (await import('@actrium/graphviz-anywhere-web/wasm')) as unknown as {
      default: VizModuleFactory;
    };
    return mod.default;
  } catch {
    try {
      // Dev / workspace case: load straight from the build output sibling.
      const mod = (await import('../dist/viz.js')) as unknown as {
        default: VizModuleFactory;
      };
      return mod.default;
    } catch (cause) {
      throw new GraphvizWebError(
        'RENDER_FAILED',
        'Wasm module not found. Build it first: scripts/build-wasm.sh.',
        { cause }
      );
    }
  }
}

/**
 * Convenience wrapper over the Embind `CGraphviz` binding, modelled after
 * `@hpcc-js/wasm`'s `Graphviz.load()`. Prefer this over the legacy
 * `createLazyWasmRenderer` factories in new code.
 *
 * @example
 * ```ts
 * import { Graphviz } from '@actrium/graphviz-anywhere-web';
 *
 * const gv = await Graphviz.load();
 * const svg = gv.dot('digraph { a -> b; b -> c; }');
 * ```
 */
export class Graphviz {
  readonly #module: EmbindModule;

  private constructor(module: EmbindModule) {
    this.#module = module;
  }

  /** Load the Graphviz wasm module and return an instance. */
  static async load(options: GraphvizLoadOptions = {}): Promise<Graphviz> {
    const factory = options.factory ?? (await loadDefaultFactory());
    const config: Parameters<VizModuleFactory>[0] = {};
    if (options.wasmBinary !== undefined) config.wasmBinary = options.wasmBinary;
    if (options.locateFile !== undefined) config.locateFile = options.locateFile;

    const module = await factory(config);
    return new Graphviz(module);
  }

  /** The underlying Graphviz library version (e.g. `"14.1.5"`). */
  version(): string {
    return this.#module.CGraphviz.version();
  }

  /**
   * Low-level layout entry point. Mirrors the Embind `CGraphviz.layout` method.
   * Throws `GraphvizWebError` on empty output / caught exceptions.
   */
  layout(
    dot: string,
    format: string = DEFAULT_FORMAT,
    engine: string = DEFAULT_ENGINE
  ): string {
    const instance = new this.#module.CGraphviz();
    try {
      let output: string;
      try {
        output = instance.layout(dot, format, engine);
      } catch (error) {
        throw new GraphvizWebError(
          'RENDER_FAILED',
          error instanceof Error ? error.message : String(error),
          { cause: error }
        );
      }

      const lastErr = this.#module.CGraphviz.lastError().trim();

      if (!output) {
        throw new GraphvizWebError(
          'RENDER_FAILED',
          lastErr || 'Graphviz produced empty output (check DOT syntax).'
        );
      }

      return output;
    } finally {
      instance.delete();
    }
  }

  /**
   * Convenience wrapper that defaults `format` to `'svg'`, matching
   * `@hpcc-js/wasm`'s `graphviz.dot()`.
   */
  dot(dotSource: string, format: string = DEFAULT_FORMAT): string {
    return this.layout(dotSource, format, DEFAULT_ENGINE);
  }
}

/**
 * Shortcut for `Graphviz.load()`, provided for symmetry with the
 * `createLazyWasmRenderer` factory style.
 */
export function createGraphviz(options?: GraphvizLoadOptions): Promise<Graphviz> {
  return Graphviz.load(options);
}

// Re-export the render options type so consumers can type-annotate calls
// without an extra import from `./shared`.
export type { GraphvizRenderOptions };
