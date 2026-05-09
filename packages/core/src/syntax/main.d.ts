import type MarkdownIt from 'markdown-it';
import type { SupramarkConfig } from '../feature.js';
/**
 * 为 main 语法家族（Core / GFM / Math / Footnote / Definition / Emoji 等）
 * 在 MarkdownIt 实例上注册插件。
 *
 * - 不处理 :::container 或 ```fence（分别由 syntax-container / syntax-fence 负责）；
 * - 当未提供 config 或 features 为空时，视为所有内置扩展均启用。
 */
export declare function registerMainSyntaxPlugins(md: MarkdownIt, config?: SupramarkConfig): void;
