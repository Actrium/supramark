declare module '@actrium/mermaid-little-web' {
  /** wasm-bindgen default async initialiser. */
  const init: (input?: unknown) => Promise<unknown>;
  export default init;

  /** Convert Mermaid source to an SVG string. */
  export function convert(mmd: string): Promise<string> | string;

  /** Same as `convert`, but with an explicit diagram id (mirrors mermaid's render(id, src)). */
  export function convertWithId(mmd: string, id: string): Promise<string> | string;

  /** Compiled wasm version (CARGO_PKG_VERSION at build time). */
  export function version(): string;
}

declare module 'mathjax-full/js/mathjax.js' {
  /** Opaque DOM-like node handle produced and consumed by the lite adaptor. */
  type MathJaxNode = unknown;
  /** Minimal shape of the document returned by `mathjax.document`. */
  interface MathJaxDocument {
    convert(input: string, options?: Record<string, unknown>): MathJaxNode;
  }
  export const mathjax: {
    document(input?: string, options?: Record<string, unknown>): MathJaxDocument;
  };
}

declare module 'mathjax-full/js/input/tex.js' {
  export class TeX {
    constructor(options?: Record<string, unknown>);
  }
}

declare module 'mathjax-full/js/output/svg.js' {
  export class SVG {
    constructor(options?: Record<string, unknown>);
  }
}

declare module 'mathjax-full/js/adaptors/liteAdaptor.js' {
  /** Opaque DOM-like node handle used by the lite adaptor. */
  type LiteAdaptorNode = unknown;
  /** Minimal shape of the lite adaptor used by the math engine. */
  export interface LiteAdaptor {
    firstChild(node: LiteAdaptorNode): LiteAdaptorNode;
    outerHTML(node: LiteAdaptorNode): string;
  }
  export function liteAdaptor(): LiteAdaptor;
}

declare module 'mathjax-full/js/handlers/html.js' {
  import type { LiteAdaptor } from 'mathjax-full/js/adaptors/liteAdaptor.js';
  export function RegisterHTMLHandler(adaptor: LiteAdaptor): void;
}

declare module 'mathjax-full/js/input/tex/AllPackages.js' {
  export const AllPackages: unknown;
}

declare module '@actrium/plantuml-little-web' {
  /** wasm-bindgen default async initialiser. */
  const init: (input?: unknown) => Promise<unknown>;
  export default init;

  /** Convert PlantUML source to an SVG string. */
  export function convert(puml: string): Promise<string> | string;

  /** Alternative names the package may expose depending on build shape. */
  export function render(puml: string): Promise<string> | string;
  export function renderSvg(puml: string): Promise<string> | string;

  /** Register a Graphviz bridge (dot -> svg). */
  export function setGraphvizBridge(
    fn: (dot: string, engine?: string) => Promise<string> | string
  ): void;
  export function set_graphviz_bridge(
    fn: (dot: string, engine?: string) => Promise<string> | string
  ): void;
  export function setGraphvizRenderer(
    fn: (dot: string, engine?: string) => Promise<string> | string
  ): void;
  export function registerGraphviz(
    fn: (dot: string, engine?: string) => Promise<string> | string
  ): void;
}

declare module '@actrium/d2-little-web' {
  /** wasm-bindgen default async initialiser. */
  const init: (input?: unknown) => Promise<unknown>;
  export default init;

  /** Convert D2 source to an SVG string. */
  export function convert(d2: string): Promise<string> | string;

  /** Alternative names the package may expose depending on build shape. */
  export function render(d2: string): Promise<string> | string;
  export function renderSvg(d2: string): Promise<string> | string;
}
