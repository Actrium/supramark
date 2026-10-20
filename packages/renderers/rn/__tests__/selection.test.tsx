import { describe, expect, it } from 'bun:test';
import React from 'react';
import { create, act, type ReactTestRenderer } from 'react-test-renderer';
import type { SupramarkRootNode } from '@supramark/core';
import type { SelectionUnit } from '@supramark/rn-selection';
import type { ContainerRendererRN } from '../src/Supramark';

import './support/mock-react-native';
import './support/mock-renderer';

// react-test-renderer needs the act environment to flush effects synchronously.
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT =
  true;

const { Supramark } = await import('../src/Supramark');
const { SelectableBlock, linearizeForSelection } = await import('@supramark/rn-selection');

// Flatten a React node tree down to its concatenated string leaves.
function flattenText(node: React.ReactNode): string {
  if (node == null || typeof node === 'boolean') return '';
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(flattenText).join('');
  if (typeof node === 'object' && 'props' in node) {
    return flattenText((node as React.ReactElement).props.children);
  }
  return '';
}

// Text rendered by each <Text> host node, in document order.
function textContents(root: ReactTestRenderer['root']): string[] {
  return root.findAllByType('Text').map(inst => flattenText(inst.props.children));
}

function selectableBlocks(r: ReactTestRenderer) {
  return r.root.findAllByType(SelectableBlock);
}

async function renderAst(
  ast: SupramarkRootNode,
  props: Record<string, unknown> = {}
): Promise<ReactTestRenderer> {
  let renderer: ReactTestRenderer | null = null;
  await act(async () => {
    renderer = create(React.createElement(Supramark, { ast, ...props }));
    // Let the async parse effect (expandOpaqueContainers / preHighlightAll) flush.
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
  return renderer as unknown as ReactTestRenderer;
}

function paragraph(value: string) {
  return { type: 'paragraph', children: [{ type: 'text', value }] };
}

function docAst(): SupramarkRootNode {
  return {
    type: 'root',
    ast_version: 2,
    diagnostics: [],
    children: [
      { type: 'heading', depth: 2, children: [{ type: 'text', value: 'Title' }] },
      {
        type: 'paragraph',
        children: [
          { type: 'text', value: 'Hello ' },
          { type: 'strong', children: [{ type: 'text', value: 'world' }] },
        ],
      },
      {
        type: 'list',
        ordered: false,
        tight: true,
        children: [
          { type: 'list_item', children: [{ type: 'text', value: 'First' }] },
          { type: 'list_item', children: [{ type: 'text', value: 'Second' }] },
        ],
      },
      { type: 'code', value: 'const x = 1;', lang: 'ts' },
      {
        type: 'table',
        children: [
          {
            type: 'table_row',
            children: [
              { type: 'table_cell', header: true, children: [{ type: 'text', value: 'A' }] },
              { type: 'table_cell', header: true, children: [{ type: 'text', value: 'B' }] },
            ],
          },
        ],
      },
    ],
  } as unknown as SupramarkRootNode;
}

/** The text a block's units contribute, per the selection model. */
function unitsText(units: readonly SelectionUnit[], unitIds: readonly string[]): string {
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

describe('Supramark selection integration', () => {
  it('renders no SelectableBlock when `selection` is omitted', async () => {
    const ast = docAst();
    const r = await renderAst(ast);
    expect(selectableBlocks(r)).toHaveLength(0);
    // Legacy shape preserved: the tight item keeps marker and text in one <Text>.
    expect(textContents(r.root)).toContain('• First');
  });

  it('mounts one SelectableBlock per selectable block when `selection` is passed', async () => {
    const ast = docAst();
    const r = await renderAst(ast, { selection: {} });
    // heading + paragraph + 2 tight items + code + 2 table cells
    expect(selectableBlocks(r)).toHaveLength(7);
  });

  it('every mounted block renders exactly its units text (the SelectableBlock contract)', async () => {
    const ast = docAst();
    const r = await renderAst(ast, { selection: {} });
    const units = linearizeForSelection(ast);
    const blocks = selectableBlocks(r);
    expect(blocks.length).toBeGreaterThan(0);
    for (const inst of blocks) {
      const unitIds = inst.props.unitIds as string[];
      expect(flattenText(inst.props.children)).toBe(unitsText(units, unitIds));
    }
  });

  it('separates the list marker from the selectable text', async () => {
    const ast = docAst();
    const r = await renderAst(ast, { selection: {} });
    const texts = textContents(r.root);
    // Marker and content are now sibling <Text> nodes...
    expect(texts).toContain('• ');
    expect(texts).toContain('First');
    expect(texts).not.toContain('• First');
    // ...and the block covers only the text, without the marker.
    const itemBlocks = selectableBlocks(r).filter(inst =>
      (inst.props.nodeId as string).startsWith('list_item@')
    );
    expect(itemBlocks.length).toBeGreaterThan(0);
  });

  it('keeps list-item content shrink-wrapped (no zero-basis flex)', async () => {
    const ast = docAst();
    const r = await renderAst(ast, { selection: {} });
    const itemBlocks = selectableBlocks(r).filter(inst =>
      (inst.props.nodeId as string).startsWith('list_item@')
    );
    expect(itemBlocks.length).toBeGreaterThan(0);
    for (const inst of itemBlocks) {
      // `flex: 1` is flexGrow 1 / flexShrink 0 / flexBasis 0 in Yoga. In a
      // shrink-wrapped host the content then measures against a zero basis,
      // wraps early and reports a height the laid-out text overflows.
      expect(inst.props.containerStyle).toEqual({ flexShrink: 1 });
    }
  });

  it('skips a loose list item but keeps its child paragraph selectable', async () => {
    const ast = {
      type: 'root',
      ast_version: 2,
      diagnostics: [],
      children: [
        {
          type: 'list',
          ordered: false,
          tight: false,
          children: [
            {
              type: 'list_item',
              children: [{ type: 'paragraph', children: [{ type: 'text', value: 'Loose body' }] }],
            },
          ],
        },
      ],
    } as unknown as SupramarkRootNode;
    const r = await renderAst(ast, { selection: {} });
    const blocks = selectableBlocks(r);
    expect(blocks).toHaveLength(1);
    expect(flattenText(blocks[0].props.children)).toBe('Loose body');
  });

  it('does not mount a block for an image-only paragraph', async () => {
    const ast = {
      type: 'root',
      ast_version: 2,
      diagnostics: [],
      children: [
        paragraph('Selectable'),
        {
          type: 'paragraph',
          children: [{ type: 'image', url: 'https://example.com/a.png', alt: 'a' }],
        },
      ],
    } as unknown as SupramarkRootNode;
    const r = await renderAst(ast, { selection: {} });
    const blocks = selectableBlocks(r);
    expect(blocks).toHaveLength(1);
    expect(flattenText(blocks[0].props.children)).toBe('Selectable');
  });

  it('keeps the code block text, plain and highlighted alike', async () => {
    const ast = {
      type: 'root',
      ast_version: 2,
      diagnostics: [],
      children: [{ type: 'code', value: 'const x = 1;' }],
    } as unknown as SupramarkRootNode;
    const r = await renderAst(ast, { selection: {} });
    const blocks = selectableBlocks(r);
    expect(blocks).toHaveLength(1);
    expect(flattenText(blocks[0].props.children)).toBe('const x = 1;');
  });
});

describe('containerText decides which host containers stay readable', () => {
  const CARD_AST = {
    type: 'root',
    ast_version: 2,
    diagnostics: [],
    children: [
      {
        type: 'container',
        name: 'card',
        children: [paragraph('inside card')],
      },
    ],
  } as unknown as SupramarkRootNode;

  const containerRenderers: Record<string, ContainerRendererRN> = {
    card: ({ node, key, renderChildren }) =>
      React.createElement('View', { key }, renderChildren(node.children)),
  };

  it('a container declared as text keeps its children selectable', async () => {
    const r = await renderAst(CARD_AST, {
      selection: { containerText: ['card'] },
      containerRenderers,
    });
    const texts = selectableBlocks(r).map(inst => flattenText(inst.props.children));
    expect(texts).toContain('inside card');
  });

  it('an undeclared host container is opaque: its children never bind', async () => {
    const r = await renderAst(CARD_AST, { selection: {}, containerRenderers });
    // The card still draws its content, but nothing inside it is selectable.
    expect(selectableBlocks(r)).toHaveLength(0);
    expect(textContents(r.root)).toContain('inside card');
  });

  it('resolveOpaqueContainers derives the opaque set from renderers and declarations', async () => {
    const { resolveOpaqueContainers } = await import('../src/selection');
    const set = resolveOpaqueContainers({ card: () => null, panel: () => null }, ['card']);
    // Built-in cards are always opaque; host renderers are opaque unless declared as text.
    expect(set.has('html')).toBe(true);
    expect(set.has('map')).toBe(true);
    expect(set.has('panel')).toBe(true);
    expect(set.has('card')).toBe(false);
    expect([...resolveOpaqueContainers(undefined, undefined)].sort()).toEqual(['html', 'map']);
  });
});