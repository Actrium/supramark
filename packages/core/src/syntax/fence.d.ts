import type Token from 'markdown-it/lib/token.mjs';
import type { SupramarkParentNode } from '../ast.js';
/**
 * 判断 fenced code 的语言是否属于内置 diagram 引擎。
 *
 * - 目前用于 ```mermaid / ```plantuml / ```vega-lite 等；
 * - 未来如果扩展更多 fence 语法，可以在此集中管理。
 */
export declare function isDiagramFenceLanguage(lang?: string | null): boolean;
/**
 * 将 markdown-it 的 fence / code_block token 映射为 supramark 块级节点。
 *
 * 当前策略：
 * - 如果语言是内置 diagram 引擎 → 生成 `diagram` 节点；
 * - 否则 → 生成普通 `code` 节点。
 *
 * Note: diagram 是否真正渲染由运行时（RN/Web）按 Feature 配置决定。
 */
export declare function mapFenceTokenToBlockNode(token: Token, parent: SupramarkParentNode): void;
