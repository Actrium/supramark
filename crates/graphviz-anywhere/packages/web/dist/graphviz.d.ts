import { type GraphvizRenderOptions } from './shared.js';
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
export declare class Graphviz {
    #private;
    private constructor();
    /** Load the Graphviz wasm module and return an instance. */
    static load(options?: GraphvizLoadOptions): Promise<Graphviz>;
    /** The underlying Graphviz library version (e.g. `"14.1.5"`). */
    version(): string;
    /**
     * Low-level layout entry point. Mirrors the Embind `CGraphviz.layout` method.
     * Throws `GraphvizWebError` on empty output / caught exceptions.
     */
    layout(dot: string, format?: string, engine?: string): string;
    /**
     * Convenience wrapper that defaults `format` to `'svg'`, matching
     * `@hpcc-js/wasm`'s `graphviz.dot()`.
     */
    dot(dotSource: string, format?: string): string;
}
/**
 * Shortcut for `Graphviz.load()`, provided for symmetry with the
 * `createLazyWasmRenderer` factory style.
 */
export declare function createGraphviz(options?: GraphvizLoadOptions): Promise<Graphviz>;
export type { GraphvizRenderOptions };
