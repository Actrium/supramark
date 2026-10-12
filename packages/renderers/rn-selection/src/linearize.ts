import type {
  SupramarkBlockquoteNode,
  SupramarkCodeNode,
  SupramarkContainerNode,
  SupramarkDeleteNode,
  SupramarkDiagramNode,
  SupramarkEmphasisNode,
  SupramarkLinkNode,
  SupramarkListItemNode,
  SupramarkListNode,
  SupramarkMathBlockNode,
  SupramarkMathInlineNode,
  SupramarkNode,
  SupramarkParentNode,
  SupramarkRootNode,
  SupramarkStrongNode,
  SupramarkTableCellNode,
  SupramarkTableNode,
  SupramarkTableRowNode,
} from '@supramark/core';
import { isInlineNode } from '@supramark/core';
import type {
  SelectionAtomUnit,
  SelectionBoundaryUnit,
  SelectionBreakUnit,
  SelectionContext,
  SelectionNodeId,
  SelectionPathSegment,
  SelectionPayload,
  SelectionSourceRange,
  SelectionTextUnit,
  SelectionUnit,
} from './model';
import type { SelectionProvider } from './providers';

export interface LinearizeSelectionOptions extends SelectionContext {
  providers?: readonly SelectionProvider[];
  /**
   * Custom node identity. Must be unique per document node and stable across
   * re-renders of the same content, and must never collide between a node and
   * one of its descendants — colliding identities make their units share a
   * unitId, which corrupts every offset past the collision.
   */
  nodeId?: (node: SupramarkNode, path: readonly SelectionPathSegment[]) => string;
  /**
   * Container names whose content is NOT rendered as readable text (host cards,
   * canvas views). Those stay a hard boundary. Everything else — admonitions
   * and unrecognized containers — renders its children through `renderNode`,
   * so the body is linearized and selectable like any other content. The AST
   * cannot tell the two apart (the canonical parser marks every `:::xxx`
   * opaque), so the render side owns this list.
   */
  opaqueContainers?: ReadonlySet<string>;
  /**
   * The title a transparent container renders above its children (taken from
   * `params` / `data.title` by the renderer). The title is visible body text,
   * so it gets its own unit and is bound to the container node; return
   * `undefined` (or an empty string) when the container has no title.
   */
  containerTitle?: (node: SupramarkContainerNode) => string | undefined;
  /**
   * Whether a node currently renders as readable text rather than as a
   * non-text object — a disabled math / diagram feature draws its raw source
   * (see `renderDisabledMathBlock` / `renderDisabledDiagram`), so selection
   * must treat it as text. Mirrors the renderer's own feature switches so the
   * two sides agree on what is on screen.
   */
  rendersAsText?: (node: SupramarkNode) => boolean;
}

/**
 * List rendering context threaded explicitly through the recursion so it never
 * leaks into `SelectionContext`/`options` (which are exposed to providers).
 */
interface ListContext {
  depth: number;
  ordered: boolean;
  start: number;
  index: number;
}

/**
 * One rendered text block, as the renderer needs it to mount a SelectableBlock:
 * the AST node to match on (by identity), the block's selectable id, and the
 * visible text units the block renders.
 *
 * `unitIds` holds text, visible hard breaks and inline attachment atoms. Each
 * atom occupies one native display offset while remaining a hard document
 * boundary. Syntax markers and trailing block separators are excluded.
 */
export interface SelectableBlockBinding {
  node: SupramarkNode;
  nodeId: SelectionNodeId;
  unitIds: SelectionNodeId[];
}

export interface LinearizedDocument {
  units: SelectionUnit[];
  /**
   * Bindings for the block types a renderer can mount as one selectable text
   * block: paragraph, heading, tight list_item, code, and table_cell. Whether a
   * specific rendered instance uses its binding stays a renderer decision (an
   * image-only paragraph renders as a gallery; a loose list item prefixes the
   * marker onto its first paragraph), so extra bindings are harmless.
   */
  blocks: SelectableBlockBinding[];
}

interface BlockCollector {
  blocks: SelectableBlockBinding[];
}

/**
 * Record one block's visible text units. Blocks with no visible text (empty
 * code fences, empty table cells, image-only paragraphs) are skipped: there is
 * nothing to select, so the renderer should not mount a SelectableBlock.
 */
function collectBlock(
  collector: BlockCollector | undefined,
  node: SupramarkNode,
  nodeId: SelectionNodeId,
  units: readonly SelectionUnit[]
): void {
  if (collector === undefined) return;
  const unitIds = units
    // Hard line breaks occupy an offset inside the native Text, unlike block separators.
    .filter(
      unit =>
        (unit.kind === 'text' && unit.text !== '') ||
        (unit.kind === 'break' && unit.reason === 'line') ||
        unit.kind === 'atom'
    )
    .map(unit => unit.unitId);
  // Object-only blocks have no selectable text (image galleries keep their renderer).
  if (!units.some(unit => unit.kind === 'text' && unit.text !== '')) return;
  collector.blocks.push({ node, nodeId, unitIds });
}

export function linearizeForSelection(
  root: SupramarkRootNode | SupramarkNode[],
  options: LinearizeSelectionOptions = {}
): SelectionUnit[] {
  return linearizeDocument(root, options).units;
}

/**
 * Linearize a document into the selection unit stream AND the per-block
 * bindings a renderer mounts as SelectableBlocks. One traversal produces both,
 * so unit ids and block ids cannot drift apart, and the renderer matches
 * bindings by AST node identity — no id or traversal-path convention to keep in
 * sync between the two sides.
 */
export function linearizeDocument(
  root: SupramarkRootNode | SupramarkNode[],
  options: LinearizeSelectionOptions = {}
): LinearizedDocument {
  // `?? []`: a malformed root (no children array) linearizes to an empty
  // document instead of throwing mid-render, so the renderer's own error
  // boundary — not the selection layer — reports the malformed AST.
  const nodes = Array.isArray(root) ? root : (root.children ?? []);
  const collector: BlockCollector = { blocks: [] };
  const units = nodes.flatMap((node, index) =>
    linearizeNode(node, [index], options, undefined, collector)
  );
  return { units, blocks: collector.blocks };
}

function linearizeNode(
  node: SupramarkNode,
  path: readonly SelectionPathSegment[],
  options: LinearizeSelectionOptions,
  listCtx?: ListContext,
  collector?: BlockCollector
): SelectionUnit[] {
  const nodeId = resolveNodeId(node, path, options);
  const provider = options.providers?.find(candidate => candidate.match(node));
  const customUnits = provider?.getUnits?.(node, { ...options, nodeId });
  if (customUnits) return customUnits;

  switch (node.type) {
    case 'text': {
      const text = node;
      return [textUnit(makeUnitId(nodeId, 0), nodeId, text, text.value, getSourceRange(node))];
    }
    case 'paragraph': {
      const units = [
        ...linearizeChildren(node as SupramarkParentNode, path, options, listCtx, collector),
        breakUnit(makeUnitId(nodeId, 0), nodeId, 'block', node),
      ];
      collectBlock(collector, node, nodeId, units);
      return units;
    }
    case 'heading': {
      const heading = node;
      const prefix = '#'.repeat(heading.depth) + ' ';
      const units = [
        syntaxUnit(makeUnitId(nodeId, 0), nodeId, heading, prefix),
        ...linearizeChildren(heading, path, options, undefined, collector),
        breakUnit(makeUnitId(nodeId, 1), nodeId, 'block', heading),
      ];
      collectBlock(collector, heading, nodeId, units);
      return units;
    }
    case 'strong': {
      const strong = node;
      return wrapInline(strong, nodeId, path, options, listCtx, collector, '**', '**');
    }
    case 'emphasis': {
      const emphasis = node;
      return wrapInline(emphasis, nodeId, path, options, listCtx, collector, '_', '_');
    }
    case 'delete': {
      const del = node;
      return wrapInline(del, nodeId, path, options, listCtx, collector, '~~', '~~');
    }
    case 'link': {
      const link = node;
      const title = link.title ? ` "${link.title}"` : '';
      return wrapInline(
        link,
        nodeId,
        path,
        options,
        listCtx,
        collector,
        '[',
        `](${link.url}${title})`
      );
    }
    case 'inline_code': {
      const code = node;
      const markdown = '`' + code.value + '`';
      return [
        {
          kind: 'text',
          unitId: makeUnitId(nodeId, 0),
          nodeId,
          text: code.value,
          node: code,
          payload: { markdown, source: markdown },
          sourceRange: getSourceRange(node),
        },
      ];
    }
    case 'break': {
      const lineBreak = node;
      return [breakUnit(makeUnitId(nodeId, 0), nodeId, 'line', lineBreak)];
    }
    case 'blockquote': {
      const blockquote = node;
      // Per-line prefixing: a leading `> `, then another `> ` after every
      // interior break so each line is quoted without a dangling prefix on the
      // final line. The prefixes are text:'' syntax units (invisible to
      // plainText), so the quoted text itself stays clean.
      const inner = linearizeChildren(blockquote, path, options, listCtx, collector);
      return [
        syntaxUnit(makeUnitId(nodeId, 0), nodeId, blockquote, '> '),
        ...prefixBlockquoteInterior(inner, nodeId, blockquote),
        breakUnit(makeUnitId(nodeId, 1), nodeId, 'block', blockquote),
      ];
    }
    case 'list':
      return linearizeList(node, path, options, listCtx, collector);
    case 'list_item':
      return linearizeListItem(node, path, options, listCtx, collector);
    case 'code': {
      const units = linearizeCodeBlock(node, nodeId);
      collectBlock(collector, node, nodeId, units);
      return units;
    }
    case 'image': {
      const image = node;
      const alt = image.alt ?? '';
      const markdown = `![${alt}](${image.url})`;
      // Image is an inline atom with no trailing break so it can sit mid-paragraph.
      return [
        {
          kind: 'atom',
          unitId: makeUnitId(nodeId, 0),
          nodeId,
          label: 'image',
          node: image,
          payload: {
            plainText: alt,
            markdown,
            source: markdown,
            metadata: { url: image.url, alt, title: image.title },
          },
        },
      ];
    }
    case 'definition_list':
      return linearizeChildren(node, path, options, listCtx, collector);
    case 'definition_item':
      return linearizeChildren(node, path, options, listCtx, collector);
    case 'definition_term': {
      const term = node;
      const units = [
        ...linearizeChildren(term, path, options, listCtx, collector),
        breakUnit(makeUnitId(nodeId, 0), nodeId, 'block', term),
      ];
      collectBlock(collector, term, nodeId, units);
      return units;
    }
    case 'definition_description': {
      const description = node;
      // In AST v2 the description's children are BLOCK nodes (paragraphs), so
      // the body belongs to those children and collects on its own — binding
      // the description node here would double-count their units. The `: `
      // syntax unit stays for Markdown serialization only.
      return [
        syntaxUnit(makeUnitId(nodeId, 0), nodeId, description, ': '),
        ...linearizeChildren(description, path, options, listCtx, collector),
        breakUnit(makeUnitId(nodeId, 1), nodeId, 'block', description),
      ];
    }
    case 'footnote_reference': {
      const ref = node;
      // The renderer draws `[index]`, so the visible unit must match it. The
      // label and the Markdown reference syntax stay separate: `[^label]` is
      // rebuilt on Markdown/source serialization and never leaks into the
      // screen offsets the user selects against.
      const markdown = `[^${ref.label ?? ref.index}]`;
      return [
        {
          kind: 'text',
          unitId: makeUnitId(nodeId, 0),
          nodeId,
          text: `[${ref.index}]`,
          node: ref,
          payload: { markdown, source: markdown },
          sourceRange: getSourceRange(node),
        },
      ];
    }
    case 'footnote_definition': {
      const def = node;
      const marker = `[^${def.label ?? def.index}]: `;
      return [
        syntaxUnit(makeUnitId(nodeId, 0), nodeId, def, marker),
        ...linearizeChildren(def, path, options, listCtx, collector),
        breakUnit(makeUnitId(nodeId, 1), nodeId, 'block', def),
      ];
    }
    case 'raw': {
      const raw = node;
      // Inline HTML renders as nothing (renderInlineNode returns null), and
      // block-level raw HTML falls through to null as well: the unit carries no
      // visible text and no Markdown payload, so a hidden node neither occupies
      // screen offsets nor copies its tags. The original syntax stays in
      // `source` alone, keeping source serialization verbatim.
      return [
        {
          kind: 'text',
          unitId: makeUnitId(nodeId, 0),
          nodeId,
          text: '',
          node: raw,
          payload: { source: raw.value },
          sourceRange: getSourceRange(node),
        },
      ];
    }
    case 'thematic_break': {
      const rule = node;
      return [
        syntaxUnit(makeUnitId(nodeId, 0), nodeId, rule, '---'),
        breakUnit(makeUnitId(nodeId, 1), nodeId, 'block', rule),
      ];
    }
    case 'math_inline': {
      // A disabled math feature draws the raw TeX as readable text
      // (renderInlineNode returns the value), so selection treats it as text
      // instead of an object.
      if (options.rendersAsText?.(node) === true) {
        return [renderedSourceUnit(nodeId, node, node.value, `$${node.value}$`)];
      }
      return [mathAtom(node, nodeId, false)];
    }
    case 'math_block': {
      if (options.rendersAsText?.(node) === true) {
        const units = [
          renderedSourceUnit(nodeId, node, node.value, `$$\n${node.value}\n$$`),
          breakUnit(makeUnitId(nodeId, 1), nodeId, 'block', node),
        ];
        collectBlock(collector, node, nodeId, units);
        return units;
      }
      return [
        mathAtom(node, nodeId, true),
        breakUnit(makeUnitId(nodeId, 1), nodeId, 'block', node),
      ];
    }
    case 'diagram': {
      if (options.rendersAsText?.(node) === true) {
        const fenced = '```' + node.engine + '\n' + node.code + '\n```';
        const units = [
          renderedSourceUnit(nodeId, node, node.code, fenced),
          breakUnit(makeUnitId(nodeId, 1), nodeId, 'block', node),
        ];
        collectBlock(collector, node, nodeId, units);
        return units;
      }
      return [diagramAtom(node, nodeId), breakUnit(makeUnitId(nodeId, 1), nodeId, 'block', node)];
    }
    // Tables linearize into a fully compositional unit stream: per-cell inline
    // text units plus structural text units carrying the per-format separators
    // (markdown pipes / HTML tags). Every format reconstructs by concatenation.
    case 'table':
      return linearizeTable(node, nodeId, path, options, collector);
    // Stray rows/cells (outside a table) recurse into their children so they
    // never fall through to the 'unsupported' boundary.
    case 'table_row':
    case 'table_cell':
      return linearizeChildren(node as SupramarkParentNode, path, options, listCtx, collector);
    // Containers whose content is NOT rendered as readable text — host cards,
    // built-in html/map cards — keep a hard boundary plus a trailing break.
    // Everything else (admonitions, unrecognized containers) renders its
    // children through renderNode, so the body linearizes and its blocks
    // collect like any other content.
    case 'container': {
      const container = node;
      if (options.opaqueContainers?.has(container.name) === true) {
        return [
          containerBoundary(container, nodeId),
          breakUnit(makeUnitId(nodeId, 1), nodeId, 'block', container),
        ];
      }
      // The visible title is body text: its own unit, bound to the container
      // node so the title's rendered <Text> can register as a block. The
      // children keep their own nodes and bindings.
      const title = options.containerTitle?.(container)?.trim();
      const titleUnit =
        title !== undefined && title !== ''
          ? {
              kind: 'text' as const,
              unitId: makeUnitId(nodeId, 0),
              nodeId,
              text: title,
              node: container,
              sourceRange: getSourceRange(container),
            }
          : undefined;
      const units = [
        ...(titleUnit !== undefined ? [titleUnit] : []),
        ...linearizeChildren(container, path, options, undefined, collector),
        breakUnit(makeUnitId(nodeId, 1), nodeId, 'block', container),
      ];
      if (titleUnit !== undefined && collector !== undefined) {
        collector.blocks.push({ node: container, nodeId, unitIds: [titleUnit.unitId] });
      }
      return units;
    }
    case 'input':
      return [
        { kind: 'boundary', unitId: makeUnitId(nodeId, 0), nodeId, node: node, reason: 'input' },
        breakUnit(makeUnitId(nodeId, 1), nodeId, 'block', node),
      ];
    default:
      return [
        { kind: 'boundary', unitId: makeUnitId(nodeId, 0), nodeId, node, reason: 'unsupported' },
        breakUnit(makeUnitId(nodeId, 1), nodeId, 'block', node),
      ];
  }
}

function linearizeChildren(
  node: SupramarkParentNode,
  path: readonly SelectionPathSegment[],
  options: LinearizeSelectionOptions,
  listCtx?: ListContext,
  collector?: BlockCollector
): SelectionUnit[] {
  return node.children.flatMap((child, index) =>
    linearizeNode(child, [...path, 'children', index], options, listCtx, collector)
  );
}

type InlineWrapNode =
  | SupramarkStrongNode
  | SupramarkEmphasisNode
  | SupramarkDeleteNode
  | SupramarkLinkNode;

function wrapInline(
  node: InlineWrapNode,
  nodeId: SelectionNodeId,
  path: readonly SelectionPathSegment[],
  options: LinearizeSelectionOptions,
  listCtx: ListContext | undefined,
  collector: BlockCollector | undefined,
  open: string,
  close: string
): SelectionUnit[] {
  return [
    syntaxUnit(makeUnitId(nodeId, 0), nodeId, node, open),
    ...linearizeChildren(node, path, options, listCtx, collector),
    syntaxUnit(makeUnitId(nodeId, 1), nodeId, node, close),
  ];
}

function linearizeList(
  list: SupramarkListNode,
  path: readonly SelectionPathSegment[],
  options: LinearizeSelectionOptions,
  parentListCtx?: ListContext,
  collector?: BlockCollector
): SelectionUnit[] {
  const depth = (parentListCtx?.depth ?? 0) + 1;
  const start = list.start ?? 1;
  // Ordered numbering counts list_item children, not the raw child index.
  let itemIndex = 0;
  return list.children.flatMap((child, index) => {
    const childListCtx: ListContext | undefined =
      child.type === 'list_item'
        ? { depth, ordered: list.ordered, start, index: itemIndex++ }
        : parentListCtx;
    return linearizeNode(child, [...path, 'children', index], options, childListCtx, collector);
  });
}

function linearizeListItem(
  item: SupramarkListItemNode,
  path: readonly SelectionPathSegment[],
  options: LinearizeSelectionOptions,
  listCtx?: ListContext,
  collector?: BlockCollector
): SelectionUnit[] {
  const nodeId = resolveNodeId(item, path, options);
  const depth = Math.max(0, (listCtx?.depth ?? 1) - 1);
  const indent = '  '.repeat(depth);
  const marker = listCtx?.ordered
    ? `${(listCtx.start ?? 1) + (listCtx.index ?? 0)}. `
    : item.checked === true
      ? '- [x] '
      : item.checked === false
        ? '- [ ] '
        : '- ';

  const units = [
    syntaxUnit(makeUnitId(nodeId, 0), nodeId, item, indent + marker),
    ...linearizeChildren(item, path, options, listCtx, collector),
    breakUnit(makeUnitId(nodeId, 1), nodeId, 'list-item', item),
  ];
  // Only a tight item (inline-only children) renders as one selectable text
  // block. A loose item's text belongs to its child paragraphs, which are
  // collected on their own.
  if (item.children.every(isInlineNode)) {
    collectBlock(collector, item, nodeId, units);
  }
  return units;
}

function linearizeCodeBlock(code: SupramarkCodeNode, nodeId: SelectionNodeId): SelectionUnit[] {
  const fence = '```';
  const info = code.lang ? code.lang + (code.meta ? ` ${code.meta}` : '') : '';
  const markdown = `${fence}${info}\n${code.value}\n${fence}`;
  // The canonical double-representation example: `text` is the raw code, while
  // the payload carries the fenced markdown/source.
  return [
    {
      kind: 'text',
      unitId: makeUnitId(nodeId, 0),
      nodeId,
      text: code.value,
      node: code,
      payload: {
        plainText: code.value,
        markdown,
        source: markdown,
        metadata: { lang: code.lang, meta: code.meta },
      },
      sourceRange: getSourceRange(code),
    },
    breakUnit(makeUnitId(nodeId, 1), nodeId, 'block', code),
  ];
}

function mathAtom(
  node: SupramarkMathInlineNode | SupramarkMathBlockNode,
  nodeId: SelectionNodeId,
  displayMode: boolean
): SelectionAtomUnit {
  const value = node.value;
  return {
    kind: 'atom',
    unitId: makeUnitId(nodeId, 0),
    nodeId,
    label: displayMode ? 'math_block' : 'math_inline',
    node,
    payload: {
      plainText: value,
      markdown: displayMode ? `$$\n${value}\n$$` : `$${value}$`,
      source: value,
      metadata: { displayMode },
    },
  };
}

function diagramAtom(node: SupramarkDiagramNode, nodeId: SelectionNodeId): SelectionAtomUnit {
  const markdown = `\`\`\`${node.engine}\n${node.code}\n\`\`\``;
  return {
    kind: 'atom',
    unitId: makeUnitId(nodeId, 0),
    nodeId,
    label: `diagram:${node.engine}`,
    node,
    payload: {
      plainText: node.code,
      markdown,
      source: node.code,
      metadata: { engine: node.engine, meta: node.meta },
    },
  };
}

function linearizeTable(
  table: SupramarkTableNode,
  nodeId: SelectionNodeId,
  path: readonly SelectionPathSegment[],
  options: LinearizeSelectionOptions,
  collector?: BlockCollector
): SelectionUnit[] {
  const units: SelectionUnit[] = [];
  const rows: Array<[SupramarkTableRowNode, number]> = [];
  table.children.forEach((child, i) => {
    if (child.type === 'table_row') rows.push([child, i]);
  });
  // Leading 0 arg keeps an empty table at columnCount 0 (avoids -Infinity).
  const columnCount = Math.max(table.align?.length ?? 0, ...rows.map(([r]) => r.children.length));
  let tseq = 0;
  const tuid = (): SelectionNodeId => makeUnitId(nodeId, tseq++);
  // Every structural unit of this table shares `nodeId` as its group id so a
  // partial selection degrades all of them together (see `richTextUnit`).
  const group = nodeId;
  units.push(richTextUnit(tuid(), nodeId, table, '', { html: '<table>\n' }, group));
  rows.forEach(([row, rowChildIdx], rowIndex) => {
    const rowPath = [...path, 'children', rowChildIdx];
    const rowNodeId = resolveNodeId(row, rowPath, options);
    const isHeader =
      rowIndex === 0 ||
      (row.children.length > 0 &&
        row.children.every(c => (c as SupramarkTableCellNode).header === true));
    const tag = isHeader ? 'th' : 'td';
    let rseq = 0;
    const ruid = (): SelectionNodeId => makeUnitId(rowNodeId, rseq++);
    units.push(
      richTextUnit(
        ruid(),
        rowNodeId,
        row,
        '',
        {
          markdown: '| ',
          source: '| ',
          html: `<tr><${tag}>`,
        },
        group
      )
    );
    let cellCount = 0;
    row.children.forEach((cell, cellIndex) => {
      if (cell.type !== 'table_cell') return;
      if (cellCount > 0) {
        units.push(
          richTextUnit(
            ruid(),
            rowNodeId,
            row,
            '\t',
            {
              markdown: ' | ',
              source: ' | ',
              html: `</${tag}><${tag}>`,
            },
            group
          )
        );
      }
      cellCount++;
      const cellPath = [...rowPath, 'children', cellIndex];
      // Each cell renders as one text block; collect it under its own id.
      const cellNodeId = resolveNodeId(cell, cellPath, options);
      const cellUnits = linearizeChildren(
        cell as SupramarkParentNode,
        cellPath,
        options,
        undefined,
        collector
      );
      collectBlock(collector, cell, cellNodeId, cellUnits);
      units.push(...cellUnits);
    });
    units.push(
      richTextUnit(
        ruid(),
        rowNodeId,
        row,
        '',
        {
          markdown: ' |',
          source: ' |',
          html: `</${tag}></tr>`,
        },
        group
      )
    );
    units.push(breakUnit(ruid(), rowNodeId, 'table-row', row));
    // GFM has exactly one header row and one alignment row directly under it, so
    // emit the delimiter after the first row only. `isHeader` still drives the
    // `th`/`td` tag; using it here too would emit a second delimiter row for any
    // AST that marks more than one row all-header.
    if (rowIndex === 0) {
      const alignMd = buildAlignmentRow(table.align, columnCount) + '\n';
      units.push(
        richTextUnit(tuid(), nodeId, table, '', { markdown: alignMd, source: alignMd }, group)
      );
    }
  });
  units.push(richTextUnit(tuid(), nodeId, table, '', { html: '</table>' }, group));
  units.push(breakUnit(tuid(), nodeId, 'block', table));
  return units;
}

function buildAlignmentRow(align: SupramarkTableNode['align'], columns: number): string {
  const cells: string[] = [];
  for (let i = 0; i < columns; i++) {
    const a = align?.[i] ?? null;
    cells.push(a === 'left' ? ':---' : a === 'right' ? '---:' : a === 'center' ? ':---:' : '---');
  }
  return `| ${cells.join(' | ')} |`;
}

function containerBoundary(
  node: SupramarkContainerNode,
  nodeId: SelectionNodeId
): SelectionBoundaryUnit {
  return {
    kind: 'boundary',
    unitId: makeUnitId(nodeId, 0),
    nodeId,
    node,
    reason: 'container',
  };
}

/**
 * Empty-text syntax unit: invisible to `plainText`, only injects the
 * markdown/source syntax (heading marks, emphasis marks, list markers, ...).
 */
function syntaxUnit(
  unitId: SelectionNodeId,
  nodeId: SelectionNodeId,
  node: SupramarkNode,
  markdown: string
): SelectionTextUnit {
  return {
    kind: 'text',
    unitId,
    nodeId,
    text: '',
    node,
    payload: { markdown, source: markdown },
  };
}

/**
 * Rich text unit: carries both visible `text` and a per-format `payload`.
 * Used for table structural units where `text` is the plain-text separator
 * (e.g. `\t`/`''`) while the payload supplies the markdown pipes / HTML tags.
 *
 * `structuralGroup` ties every scaffolding unit of one table together so
 * `resolve.ts` can strip the syntax payload (leaving the plain `text`, already
 * valid TSV) when a selection covers only part of the table — otherwise a
 * partial slice would leak unbalanced pipes / tags into markdown / HTML.
 */
function richTextUnit(
  unitId: SelectionNodeId,
  nodeId: SelectionNodeId,
  node: SupramarkNode,
  text: string,
  payload: SelectionPayload,
  structuralGroup: SelectionNodeId
): SelectionTextUnit {
  return { kind: 'text', unitId, nodeId, text, node, payload, structuralGroup };
}

/**
 * Blockquote per-line prefixing: emit each inner unit verbatim, and after every
 * interior break that still has quoted content ahead of it insert a `> ` syntax
 * unit so the following line is quoted too. A `> ` only makes sense as the
 * prefix of a following line, so trailing breaks (a nested quote's own block
 * break, or any run of breaks after the last content unit) get none — otherwise
 * a pure-nested quote emits a dangling empty `> ` line. unitIds 0/1 are reserved
 * for the leading prefix and trailing block break, so inserted prefixes start at 2.
 */
function prefixBlockquoteInterior(
  inner: SelectionUnit[],
  nodeId: SelectionNodeId,
  node: SupramarkBlockquoteNode
): SelectionUnit[] {
  let lastContentIndex = -1;
  for (let j = inner.length - 1; j >= 0; j--) {
    if (inner[j].kind !== 'break') {
      lastContentIndex = j;
      break;
    }
  }
  const out: SelectionUnit[] = [];
  let seq = 2;
  inner.forEach((unit, i) => {
    out.push(unit);
    if (unit.kind === 'break' && i < lastContentIndex) {
      out.push(syntaxUnit(makeUnitId(nodeId, seq++), nodeId, node, '> '));
    }
  });
  return out;
}

/**
 * A source view rendered as readable text (a feature disabled in the renderer):
 * the visible `text` is what the renderer draws, while the Markdown payload
 * rebuilds the original syntax. Distinct from an atom, which carries no
 * on-screen text.
 */
function renderedSourceUnit(
  nodeId: SelectionNodeId,
  node: SupramarkNode,
  text: string,
  markdown: string
): SelectionTextUnit {
  return {
    kind: 'text',
    unitId: makeUnitId(nodeId, 0),
    nodeId,
    text,
    node,
    payload: { markdown, source: text },
    sourceRange: getSourceRange(node),
  };
}

/**
 * Plain-text content unit: `text` is the visible text and there is no payload,
 * so the serializer falls back to `text` for markdown/html/source.
 */
function textUnit(
  unitId: SelectionNodeId,
  nodeId: SelectionNodeId,
  node: SupramarkNode,
  text: string,
  sourceRange?: SelectionSourceRange
): SelectionTextUnit {
  return {
    kind: 'text',
    unitId,
    nodeId,
    text,
    node,
    sourceRange,
  };
}

function breakUnit(
  unitId: SelectionNodeId,
  nodeId: SelectionNodeId,
  reason: SelectionBreakUnit['reason'],
  node?: SupramarkNode
): SelectionBreakUnit {
  return { kind: 'break', unitId, nodeId, text: '\n', reason, node };
}

/**
 * Build a globally unique unit id. `localIdx` is the 0-based position of the
 * unit among the units that share `nodeId`, in emission order (content unit
 * first, then any prefix/marker/break units).
 */
function makeUnitId(nodeId: SelectionNodeId, localIdx: number): SelectionNodeId {
  return `${nodeId}#${localIdx}`;
}

/**
 * Default identity: the node's type plus its AST path. Unique for every node in
 * a document and stable across re-renders of the same content.
 *
 * Source positions cannot serve as identity: the canonical parser reports the
 * same range for a paragraph and its only text child, so position-based ids
 * collide between a node and its descendant and their units end up sharing a
 * unitId. Positions remain on each unit's `sourceRange` as metadata only.
 */
function resolveNodeId(
  node: SupramarkNode,
  path: readonly SelectionPathSegment[],
  options: LinearizeSelectionOptions
): string {
  if (options.nodeId) return options.nodeId(node, path);
  return `${node.type}@${path.join('/')}`;
}

function getSourceRange(node: SupramarkNode): SelectionSourceRange | undefined {
  const position = node.position;
  if (!position) return undefined;
  return {
    startUtf16: position.start.utf16_offset,
    endUtf16: position.end.utf16_offset,
    startByte: position.start.byte_offset,
    endByte: position.end.byte_offset,
  };
}
