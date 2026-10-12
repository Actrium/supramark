/**
 * Error codes returned by the native Graphviz module.
 */
export declare const GraphvizErrorCode: {
    readonly NULL_INPUT: "NULL_INPUT";
    readonly INVALID_DOT: "INVALID_DOT";
    readonly LAYOUT_FAILED: "LAYOUT_FAILED";
    readonly RENDER_FAILED: "RENDER_FAILED";
    readonly INVALID_ENGINE: "INVALID_ENGINE";
    readonly INVALID_FORMAT: "INVALID_FORMAT";
    readonly OUT_OF_MEMORY: "OUT_OF_MEMORY";
    readonly NOT_INITIALIZED: "NOT_INITIALIZED";
    readonly UNKNOWN: "UNKNOWN";
};
export type GraphvizErrorCodeType = (typeof GraphvizErrorCode)[keyof typeof GraphvizErrorCode];
/**
 * Layout engines supported by Graphviz.
 */
export type GraphvizEngine = 'dot' | 'neato' | 'fdp' | 'sfdp' | 'circo' | 'twopi' | 'osage' | 'patchwork';
/**
 * Output formats supported by Graphviz.
 */
export type GraphvizFormat = 'svg' | 'png' | 'pdf' | 'ps' | 'json' | 'dot' | 'xdot' | 'plain';
interface NativeGraphvizModule {
    renderDot(dot: string, engine: string, format: string): Promise<string>;
    getVersion(): Promise<string>;
}
/**
 * Resolve the native module, preferring TurboModules (new arch) with fallback
 * to the bridge-based NativeModules (old arch). When neither is linked, return
 * a Proxy that throws an actionable error on first use rather than at import
 * time. Kept a pure function of its inputs so the fallback order is testable.
 */
export declare function resolveNative(turbo: NativeGraphvizModule | null | undefined, bridged: NativeGraphvizModule | null | undefined): NativeGraphvizModule;
/**
 * Render a DOT language string into the specified output format.
 *
 * All rendering is performed on a background thread and the result
 * is delivered asynchronously via a Promise.
 *
 * @param dot - DOT language string describing the graph
 * @param engine - Layout engine to use (default: "dot")
 * @param format - Output format (default: "svg")
 * @returns Promise resolving to the rendered output string.
 *          For text formats (svg, json, dot, xdot, plain) the raw text is returned.
 *          For binary formats (png, pdf, ps) the output is base64-encoded.
 */
export declare function renderDot(dot: string, engine?: GraphvizEngine, format?: GraphvizFormat): Promise<string>;
/**
 * Get the Graphviz library version string.
 *
 * @returns Promise resolving to the version string (e.g. "12.2.1")
 */
export declare function getVersion(): Promise<string>;
declare const _default: {
    renderDot: typeof renderDot;
    getVersion: typeof getVersion;
    GraphvizErrorCode: {
        readonly NULL_INPUT: "NULL_INPUT";
        readonly INVALID_DOT: "INVALID_DOT";
        readonly LAYOUT_FAILED: "LAYOUT_FAILED";
        readonly RENDER_FAILED: "RENDER_FAILED";
        readonly INVALID_ENGINE: "INVALID_ENGINE";
        readonly INVALID_FORMAT: "INVALID_FORMAT";
        readonly OUT_OF_MEMORY: "OUT_OF_MEMORY";
        readonly NOT_INITIALIZED: "NOT_INITIALIZED";
        readonly UNKNOWN: "UNKNOWN";
    };
};
export default _default;
//# sourceMappingURL=index.d.ts.map