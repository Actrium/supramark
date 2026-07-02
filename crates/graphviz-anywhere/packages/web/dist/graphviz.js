// High-level Graphviz wrapper matching the hpcc-js/wasm convention.
//
// This is the canonical API for 0.1.3 and later. It consumes the Embind
// `CGraphviz` class exposed by `dist/viz.js` directly, without the legacy
// `VizWasmInstance` abstraction. The older `createLazyWasmRenderer` /
// `createServerWasmRenderer` / `createWorkerWasmRenderer` factories are
// retained in `./index.ts` as a compatibility shim and ultimately resolve
// to the same underlying Embind entry points.
var __classPrivateFieldSet = (this && this.__classPrivateFieldSet) || function (receiver, state, value, kind, f) {
    if (kind === "m") throw new TypeError("Private method is not writable");
    if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a setter");
    if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver)) throw new TypeError("Cannot write private member to an object whose class did not declare it");
    return (kind === "a" ? f.call(receiver, value) : f ? f.value = value : state.set(receiver, value)), value;
};
var __classPrivateFieldGet = (this && this.__classPrivateFieldGet) || function (receiver, state, kind, f) {
    if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a getter");
    if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver)) throw new TypeError("Cannot read private member from an object whose class did not declare it");
    return kind === "m" ? f : kind === "a" ? f.call(receiver) : f ? f.value : state.get(receiver);
};
var _Graphviz_module;
import { DEFAULT_ENGINE, DEFAULT_FORMAT, GraphvizWebError, } from './shared.js';
async function loadDefaultFactory() {
    try {
        const mod = (await import('@actrium/graphviz-anywhere-web/wasm'));
        return mod.default;
    }
    catch {
        try {
            // Dev / workspace case: load straight from the build output sibling.
            const mod = (await import('../dist/viz.js'));
            return mod.default;
        }
        catch (cause) {
            throw new GraphvizWebError('RENDER_FAILED', 'Wasm module not found. Build it first: scripts/build-wasm.sh.', { cause });
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
    constructor(module) {
        _Graphviz_module.set(this, void 0);
        __classPrivateFieldSet(this, _Graphviz_module, module, "f");
    }
    /** Load the Graphviz wasm module and return an instance. */
    static async load(options = {}) {
        const factory = options.factory ?? (await loadDefaultFactory());
        const config = {};
        if (options.wasmBinary !== undefined)
            config.wasmBinary = options.wasmBinary;
        if (options.locateFile !== undefined)
            config.locateFile = options.locateFile;
        const module = await factory(config);
        return new Graphviz(module);
    }
    /** The underlying Graphviz library version (e.g. `"14.1.5"`). */
    version() {
        return __classPrivateFieldGet(this, _Graphviz_module, "f").CGraphviz.version();
    }
    /**
     * Low-level layout entry point. Mirrors the Embind `CGraphviz.layout` method.
     * Throws `GraphvizWebError` on empty output / caught exceptions.
     */
    layout(dot, format = DEFAULT_FORMAT, engine = DEFAULT_ENGINE) {
        const instance = new (__classPrivateFieldGet(this, _Graphviz_module, "f").CGraphviz)();
        try {
            let output;
            try {
                output = instance.layout(dot, format, engine);
            }
            catch (error) {
                throw new GraphvizWebError('RENDER_FAILED', error instanceof Error ? error.message : String(error), { cause: error });
            }
            const lastErr = __classPrivateFieldGet(this, _Graphviz_module, "f").CGraphviz.lastError().trim();
            if (!output) {
                throw new GraphvizWebError('RENDER_FAILED', lastErr || 'Graphviz produced empty output (check DOT syntax).');
            }
            return output;
        }
        finally {
            instance.delete();
        }
    }
    /**
     * Convenience wrapper that defaults `format` to `'svg'`, matching
     * `@hpcc-js/wasm`'s `graphviz.dot()`.
     */
    dot(dotSource, format = DEFAULT_FORMAT) {
        return this.layout(dotSource, format, DEFAULT_ENGINE);
    }
}
_Graphviz_module = new WeakMap();
/**
 * Shortcut for `Graphviz.load()`, provided for symmetry with the
 * `createLazyWasmRenderer` factory style.
 */
export function createGraphviz(options) {
    return Graphviz.load(options);
}
