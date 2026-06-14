// 根入口只导出"包级公共 API"，具体 engine 走 subpath（./mermaid, ./echarts 等）。
// 避免 `export *` 在多个 engine 都有同名 `Options` 时冲突。

// ── 类型 ────────────────────────────────────────────────
export type {
  // Engine v2 types
  RenderOptions,
  RenderFn,
  EngineFactory,
  ErrorCode,
  // Diagram service facade
  DiagramEngineType,
  DiagramRenderFormat,
  DiagramErrorInfo,
  DiagramRenderResult,
  DiagramRenderService,
  GraphvizAttributeValue,
  GraphvizImageSize,
  GraphvizDiagramOptions,
  GraphvizCapabilities,
  GraphvizRenderAdapter,
  DiagramEngineOptions,
} from './types.js';

export { DiagramRenderError } from './types.js';

// ── Diagram runtime facade ───────────────────────────────
export { createDiagramEngine } from './engine.js';
export {
  GRAPHVIZ_LAYOUT_ENGINES,
  renderGraphvizSvg,
  isGraphvizDiagramEngine,
  pickGraphvizDiagramOptions,
  resolveGraphvizLayoutEngine,
} from './graphviz/index.js';
export { renderMermaidSvg } from './mermaid/index.js';
export { renderMathJaxSvg, getSvgViewBoxSize } from './mathjax/index.js';
export {
  createCodeHighlighter,
  withCodeHighlightCache,
  buildCodeHighlightCacheKey,
  type CodeHighlightService,
  type CodeHighlightCacheOptions,
} from './code-highlight.js';
