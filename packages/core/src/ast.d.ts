export interface Position {
    start: Point;
    end: Point;
}
export interface Point {
    line: number;
    column: number;
    offset?: number;
}
export type SupramarkNodeType = 'root' | 'paragraph' | 'heading' | 'code' | 'list' | 'list_item' | 'blockquote' | 'thematic_break' | 'diagram' | 'container' | 'input' | 'math_block' | 'footnote_definition' | 'definition_list' | 'definition_item' | 'table' | 'table_row' | 'table_cell' | 'text' | 'strong' | 'emphasis' | 'inline_code' | 'math_inline' | 'link' | 'image' | 'break' | 'delete' | 'footnote_reference';
export interface SupramarkBaseNode {
    type: SupramarkNodeType;
    position?: Position;
    data?: Record<string, unknown>;
}
export interface SupramarkTextNode extends SupramarkBaseNode {
    type: 'text';
    value: string;
}
/**
 * Diagram 引擎标识符
 *
 * - 与 parseMarkdown() 中 isDiagramLanguage() 的列表保持一致；
 * - 允许扩展字符串，方便宿主添加自定义引擎。
 */
export declare const BUILT_IN_DIAGRAM_ENGINES: readonly ["mermaid", "plantuml", "vega", "vega-lite", "echarts", "chart", "chartjs", "dot", "graphviz", "d2"];
export type BuiltInDiagramEngineId = (typeof BUILT_IN_DIAGRAM_ENGINES)[number];
export type SupramarkDiagramEngineId = BuiltInDiagramEngineId | string;
export interface SupramarkDiagramNode extends SupramarkBaseNode {
    type: 'diagram';
    engine: SupramarkDiagramEngineId;
    code: string;
    meta?: Record<string, unknown>;
}
/**
 * 地图标记点类型（供 :::map 容器的 data 字段使用）
 */
export interface SupramarkMapMarker {
    lat: number;
    lng: number;
}
/**
 * 通用容器节点（统一表达 :::xxx）
 *
 * - type 固定为 'container'
 * - name 为容器语义名（例如 'map' / 'html' / 'note' / 'weather' 等）
 * - params 为容器的参数字符串（例如 "note title..." 或 "id=1"），由具体扩展自行解释
 * - data 为扩展自定义结构化数据（可选）
 *
 * 所有 ::: 语法的扩展都生成此节点类型，通过 name 字段区分具体扩展。
 */
export interface SupramarkContainerNode extends SupramarkParentNode {
    type: 'container';
    name: string;
    params?: string;
    data?: Record<string, unknown>;
}
/**
 * 输入块节点（统一表达 %%%xxx）
 *
 * - type 固定为 'input'
 * - name 为输入块语义名（例如 'form' / 'survey' 等）
 * - params 为输入块的参数字符串，由具体扩展自行解释
 * - data 为扩展自定义结构化数据（可选）
 *
 * 所有 %%% 语法的扩展都生成此节点类型，通过 name 字段区分具体扩展。
 */
export interface SupramarkInputNode extends SupramarkParentNode {
    type: 'input';
    name: string;
    params?: string;
    data?: Record<string, unknown>;
}
/**
 * 单个 Diagram 引擎的配置
 */
export interface SupramarkDiagramEngineConfig {
    /** 是否启用此引擎（可选，默认由 Feature 决定） */
    enabled?: boolean;
    /** 渲染超时时间（毫秒），优先于全局 defaultTimeoutMs */
    timeoutMs?: number;
    /** 可选：特定引擎的服务端地址（例如 PlantUML server） */
    server?: string;
    /** 缓存配置（仅作为上层参考，具体实现由运行时决定） */
    cache?: {
        enabled?: boolean;
        maxSize?: number;
        ttl?: number;
    };
}
/**
 * Diagram 全局配置
 *
 * 由运行时（@supramark/rn / @supramark/web）消费，用于：
 * - 设置默认超时与缓存策略；
 * - 为各个引擎提供附加选项（如 PlantUML server）。
 */
export interface SupramarkDiagramConfig {
    /** 默认超时时间（毫秒），用于未单独配置的引擎 */
    defaultTimeoutMs?: number;
    /** 默认缓存配置 */
    defaultCache?: {
        enabled?: boolean;
        maxSize?: number;
        ttl?: number;
    };
    /**
     * 各个引擎的配置
     *
     * - 对常见内置引擎提供显式字段，方便补全；
     * - 同时保留索引签名以支持自定义引擎。
     */
    engines?: {
        mermaid?: SupramarkDiagramEngineConfig;
        plantuml?: SupramarkDiagramEngineConfig;
        vega?: SupramarkDiagramEngineConfig;
        'vega-lite'?: SupramarkDiagramEngineConfig;
        echarts?: SupramarkDiagramEngineConfig;
        chart?: SupramarkDiagramEngineConfig;
        chartjs?: SupramarkDiagramEngineConfig;
        dot?: SupramarkDiagramEngineConfig;
        graphviz?: SupramarkDiagramEngineConfig;
        d2?: SupramarkDiagramEngineConfig;
        [engineId: string]: SupramarkDiagramEngineConfig | undefined;
    };
}
/**
 * 判断给定 engine 是否为内置图表引擎。
 */
export declare function isBuiltInDiagramEngine(engine: SupramarkDiagramEngineId): engine is BuiltInDiagramEngineId;
/**
 * 当使用非内置 diagram engine 时给出一次性告警。
 *
 * - 不阻止自定义引擎，仅在第一次遇到未知 engine 时通过 console.warn 提示；
 * - 方便在调试阶段发现拼写错误或未声明的引擎。
 */
export declare function warnIfUnknownDiagramEngine(engine: SupramarkDiagramEngineId, context?: string): void;
export interface SupramarkParentNode extends SupramarkBaseNode {
    children: SupramarkNode[];
}
export interface SupramarkParagraphNode extends SupramarkParentNode {
    type: 'paragraph';
}
export interface SupramarkHeadingNode extends SupramarkParentNode {
    type: 'heading';
    depth: 1 | 2 | 3 | 4 | 5 | 6;
}
export interface SupramarkCodeNode extends SupramarkBaseNode {
    type: 'code';
    value: string;
    lang?: string;
    meta?: string;
}
/**
 * Block-level math node (matches Markdown's `$$ ... $$`).
 *
 * Semantically close to mdast's "math" node — supramark keeps only the
 * raw TeX source here. The rendering surface is the host's renderer
 * (typically `@supramark/engines/mathjax` for SSR-side SVG output, or
 * a host-supplied KaTeX path).
 */
export interface SupramarkMathBlockNode extends SupramarkBaseNode {
    type: 'math_block';
    value: string;
}
export interface SupramarkInlineCodeNode extends SupramarkBaseNode {
    type: 'inline_code';
    value: string;
}
/**
 * 行内数学公式节点（对应 $...$）
 *
 * 与块级 Math 一样，仅保存原始 TeX 文本。
 */
export interface SupramarkMathInlineNode extends SupramarkBaseNode {
    type: 'math_inline';
    value: string;
}
/**
 * 脚注引用节点，例如正文中的 `[^1]` 或 `^[inline]`。
 *
 * - index：用于展示给用户看的编号（从 1 开始）
 * - label：原始 label（如 `1` 或 `note`），内联脚注可能为空
 * - subId：同一脚注被多次引用时的子编号（从 0 开始）
 */
export interface SupramarkFootnoteReferenceNode extends SupramarkBaseNode {
    type: 'footnote_reference';
    index: number;
    label?: string;
    subId?: number;
}
/**
 * 脚注定义节点，对应形如：
 *
 * ```markdown
 * 这是正文[^1]
 *
 * [^1]: 这里是脚注内容
 * ```
 *
 * 所有脚注定义会被追加到文档末尾（root.children 的后部）。
 */
export interface SupramarkFootnoteDefinitionNode extends SupramarkParentNode {
    type: 'footnote_definition';
    index: number;
    label?: string;
}
/**
 * 定义列表（definition list），对应 Markdown Extra / Pandoc 风格：
 *
 * Term
 * :   描述一
 * :   描述二
 */
export interface SupramarkDefinitionListNode extends SupramarkParentNode {
    type: 'definition_list';
    children: SupramarkDefinitionItemNode[];
}
/**
 * 定义列表中的单个条目。
 *
 * - term: 术语部分（通常是一个行内节点序列）
 * - descriptions: 描述段落列表，每个元素是一组块级/行内节点
 */
export interface SupramarkDefinitionItemNode extends SupramarkBaseNode {
    type: 'definition_item';
    term: SupramarkNode[];
    descriptions: SupramarkNode[][];
}
/**
 * Admonition 类型常量（供 :::note, :::warning 等容器扩展使用）
 *
 * 注意：Admonition 现在统一使用 SupramarkContainerNode，
 * 通过 name 字段区分类型（'note', 'tip', 'warning' 等）。
 */
export declare const SUPRAMARK_ADMONITION_KINDS: readonly ["note", "tip", "info", "warning", "danger"];
export type SupramarkAdmonitionKind = (typeof SUPRAMARK_ADMONITION_KINDS)[number];
export interface SupramarkListNode extends SupramarkParentNode {
    type: 'list';
    ordered: boolean;
    start: number | null;
    tight?: boolean;
}
export interface SupramarkListItemNode extends SupramarkParentNode {
    type: 'list_item';
    checked?: boolean | null;
}
export interface SupramarkBlockquoteNode extends SupramarkParentNode {
    type: 'blockquote';
}
export interface SupramarkThematicBreakNode extends SupramarkBaseNode {
    type: 'thematic_break';
}
export interface SupramarkStrongNode extends SupramarkParentNode {
    type: 'strong';
}
export interface SupramarkEmphasisNode extends SupramarkParentNode {
    type: 'emphasis';
}
export interface SupramarkLinkNode extends SupramarkParentNode {
    type: 'link';
    url: string;
    title?: string;
}
export interface SupramarkImageNode extends SupramarkBaseNode {
    type: 'image';
    url: string;
    alt?: string;
    title?: string;
}
export interface SupramarkBreakNode extends SupramarkBaseNode {
    type: 'break';
}
export interface SupramarkDeleteNode extends SupramarkParentNode {
    type: 'delete';
}
export interface SupramarkTableNode extends SupramarkParentNode {
    type: 'table';
    align?: ('left' | 'right' | 'center' | null)[];
}
export interface SupramarkTableRowNode extends SupramarkParentNode {
    type: 'table_row';
}
export interface SupramarkTableCellNode extends SupramarkParentNode {
    type: 'table_cell';
    align?: 'left' | 'right' | 'center' | null;
    header?: boolean;
}
export type SupramarkBlockNode = SupramarkParagraphNode | SupramarkHeadingNode | SupramarkCodeNode | SupramarkMathBlockNode | SupramarkFootnoteDefinitionNode | SupramarkDefinitionListNode | SupramarkDefinitionItemNode | SupramarkListNode | SupramarkListItemNode | SupramarkBlockquoteNode | SupramarkThematicBreakNode | SupramarkDiagramNode | SupramarkContainerNode | SupramarkInputNode | SupramarkTableNode | SupramarkTableRowNode | SupramarkTableCellNode;
export type SupramarkInlineNode = SupramarkTextNode | SupramarkStrongNode | SupramarkEmphasisNode | SupramarkInlineCodeNode | SupramarkMathInlineNode | SupramarkFootnoteReferenceNode | SupramarkLinkNode | SupramarkImageNode | SupramarkBreakNode | SupramarkDeleteNode;
export type SupramarkNode = SupramarkRootNode | SupramarkBlockNode | SupramarkInlineNode;
export interface SupramarkRootNode extends SupramarkParentNode {
    type: 'root';
}
