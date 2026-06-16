/**
 * @supramark/web/client
 *
 * 客户端（浏览器）渲染专用导出。
 *
 * 此模块提供 React 组件，用于在浏览器中动态渲染 Markdown。
 * 适用于 CSR（客户端渲染）场景和 SPA（单页应用）。
 *
 * @example
 * ```typescript
 * import { Supramark, parse } from '@supramark/web/client';
 *
 * function App() {
 *   const [markdown, setMarkdown] = useState('# Hello World');
 *
 *   return (
 *     <div>
 *       <Supramark markdown={markdown} />
 *     </div>
 *   );
 * }
 * ```
 *
 * @example
 * ```typescript
 * // 预解析 AST 然后传入（性能优化）
 * import { Supramark, parse } from '@supramark/web/client';
 *
 * const ast = await parse('# Hello World');
 *
 * function App() {
 *   return <Supramark ast={ast} markdown="" />;
 * }
 * ```
 */

// React 组件
export { Supramark } from './Supramark.js';
export type { SupramarkRenderState, SupramarkWebProps } from './Supramark.js';
export { DiagramEngineProvider } from './DiagramEngineProvider.js';
export type { DiagramEngineProviderProps } from './DiagramEngineProvider.js';

// ClassName 系统
export type { SupramarkClassNames } from './classNames.js';
export {
  defaultClassNames,
  mergeClassNames,
  tailwindClassNames,
  minimalClassNames,
} from './classNames.js';

// 核心解析功能（可选，浏览器中也可以解析 Markdown）
export { parse } from '@supramark/core';
export type { SupramarkRootNode, SupramarkNode } from '@supramark/core';
