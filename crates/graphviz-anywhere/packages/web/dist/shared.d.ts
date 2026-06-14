export declare const DEFAULT_ENGINE = "dot";
export declare const DEFAULT_FORMAT = "svg";
export declare const KNOWN_ENGINES: readonly ["dot", "neato", "fdp", "sfdp", "circo", "twopi", "osage", "patchwork"];
export declare const KNOWN_TEXT_FORMATS: readonly ["svg", "json", "dot", "xdot", "plain"];
type CustomStringLiteral = string & Record<never, never>;
export type GraphvizEngine = (typeof KNOWN_ENGINES)[number] | CustomStringLiteral;
export type GraphvizFormat = (typeof KNOWN_TEXT_FORMATS)[number] | CustomStringLiteral;
export type GraphvizErrorLevel = 'error' | 'warning';
export type GraphvizAttributeValue = string | number | boolean | {
    html: string;
};
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
export type GraphvizWebErrorCode = 'UNSUPPORTED_ENGINE' | 'UNSUPPORTED_FORMAT' | 'RENDER_FAILED' | 'WORKER_UNAVAILABLE' | 'TIMEOUT' | 'DISPOSED';
export declare class GraphvizWebError extends Error {
    readonly code: GraphvizWebErrorCode;
    readonly issues: GraphvizRenderIssue[];
    constructor(code: GraphvizWebErrorCode, message: string, options?: {
        issues?: GraphvizRenderIssue[];
        cause?: unknown;
    });
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
    renderFormats(input: string, formats: string[], options?: GraphvizRenderOptions): VizBatchRenderResult;
}
export interface VizWasmModule {
    instance(): Promise<VizWasmInstance>;
}
export type LoadVizWasmModule = () => Promise<VizWasmModule>;
export interface GraphvizWorkerRequest {
    id: number;
    action: 'preload' | 'capabilities' | 'render' | 'renderDetailed' | 'renderMany' | 'renderManyDetailed' | 'dispose';
    dot?: string;
    formats?: string[];
    options?: GraphvizRenderOptions;
}
export interface GraphvizWorkerErrorPayload {
    code: GraphvizWebErrorCode;
    message: string;
    issues?: GraphvizRenderIssue[];
}
export type GraphvizWorkerResponse = {
    id: number;
    ok: true;
    value: unknown;
} | {
    id: number;
    ok: false;
    error: GraphvizWorkerErrorPayload;
};
export declare function loadDefaultVizWasmModule(): Promise<VizWasmModule>;
export declare function normalizeRenderOptions(options?: GraphvizRenderOptions): GraphvizRenderOptions;
export declare function snapshotCapabilities(viz: VizWasmInstance): GraphvizCapabilities;
export declare function issuesToMessage(issues: GraphvizRenderIssue[], fallback: string): string;
export declare function assertEngineSupported(viz: VizWasmInstance, engine: string | undefined): void;
export declare function assertFormatSupported(viz: VizWasmInstance, format: string | undefined): void;
export declare function assertFormatsSupported(viz: VizWasmInstance, formats: string[]): void;
export declare function assertActive(disposed: boolean, surface: string): void;
export {};
