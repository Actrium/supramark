import { describe, expect, it } from 'bun:test';
import React from 'react';
import { create, act, type ReactTestRenderer } from 'react-test-renderer';
import type { SupramarkNode, SupramarkRootNode } from '@supramark/core';

import './support/mock-react-native';
import './support/mock-renderer';

// Drive the renderer with the REAL Rust parser's output — see issue #125:
// every other RN test hand-builds its AST, so parser/renderer shape mismatches
// (e.g. the `raw` node the default parser emits but the renderer drops) are
// invisible to CI.
//
// bun's mock.module registry is process-wide and SupramarkCache.test.tsx mocks
// `@supramark/markdown-web` with a hand-built AST that would leak here, so we
// cannot call core's `parse()` (it imports that mocked specifier). Instead we
// load the wasm artifact directly by dist path — a specifier no other file
// mocks — and parse the fixture ourselves. This is the same artifact core's
// parse() drives under Node, so the AST shape is identical.
const realParser = await import(
  '../../../../crates/supramark-markdown/packages/web/dist/node.js'
);

// react-test-renderer needs the act environment to flush effects synchronously.
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT =
  true;

const { Supramark, BLOCK_NODE_TYPES, INLINE_NODE_TYPES } = await import('../src/Supramark');

// A document that exercises every node type the RN renderer switches on, plus
// the three the parser emits but the renderer currently drops (raw inline HTML,
// blockquote, thematic_break). Built once, parsed once through the real Rust
// parser — see issue #125: hand-built AST fixtures let parser/renderer shape
// mismatches slip through CI.
const FIXTURE = [
  '# Heading',
  '',
  'para with `code`, *em*, **strong**, ~~del~~, [link](https://x) and ![img](https://y)',
  '',
  '- tight a',
  '- tight b',
  '',
  '1. ordered',
  '2. ordered',
  '',
  '- [x] task',
  '',
  '- loose',
  '',
  '  para2',
  '',
  '- outer',
  '  - inner',
  '',
  '> blockquote',
  '',
  '```js',
  'code block',
  '```',
  '',
  '| a | b |',
  '|---|---|',
  '| 1 | 2 |',
  '',
  '[^1]: footnote definition',
  '',
  'term',
  ':   description',
  '',
  '---',
  '',
  'hard<br/>break',
  '',
  'inline <span>raw</span> html',
  '',
  '$$x^2$$',
].join('\n');

function flattenText(node: React.ReactNode): string {
  if (node == null || typeof node === 'boolean') return '';
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(flattenText).join('');
  if (typeof node === 'object' && 'props' in node) {
    return flattenText((node as React.ReactElement).props.children);
  }
  return '';
}

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

// Collect every node.type the real parser emits for the fixture.
function collectEmittedTypes(root: SupramarkNode, into: Set<string>): void {
  if (!root || typeof root !== 'object') return;
  if (root.type) into.add(root.type);
  const node = root as { children?: SupramarkNode[] };
  if (Array.isArray(node.children)) {
    for (const child of node.children) collectEmittedTypes(child, into);
  }
}

// Node types that have no case in renderNode / renderInlineNode but are still
// rendered — their parent's branch iterates and emits them directly. Keep in
// sync with the definition_list branch in renderNode.
const RENDERED_BY_PARENT = new Set<string>([
  'definition_item',
  'definition_term',
  'definition_description',
]);

// Emitted types the renderer currently drops (fall through to `return null`).
// The test locks this set: any NEW unhandled type the parser starts emitting
// fails CI here, and any of these that gets a rendering branch must be removed
// — so the gap stays visible rather than silently widening.
const KNOWN_UNHANDLED = new Set<string>([]);

describe('parse-smoke: real parser output drives the renderer (issue #125)', () => {
  it('parses the fixture through the real Rust parser and renders it', async () => {
    const ast = realParser.parse(FIXTURE) as SupramarkRootNode;
    const renderer = await renderAst(ast);
    // Sanity: the document rendered *something* — no parse error fallback path.
    expect(renderer.root.findAllByType('Text').length).toBeGreaterThan(0);
  });

  it('every node type the parser emits is either rendered by a switch branch, rendered by a parent, or a known unhandled gap', () => {
    const ast = realParser.parse(FIXTURE) as SupramarkRootNode;
    const emitted = new Set<string>();
    collectEmittedTypes(ast, emitted);

    const handled = new Set<string>([
      ...BLOCK_NODE_TYPES,
      ...INLINE_NODE_TYPES,
      ...RENDERED_BY_PARENT,
      'root',
    ]);
    const unhandled = [...emitted].filter(t => !handled.has(t)).sort();

    expect(unhandled).toEqual([...KNOWN_UNHANDLED].sort());
  });

  it('content of handled block types survives rendering (heading / code / list / table)', async () => {
    const ast = realParser.parse(FIXTURE) as SupramarkRootNode;
    const renderer = await renderAst(ast);
    const texts = textContents(renderer.root);
    expect(texts.some(t => t.includes('Heading'))).toBe(true);
    expect(texts.some(t => t.includes('code block'))).toBe(true);
    expect(texts.some(t => t.includes('tight a'))).toBe(true);
    expect(texts.some(t => t.includes('tight b'))).toBe(true);
    expect(texts.some(t => t.includes('ordered'))).toBe(true);
    expect(texts.some(t => t.includes('1'))).toBe(true);
    expect(texts.some(t => t.includes('2'))).toBe(true);
    // blockquote content survives (rendered inside an indented block, not dropped).
    expect(texts.some(t => t.includes('blockquote'))).toBe(true);
  });

  it('inline raw HTML in a list item stays in one <Text> line (issue #125 regression)', async () => {
    // Real parser emits [text "a ", raw "<span>", text "b", raw "</span>", text " c"].
    // The renderer must drop the tags and keep the flow in a single <Text>, not
    // split into three lines with empty indented Views (the #105 regression).
    const ast = realParser.parse('- a <span>b</span> c') as SupramarkRootNode;
    const renderer = await renderAst(ast);
    const texts = textContents(renderer.root);
    expect(texts).toEqual(['• a b c']);
  });

  it('thematic_break renders as a divider (a View, not dropped to null)', async () => {
    const ast = realParser.parse('a\n\n---\n\nb') as SupramarkRootNode;
    const renderer = await renderAst(ast);
    const texts = textContents(renderer.root);
    expect(texts.some(t => t.includes('a'))).toBe(true);
    expect(texts.some(t => t.includes('b'))).toBe(true);
    // The divider is a host View; with only text + a divider, at least one View
    // must be present (the thematic_break), proving it didn't fall through to null.
    expect(renderer.root.findAllByType('View').length).toBeGreaterThanOrEqual(1);
  });
});
