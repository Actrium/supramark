import type { SupramarkRootNode } from './ast.js';
import type { SupramarkParseOptions } from './plugin.js';
export declare function parseMarkdownWithRemark(markdown: string, options?: SupramarkParseOptions): Promise<SupramarkRootNode>;
