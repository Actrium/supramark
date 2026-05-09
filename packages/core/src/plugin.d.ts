import type { SupramarkRootNode } from './ast.js';
import { type SupramarkConfig } from './feature.js';
/**
 * 插件解析上下文，提供给插件访问原始数据和共享状态。
 */
export interface SupramarkParseContext {
    /**
     * 原始 markdown 文本。
     */
    source: string;
    /**
     * 插件共享数据存储，插件可以在这里存储和读取数据。
     * 用于插件间通信。
     */
    data: Record<string, unknown>;
}
/**
 * Supramark 插件接口。
 *
 * 插件可以在解析过程中转换 AST，添加自定义节点类型，
 * 或者为现有节点添加额外的数据。
 *
 * @example
 * ```typescript
 * const myPlugin: SupramarkPlugin = {
 *   name: 'my-plugin',
 *   transform(root, context) {
 *     // 遍历 AST 并修改节点
 *     root.children.forEach(node => {
 *       if (node.type === 'heading') {
 *         // 为标题添加 ID
 *         if (!node.data) node.data = {};
 *         node.data.id = generateId(node);
 *       }
 *     });
 *   }
 * };
 * ```
 */
export interface SupramarkPlugin {
    /**
     * 插件名称，必须唯一。
     *
     * 推荐使用 npm 包名格式，如 '@supramark/plugin-gfm' 或 'supramark-plugin-toc'。
     */
    name: string;
    /**
     * 插件版本（可选）。
     *
     * 用于调试和兼容性检查。
     */
    version?: string;
    /**
     * 插件依赖列表（可选）。
     *
     * 列出此插件依赖的其他插件名称。
     * 解析器会确保依赖的插件在此插件之前执行。
     *
     * @example
     * ```typescript
     * {
     *   name: 'plugin-enhanced-gfm',
     *   dependencies: ['plugin-gfm', 'plugin-emoji']
     * }
     * ```
     */
    dependencies?: string[];
    /**
     * 解析阶段的 AST 转换钩子。
     *
     * 此方法在 markdown 解析为初始 AST 之后执行，
     * 插件可以遍历和修改 AST 树。
     *
     * @param root - Supramark AST 根节点
     * @param context - 解析上下文，包含原始文本和共享数据
     *
     * @example
     * ```typescript
     * transform(root, context) {
     *   // 添加自定义节点
     *   root.children.push({
     *     type: 'custom',
     *     data: { foo: 'bar' }
     *   });
     * }
     * ```
     */
    transform?(root: SupramarkRootNode, context: SupramarkParseContext): void | Promise<void>;
}
/**
 * Markdown 解析选项。
 */
export interface SupramarkParseOptions {
    /**
     * 插件列表。
     *
     * 插件将按照依赖关系排序后依次执行。
     * 如果没有依赖关系，则按照数组顺序执行。
     *
     * @example
     * ```typescript
     * parseMarkdown(markdown, {
     *   plugins: [gfmPlugin(), diagramPlugin(), tocPlugin()]
     * });
     * ```
     */
    plugins?: SupramarkPlugin[];
    /**
     * Feature 运行时配置（可选）
     *
     * - 如果提供，将用于决定是否启用某些扩展语法的 AST 建模（如 Math / Footnote / Definition / Admonition / GFM 表格等）；
     * - 未提供或 features 为空时，行为与此前版本保持一致：视为所有内置扩展均启用。
     */
    config?: SupramarkConfig;
}
export declare function parseMarkdown(markdown: string, options?: SupramarkParseOptions): Promise<SupramarkRootNode>;
/**
 * Supramark 预设类型。
 *
 * 预设是一个返回解析选项的函数，用于快速配置常见的插件组合。
 *
 * @example
 * ```typescript
 * // 使用预设
 * const ast = await parseMarkdown(markdown, presetGFM());
 * ```
 */
export type SupramarkPreset = () => SupramarkParseOptions;
/**
 * 默认预设。
 *
 * 包含基础 Markdown 功能和 GFM 扩展（删除线、任务列表、表格）。
 * 这是推荐的默认配置。
 *
 * @returns 解析选项
 *
 * @example
 * ```typescript
 * const ast = await parseMarkdown(markdown, presetDefault());
 * ```
 */
export declare function presetDefault(): SupramarkParseOptions;
/**
 * GFM（GitHub Flavored Markdown）预设。
 *
 * 包含 GitHub Flavored Markdown 的所有扩展功能：
 * - 删除线（strikethrough）: ~~text~~
 * - 任务列表（task lists）: - [ ] / - [x]
 * - 表格（tables）
 *
 * 注意：当前这些功能已内置启用，此预设主要用于文档和语义化目的。
 *
 * @returns 解析选项
 *
 * @example
 * ```typescript
 * const ast = await parseMarkdown(markdown, presetGFM());
 * ```
 */
export declare function presetGFM(): SupramarkParseOptions;
