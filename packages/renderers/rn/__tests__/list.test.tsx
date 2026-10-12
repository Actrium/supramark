import { describe, expect, it } from 'bun:test';
import React from 'react';
import { create, act, type ReactTestRenderer } from 'react-test-renderer';
import type { SupramarkRootNode } from '@supramark/core';

import './support/mock-react-native';
import './support/mock-renderer';

// react-test-renderer needs the act environment to flush effects synchronously.
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT =
  true;

const { Supramark } = await import('../src/Supramark');

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

async function renderAst(ast: SupramarkRootNode): Promise<ReactTestRenderer> {
  let renderer: ReactTestRenderer | null = null;
  await act(async () => {
    renderer = create(React.createElement(Supramark, { ast }));
    // Let the async parse effect (expandOpaqueContainers / preHighlightAll) flush.
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
  return renderer as unknown as ReactTestRenderer;
}

function listAst(opts: {
  ordered: boolean;
  start?: number;
  items: Array<{ checked?: boolean; text: string }>;
}): SupramarkRootNode {
  return {
    type: 'root',
    children: [
      {
        type: 'list',
        ordered: opts.ordered,
        start: opts.start,
        tight: true,
        children: opts.items.map(item => ({
          type: 'list_item',
          checked: item.checked,
          children: [{ type: 'text', value: item.text }],
        })),
      },
    ],
  } as unknown as SupramarkRootNode;
}

describe('list rendering', () => {
  it('ordered list renders numeric markers in the same <Text> as the content', async () => {
    const r = await renderAst(
      listAst({ ordered: true, items: [{ text: 'a' }, { text: 'b' }] }),
    );
    const texts = textContents(r.root);
    // "1. a" (not "1." + "a" split across two <Text>) locks the plain-Text shape.
    expect(texts).toContain('1. a');
    expect(texts).toContain('2. b');
  });

  it('honors list.start for the first marker', async () => {
    const r = await renderAst(
      listAst({ ordered: true, start: 3, items: [{ text: 'a' }, { text: 'b' }] }),
    );
    const texts = textContents(r.root);
    expect(texts).toContain('3. a');
    expect(texts).toContain('4. b');
  });

  it('unordered list renders a bullet marker', async () => {
    const r = await renderAst(
      listAst({ ordered: false, items: [{ text: 'a' }] }),
    );
    const texts = textContents(r.root);
    expect(texts).toContain('• a');
  });

  it('task list renders checked / unchecked boxes', async () => {
    const r = await renderAst(
      listAst({ ordered: false, items: [{ checked: false, text: 'a' }, { checked: true, text: 'b' }] }),
    );
    const texts = textContents(r.root);
    expect(texts).toContain('• ☐ a');
    expect(texts).toContain('• ☑ b');
  });

  it('ordered task list preserves numeric markers before checkboxes', async () => {
    const r = await renderAst(
      listAst({
        ordered: true,
        start: 3,
        items: [
          { checked: false, text: 'a' },
          { checked: true, text: 'b' },
        ],
      }),
    );
    const texts = textContents(r.root);
    expect(texts).toContain('3. ☐ a');
    expect(texts).toContain('4. ☑ b');
  });
});

describe('loose / nested list rendering (block children)', () => {
  it('loose unordered list renders item bodies (paragraph children) with markers', async () => {
    const ast = {
      type: 'root',
      children: [
        {
          type: 'list',
          ordered: false,
          children: [
            { type: 'list_item', children: [{ type: 'paragraph', children: [{ type: 'text', value: 'a' }] }] },
            { type: 'list_item', children: [{ type: 'paragraph', children: [{ type: 'text', value: 'b' }] }] },
          ],
        },
      ],
    } as unknown as SupramarkRootNode;
    const r = await renderAst(ast);
    const texts = textContents(r.root);
    expect(texts).toContain('• a');
    expect(texts).toContain('• b');
  });

  it('loose ordered list renders numbered markers with bodies', async () => {
    const ast = {
      type: 'root',
      children: [
        {
          type: 'list',
          ordered: true,
          start: 1,
          children: [
            { type: 'list_item', children: [{ type: 'paragraph', children: [{ type: 'text', value: 'a' }] }] },
            { type: 'list_item', children: [{ type: 'paragraph', children: [{ type: 'text', value: 'b' }] }] },
          ],
        },
      ],
    } as unknown as SupramarkRootNode;
    const r = await renderAst(ast);
    const texts = textContents(r.root);
    expect(texts).toContain('1. a');
    expect(texts).toContain('2. b');
  });

  it('nested list renders the sub-list instead of dropping it', async () => {
    const ast = {
      type: 'root',
      children: [
        {
          type: 'list',
          ordered: false,
          children: [
            {
              type: 'list_item',
              children: [
                { type: 'text', value: 'outer' },
                {
                  type: 'list',
                  ordered: false,
                  children: [{ type: 'list_item', children: [{ type: 'text', value: 'inner' }] }],
                },
              ],
            },
          ],
        },
      ],
    } as unknown as SupramarkRootNode;
    const r = await renderAst(ast);
    const texts = textContents(r.root);
    expect(texts).toContain('• outer');
    expect(texts).toContain('• inner');
  });

  it('loose nested list: paragraph body + nested sub-list both render', async () => {
    const ast = {
      type: 'root',
      children: [
        {
          type: 'list',
          ordered: false,
          children: [
            {
              type: 'list_item',
              children: [
                { type: 'paragraph', children: [{ type: 'text', value: 'outer' }] },
                {
                  type: 'list',
                  ordered: false,
                  children: [{ type: 'list_item', children: [{ type: 'text', value: 'inner' }] }],
                },
              ],
            },
          ],
        },
      ],
    } as unknown as SupramarkRootNode;
    const r = await renderAst(ast);
    const texts = textContents(r.root);
    expect(texts).toContain('• outer');
    expect(texts).toContain('• inner');
  });
});
