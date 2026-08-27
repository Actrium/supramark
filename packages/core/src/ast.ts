// Position information (compatible with unist, extended for AST v2 source maps)
export interface Position {
  start: Point;
  end: Point;
}

export interface Point {
  line: number;
  column: number;
  offset?: number;
  byte_offset?: number;
  utf16_offset?: number;
}

export type SupramarkNodeType =
  // Block-level nodes
  | 'root'
  | 'paragraph'
  | 'heading'
  | 'code' // mdast: Code
  | 'list'
  | 'list_item'
  | 'blockquote'
  | 'thematic_break'
  | 'diagram'
  | 'container' // ::: syntax - all container extensions (map, html, admonition, etc.)
  | 'input' // %%% syntax - input block extensions (reserved)
  | 'math_block'
  | 'footnote_definition'
  | 'definition_list'
  | 'definition_item'
  | 'definition_term'
  | 'definition_description'
  | 'table'
  | 'table_row'
  | 'table_cell'
  | 'raw'
  | 'unsupported'
  // Inline-level nodes
  | 'text'
  | 'strong'
  | 'emphasis'
  | 'inline_code' // mdast: InlineCode
  | 'math_inline'
  | 'link'
  | 'image'
  | 'break'
  | 'delete' // GFM strikethrough
  | 'footnote_reference';

export type DiagnosticSeverity = 'info' | 'warning' | 'error';

export interface SupramarkDiagnostic {
  code: string;
  severity: DiagnosticSeverity;
  message: string;
  position?: Position;
  data?: Record<string, unknown>;
}

export interface SupramarkParserInfo {
  name: string;
  version?: string;
}

export interface SupramarkBaseNode {
  type: SupramarkNodeType;
  position?: Position; // Optional source position information
  data?: Record<string, unknown>; // Plugin custom data
  diagnostics?: SupramarkDiagnostic[];
}

export interface SupramarkTextNode extends SupramarkBaseNode {
  type: 'text';
  value: string;
}

/**
 * Diagram engine identifiers.
 *
 * - Kept in sync with the list used by isDiagramLanguage() in parse();
 * - Allows arbitrary strings so hosts can add custom engines.
 */
export const BUILT_IN_DIAGRAM_ENGINES = [
  'mermaid',
  'plantuml',
  'vega',
  'vega-lite',
  'echarts',
  'chart',
  'chartjs',
  'dot',
  'graphviz',
  'd2',
] as const;

export type BuiltInDiagramEngineId = (typeof BUILT_IN_DIAGRAM_ENGINES)[number];

// Any string is accepted (custom engines), so the type widens to `string`.
// The built-in ids live in `BUILT_IN_DIAGRAM_ENGINES` / `isBuiltInDiagramEngine`.
export type SupramarkDiagramEngineId = string;

export interface SupramarkDiagramNode extends SupramarkBaseNode {
  type: 'diagram';
  engine: SupramarkDiagramEngineId;
  code: string;
  /** True only when the Markdown source contains an explicit closing fence. */
  fence_closed: boolean;
  meta?: Record<string, unknown>;
}

/**
 * Map marker type (used by the data field of the :::map container).
 */
export interface SupramarkMapMarker {
  lat: number;
  lng: number;
  label?: string;
  id?: string;
  data?: Record<string, unknown>;
}

export type SupramarkExtensionMode = 'transparent' | 'opaque';

/**
 * A generic container node (uniformly represents :::xxx).
 *
 * - type is always 'container'
 * - name is the container's semantic name (e.g. 'map' / 'html' / 'note' / 'weather')
 * - params is the container's parameter string (e.g. "note title..." or "id=1"),
 *   interpreted by the specific extension
 * - data holds the extension's custom structured data (optional)
 *
 * Every ::: syntax extension produces this node type, distinguished by the name field.
 */
export interface SupramarkContainerNode extends SupramarkParentNode {
  type: 'container';
  name: string;
  mode?: SupramarkExtensionMode;
  params?: string;
  value?: string;
  data?: Record<string, unknown>;
}

/**
 * An input block node (uniformly represents %%%xxx).
 *
 * - type is always 'input'
 * - name is the input block's semantic name (e.g. 'form' / 'survey')
 * - params is the input block's parameter string, interpreted by the specific
 *   extension
 * - data holds the extension's custom structured data (optional)
 *
 * Every %%% syntax extension produces this node type, distinguished by the name
 * field.
 */
export interface SupramarkInputNode extends SupramarkParentNode {
  type: 'input';
  name: string;
  mode?: SupramarkExtensionMode;
  params?: string;
  value?: string;
  data?: Record<string, unknown>;
}

/**
 * Configuration for a single Diagram engine.
 */
export interface SupramarkDiagramEngineConfig {
  /** Whether this engine is enabled (optional; defaults to the Feature's decision) */
  enabled?: boolean;

  /** Render timeout (milliseconds), takes precedence over the global defaultTimeoutMs */
  timeoutMs?: number;

  /** Optional: the server address for a specific engine (e.g. a PlantUML server) */
  server?: string;

  /**
   * Optional: enable mermaid edge-label decluster (#93). Consumed only by the mermaid
   * engine; when off, output is byte-exact with upstream `mermaid@11.14.0`, when on it
   * pushes apart overlapping edge-label boxes and reserves realistic rendering width
   * for CJK (Chinese/Japanese/Korean) labels. Other engines ignore this field.
   */
  edgeLabelDecluster?: boolean;

  /** Cache configuration (informational for upstream layers only; the actual implementation is decided by the runtime) */
  cache?: {
    enabled?: boolean;
    maxSize?: number;
    /** Maximum estimated resident bytes for this engine's runtime cache. */
    maxBytes?: number;
    ttl?: number;
  };
}

/**
 * Global Diagram configuration.
 *
 * Consumed by the runtime (@supramark/rn / @supramark/web) to:
 * - set the default timeout and caching policy;
 * - provide extra options for individual engines (e.g. the PlantUML server).
 */
export interface SupramarkDiagramConfig {
  /** Default timeout (milliseconds), used for engines without their own config */
  defaultTimeoutMs?: number;

  /**
   * Host-provided cap on diagram display width. React Native only —
   * @supramark/web currently ignores this option and lays diagrams out with
   * CSS. Units are RN dp, and the value is a cap, not a target: display width
   * is clamped to [0.6 × cap, cap] where cap = min(windowWidth × 0.9,
   * maxWidth), so small diagrams still get the 0.6× floor. Wrapping
   * `<Supramark>` in a narrower container is NOT enough — the cap is derived
   * from the window, so hosts with a bubble narrower than the window must pass
   * the number explicitly or the diagram overflows the container.
   */
  maxWidth?: number;

  /** Default cache configuration */
  defaultCache?: {
    enabled?: boolean;
    maxSize?: number;
    /** Maximum estimated resident bytes for a runtime cache. */
    maxBytes?: number;
    ttl?: number;
  };

  /**
   * Per-engine configuration.
   *
   * - Explicit fields are provided for common built-in engines, for autocompletion;
   * - An index signature is also kept to support custom engines.
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
 * Determine whether the given engine is a built-in diagram engine.
 */
export function isBuiltInDiagramEngine(
  engine: SupramarkDiagramEngineId
): engine is BuiltInDiagramEngineId {
  const normalized = String(engine).toLowerCase();
  // The type assertion here is only to satisfy the TS check; the runtime comparison
  // is still a plain string comparison.
  return BUILT_IN_DIAGRAM_ENGINES.includes(normalized as BuiltInDiagramEngineId);
}

const warnedDiagramEngines = new Set<string>();

/**
 * Emit a one-time warning when a non-built-in diagram engine is used.
 *
 * - Does not block custom engines; only prints via console.warn the first time an
 *   unknown engine is encountered;
 * - Makes it easier to spot typos or undeclared engines while debugging.
 */
export function warnIfUnknownDiagramEngine(
  engine: SupramarkDiagramEngineId,
  context?: string
): void {
  if (isBuiltInDiagramEngine(engine)) return;

  const normalized = String(engine).toLowerCase();
  if (warnedDiagramEngines.has(normalized)) return;
  warnedDiagramEngines.add(normalized);

  // eslint-disable-next-line no-console
  if (typeof console !== 'undefined' && typeof console.warn === 'function') {
    const details = context ? ` (${context})` : '';
    console.warn(
      `[supramark] Unknown diagram engine "${engine}"${details}. ` +
        'If this is a custom engine, make sure to: ' +
        '1) recognize it as a diagram node in the parsing layer; ' +
        '2) define a corresponding Feature and rendering implementation for it.'
    );
  }
}

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

export type SupramarkCodeHighlightFontStyle = 'bold' | 'italic' | 'underline';

export interface SupramarkCodeHighlightToken {
  text: string;
  color?: string;
  backgroundColor?: string;
  fontStyle?: SupramarkCodeHighlightFontStyle[];
}

export interface SupramarkCodeHighlightLine {
  tokens: SupramarkCodeHighlightToken[];
}

export interface SupramarkCodeHighlightResult {
  language?: string;
  theme?: string;
  lines: SupramarkCodeHighlightLine[];
}

export interface SupramarkCodeHighlightInput {
  code: string;
  lang?: string;
  meta?: string;
  theme?: string;
}

export type SupramarkCodeHighlighter = (
  input: SupramarkCodeHighlightInput
) => Promise<SupramarkCodeHighlightResult | null | undefined>;

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
 * Inline math formula node (matches `$...$`).
 *
 * Like block-level Math, it only stores the raw TeX text.
 */
export interface SupramarkMathInlineNode extends SupramarkBaseNode {
  type: 'math_inline';
  value: string;
}

/**
 * Footnote reference node, e.g. `[^1]` or `^[inline]` within the body text.
 *
 * - index: the number shown to the user (starting from 1)
 * - label: the raw label (e.g. `1` or `note`); may be empty for inline footnotes
 * - subId: the sub-index when the same footnote is referenced multiple times
 *   (starting from 0)
 */
export interface SupramarkFootnoteReferenceNode extends SupramarkBaseNode {
  type: 'footnote_reference';
  index: number;
  label?: string;
  /** The normalized reference key (leading/trailing whitespace trimmed, internal
   * whitespace collapsed to a single space, lowercased), used to associate refs with
   * defs. */
  identifier: string;
  subId?: number;
}

/**
 * Footnote definition node, matching a form like:
 *
 * ```markdown
 * This is body text[^1]
 *
 * [^1]: This is the footnote content
 * ```
 *
 * All footnote definitions are appended to the end of the document (the tail of
 * root.children).
 */
export interface SupramarkFootnoteDefinitionNode extends SupramarkParentNode {
  type: 'footnote_definition';
  index: number;
  label?: string;
  /** The normalized reference key (leading/trailing whitespace trimmed, internal
   * whitespace collapsed to a single space, lowercased), used to associate refs with
   * defs. */
  identifier: string;
}

/**
 * Definition list, matching the Markdown Extra / Pandoc style:
 *
 * Term
 * :   Description one
 * :   Description two
 */
export interface SupramarkDefinitionListNode extends SupramarkParentNode {
  type: 'definition_list';
  children: SupramarkDefinitionItemNode[];
}

export interface SupramarkDefinitionItemNode extends SupramarkParentNode {
  type: 'definition_item';
  children: Array<SupramarkDefinitionTermNode | SupramarkDefinitionDescriptionNode>;
}

export interface SupramarkDefinitionTermNode extends SupramarkParentNode {
  type: 'definition_term';
}

export interface SupramarkDefinitionDescriptionNode extends SupramarkParentNode {
  type: 'definition_description';
}

/**
 * Admonition kind constants (used by container extensions like :::note, :::warning).
 *
 * Note: Admonition now uniformly uses SupramarkContainerNode, distinguished by the
 * name field ('note', 'tip', 'warning', etc.).
 */
export const SUPRAMARK_ADMONITION_KINDS = ['note', 'tip', 'info', 'warning', 'danger'] as const;

export type SupramarkAdmonitionKind = (typeof SUPRAMARK_ADMONITION_KINDS)[number];

export interface SupramarkListNode extends SupramarkParentNode {
  type: 'list';
  ordered: boolean;
  start?: number;
  tight?: boolean;
}

export interface SupramarkListItemNode extends SupramarkParentNode {
  type: 'list_item';
  checked?: boolean;
}

export interface SupramarkBlockquoteNode extends SupramarkParentNode {
  type: 'blockquote';
}

export interface SupramarkThematicBreakNode extends SupramarkBaseNode {
  type: 'thematic_break';
}

// Inline nodes
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
  alt: string;
  title?: string;
}

export interface SupramarkBreakNode extends SupramarkBaseNode {
  type: 'break';
}

export interface SupramarkDeleteNode extends SupramarkParentNode {
  type: 'delete';
}

// GFM Table nodes
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

export interface SupramarkRawNode extends SupramarkBaseNode {
  type: 'raw';
  format: string;
  value: string;
  block: boolean;
}

export interface SupramarkUnsupportedNode extends SupramarkParentNode {
  type: 'unsupported';
  syntax: string;
  reason: string;
  value?: string;
  diagnostics?: SupramarkDiagnostic[];
}

// Node type unions
export type SupramarkBlockNode =
  | SupramarkParagraphNode
  | SupramarkHeadingNode
  | SupramarkCodeNode
  | SupramarkMathBlockNode
  | SupramarkFootnoteDefinitionNode
  | SupramarkDefinitionListNode
  | SupramarkDefinitionItemNode
  | SupramarkDefinitionTermNode
  | SupramarkDefinitionDescriptionNode
  | SupramarkListNode
  | SupramarkListItemNode
  | SupramarkBlockquoteNode
  | SupramarkThematicBreakNode
  | SupramarkDiagramNode
  | SupramarkContainerNode // ::: extensions (map, html, note, weather, etc.)
  | SupramarkInputNode // %%% extensions (form, survey, etc.)
  | SupramarkTableNode
  | SupramarkTableRowNode
  | SupramarkTableCellNode
  | SupramarkRawNode
  | SupramarkUnsupportedNode;

export type SupramarkInlineNode =
  | SupramarkTextNode
  | SupramarkStrongNode
  | SupramarkEmphasisNode
  | SupramarkInlineCodeNode
  | SupramarkMathInlineNode
  | SupramarkFootnoteReferenceNode
  | SupramarkLinkNode
  | SupramarkImageNode
  | SupramarkBreakNode
  | SupramarkDeleteNode
  | SupramarkRawNode
  | SupramarkUnsupportedNode;

export type SupramarkNode = SupramarkRootNode | SupramarkBlockNode | SupramarkInlineNode;

export interface SupramarkRootNode extends SupramarkParentNode {
  type: 'root';
  ast_version: 2;
  diagnostics: SupramarkDiagnostic[];
  parser?: SupramarkParserInfo;
}
