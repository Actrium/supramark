import { describe, expect, test } from 'bun:test';
import type {
  SupramarkBlockquoteNode,
  SupramarkCodeNode,
  SupramarkContainerNode,
  SupramarkHeadingNode,
  SupramarkImageNode,
  SupramarkListItemNode,
  SupramarkListNode,
  SupramarkNode,
  SupramarkParagraphNode,
  SupramarkStrongNode,
  SupramarkTableCellNode,
  SupramarkTableNode,
  SupramarkTableRowNode,
  SupramarkTextNode,
} from '@supramark/core';
import { linearizeDocument, linearizeForSelection } from '../linearize';
import type { SelectionUnit } from '../model';

// AST literals only — no runtime import from @supramark/core.
type N = SupramarkNode;
const text = (value: string): SupramarkTextNode => ({ type: 'text', value }) as SupramarkTextNode;
const strong = (...children: N[]): SupramarkStrongNode =>
  ({ type: 'strong', children }) as SupramarkStrongNode;
const paragraph = (...children: N[]): SupramarkParagraphNode =>
  ({ type: 'paragraph', children }) as SupramarkParagraphNode;
const heading = (depth: 1 | 2 | 3 | 4 | 5 | 6, ...children: N[]): SupramarkHeadingNode =>
  ({ type: 'heading', depth, children }) as SupramarkHeadingNode;
const listItem = (checked: boolean | undefined, ...children: N[]): SupramarkListItemNode =>
  ({ type: 'list_item', checked, children }) as SupramarkListItemNode;
const list = (ordered: boolean, start: number | undefined, ...items: N[]): SupramarkListNode =>
  ({ type: 'list', ordered, start, children: items }) as SupramarkListNode;
const blockquote = (...children: N[]): SupramarkBlockquoteNode =>
  ({ type: 'blockquote', children }) as SupramarkBlockquoteNode;
const image = (url: string, alt: string): SupramarkImageNode =>
  ({ type: 'image', url, alt }) as SupramarkImageNode;
const code = (value: string, lang?: string): SupramarkCodeNode =>
  ({ type: 'code', value, lang }) as SupramarkCodeNode;
const tableCell = (...children: N[]): SupramarkTableCellNode =>
  ({ type: 'table_cell', children }) as SupramarkTableCellNode;
const tableRow = (...cells: N[]): SupramarkTableRowNode =>
  ({ type: 'table_row', children: cells }) as SupramarkTableRowNode;
const table = (...rows: N[]): SupramarkTableNode =>
  ({ type: 'table', children: rows }) as SupramarkTableNode;

/** The text a block's units contribute, i.e. what SelectableBlock must render. */
function textOf(units: readonly SelectionUnit[], unitIds: readonly string[]): string {
  const byId = new Map(units.map(unit => [unit.unitId, unit]));
  return unitIds
    .map(unitId => {
      const unit = byId.get(unitId);
      if (unit === undefined || unit.kind !== 'text') {
        throw new Error(`unit ${unitId} is missing or not a text unit`);
      }
      return unit.text;
    })
    .join('');
}

describe('linearizeDocument', () => {
  test('linearizeForSelection still returns exactly the document units', () => {
    const root = [paragraph(text('Hello')), heading(2, text('Title')), code('const x = 1')];
    const { units } = linearizeDocument(root);
    expect(units).toEqual(linearizeForSelection(root));
  });

  test('collects a paragraph with only its visible text units', () => {
    const node = paragraph(text('Hello '), strong(text('world')), text('!'));
    const { units, blocks } = linearizeDocument([node]);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].node).toBe(node);
    expect(textOf(units, blocks[0].unitIds)).toBe('Hello world!');
    // The strong markers are syntax units and must not be rendered as text.
    expect(blocks[0].unitIds).toHaveLength(3);
  });

  test('collects a heading without the leading `#` syntax unit', () => {
    const node = heading(3, text('Section'));
    const { units, blocks } = linearizeDocument([node]);
    expect(blocks).toHaveLength(1);
    expect(textOf(units, blocks[0].unitIds)).toBe('Section');
  });

  test('collects a tight list item without its marker', () => {
    const item = listItem(undefined, text('First'));
    const { units, blocks } = linearizeDocument([list(false, undefined, item)]);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].node).toBe(item);
    expect(textOf(units, blocks[0].unitIds)).toBe('First');
  });

  test('collects task and ordered tight items with their text only', () => {
    const done = listItem(true, text('done'));
    const todo = listItem(false, text('todo'));
    const ordered = listItem(undefined, text('one'));
    const { units, blocks } = linearizeDocument([
      list(false, undefined, done, todo),
      list(true, 3, ordered),
    ]);
    expect(blocks.map(b => textOf(units, b.unitIds))).toEqual(['done', 'todo', 'one']);
  });

  test('skips a loose list item but collects its child paragraph', () => {
    const inner = paragraph(text('Loose body'));
    const item = listItem(undefined, inner);
    const { units, blocks } = linearizeDocument([list(false, undefined, item)]);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].node).toBe(inner);
    expect(textOf(units, blocks[0].unitIds)).toBe('Loose body');
  });

  test('collects a code block as its raw value', () => {
    const node = code('const x = 1;\nreturn x;', 'ts');
    const { units, blocks } = linearizeDocument([node]);
    expect(blocks).toHaveLength(1);
    expect(textOf(units, blocks[0].unitIds)).toBe('const x = 1;\nreturn x;');
  });

  test('collects table cells and leaves row separators unowned', () => {
    const c1 = tableCell(text('A'));
    const c2 = tableCell(text('B'));
    const c3 = tableCell(text('C'));
    const { units, blocks } = linearizeDocument([table(tableRow(c1, c2), tableRow(c3))]);
    expect(blocks.map(b => b.node)).toEqual([c1, c2, c3]);
    expect(blocks.map(b => textOf(units, b.unitIds))).toEqual(['A', 'B', 'C']);
    // The `\t` separators are owned by the row, so no block claims them.
    const owned = new Set(blocks.flatMap(b => b.unitIds));
    const separators = units.filter(unit => unit.kind === 'text' && unit.text === '\t');
    expect(separators.length).toBeGreaterThan(0);
    expect(separators.every(unit => !owned.has(unit.unitId))).toBe(true);
  });

  test('collects paragraphs nested inside a blockquote', () => {
    const inner = paragraph(text('Quoted'));
    const { units, blocks } = linearizeDocument([blockquote(inner)]);
    expect(blocks).toHaveLength(1);
    expect(blocks[0].node).toBe(inner);
    expect(textOf(units, blocks[0].unitIds)).toBe('Quoted');
  });

  test('skips empty blocks: blank paragraph, empty code, empty cell, image-only paragraph', () => {
    const blank = paragraph();
    const emptyCode = code('');
    const emptyCell = tableCell();
    const imageOnly = paragraph(image('https://example.com/a.png', 'a'));
    const { blocks } = linearizeDocument([
      blank,
      emptyCode,
      table(tableRow(emptyCell)),
      imageOnly,
    ]);
    expect(blocks).toHaveLength(0);
  });

  test('block ids are unique per block and shared with the unit stream', () => {
    const root = [paragraph(text('One')), paragraph(text('Two'))];
    const { units, blocks } = linearizeDocument(root);
    const ids = blocks.map(b => b.nodeId);
    expect(new Set(ids).size).toBe(ids.length);
    const unitIds = new Set(units.map(unit => unit.unitId));
    for (const block of blocks) {
      for (const unitId of block.unitIds) {
        expect(unitIds.has(unitId)).toBe(true);
      }
    }
  });

  test('custom nodeId option applies to blocks and units alike', () => {
    const root = [paragraph(text('One'))];
    const { units, blocks } = linearizeDocument(root, { nodeId: () => 'fixed' });
    expect(blocks[0].nodeId).toBe('fixed');
    expect(blocks[0].unitIds.every(unitId => unitId.startsWith('fixed#'))).toBe(true);
    expect(units[0].nodeId).toBe('fixed');
  });
});

describe('containerTitle', () => {
  const container = (params: string | undefined, ...children: N[]): SupramarkContainerNode =>
    ({ type: 'container', name: 'note', params, children }) as unknown as SupramarkContainerNode;
  // The renderer derives the visible title from `params`.
  const titleFromParams = (node: SupramarkContainerNode): string | undefined => node.params;

  test('a titled container emits one title unit bound to the container and a title-only block', () => {
    const note = container('My title', paragraph(text('body')));
    const { units, blocks } = linearizeDocument([note], { containerTitle: titleFromParams });

    // The title text appears exactly once, as a standalone unit in the
    // container's id slot #0 (its trailing block break takes #1).
    const titleUnits = units.filter(u => u.kind === 'text' && u.text === 'My title');
    expect(titleUnits).toHaveLength(1);
    const titleUnit = titleUnits[0];
    expect(titleUnit.unitId).toBe(`${titleUnit.nodeId}#0`);
    expect(units[units.length - 1].nodeId).toBe(titleUnit.nodeId);

    // The title collects as its own block; the child paragraph keeps its own.
    const titleBlock = blocks.find(b => b.node === note);
    expect(titleBlock).toBeDefined();
    expect(titleBlock?.unitIds).toEqual([titleUnit.unitId]);
    const bodyBlock = blocks.find(b => b.node === note.children[0]);
    expect(bodyBlock).toBeDefined();
    expect(textOf(units, bodyBlock!.unitIds)).toBe('body');
  });

  test('the callback result is trimmed before it becomes a unit', () => {
    const note = container('  Padded  ', paragraph(text('body')));
    const { units } = linearizeDocument([note], { containerTitle: titleFromParams });
    expect(units.filter(u => u.kind === 'text' && u.text === 'Padded')).toHaveLength(1);
    expect(units.some(u => u.kind === 'text' && u.text === '  Padded  ')).toBe(false);
  });

  test('an empty or missing title produces neither a title unit nor a title block', () => {
    for (const params of [undefined, '', '   ']) {
      const note = container(params, paragraph(text('body')));
      const { units, blocks } = linearizeDocument([note], { containerTitle: titleFromParams });
      // No block claims the container itself.
      expect(blocks.some(b => b.node === note)).toBe(false);
      // The container owns only its trailing block break.
      const containerOwned = units.filter(u => u.nodeId === units[units.length - 1].nodeId);
      expect(containerOwned).toHaveLength(1);
      expect(containerOwned[0].kind).toBe('break');
    }
  });
});