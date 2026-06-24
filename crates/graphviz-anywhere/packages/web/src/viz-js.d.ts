// Ambient types for the Embind-backed Graphviz Wasm module.
// The canonical declaration is generated alongside the build artifact at
// packages/web/dist/viz.d.ts by scripts/build-wasm.sh.

declare module '@actrium/graphviz-anywhere-web/wasm' {
  export interface CGraphviz {
    layout(dot: string, format: string, engine: string): string;
    delete(): void;
  }

  export interface CGraphvizConstructor {
    new (): CGraphviz;
    new (yInvert: number, nop: number): CGraphviz;
    version(): string;
    lastError(): string;
  }

  export interface VizModuleInstance {
    CGraphviz: CGraphvizConstructor;
  }

  export type VizModuleFactory = (config?: {
    wasmBinary?: ArrayBuffer | Uint8Array;
    locateFile?: (path: string, prefix: string) => string;
  }) => Promise<VizModuleInstance>;

  const VizModule: VizModuleFactory;
  export default VizModule;
}
