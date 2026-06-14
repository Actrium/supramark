import type { SupramarkConfig } from '../feature.js';

/**
 * @deprecated Markdown token registration was removed in AST v2.
 */
export function registerMainSyntaxPlugins(_parser: unknown, _config?: SupramarkConfig): void {
  // Core Markdown, GFM, math, footnotes, and definition lists are parsed by supramark-markdown.
}
