import { describe, expect, it } from 'bun:test';
import React from 'react';
import { create, act, type ReactTestRenderer } from 'react-test-renderer';
import type { SupramarkNode, SupramarkRootNode } from '@supramark/core';

import { openUrlMock } from './support/mock-react-native';
import './support/mock-renderer';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT =
  true;

const { Supramark } = await import('../src/Supramark');
const { rendererContainerTitle, resolveOpaqueContainers } = await import('../src/selection');
const { SelectableBlock, SelectionRoot, linearizeForSelection, resolveSelectionRange, serializeSelectionUnits } =
  await import('@supramark/rn-selection');

/**
 * Linearize exactly as the component's plan does: same container policy and
 * title mapping, same feature switches (all enabled — no config is passed).
 * Sharing the exported helpers keeps the test-side units identical to the ones
 * the mounted blocks were bound against.
 */
function unitsFor(ast: SupramarkRootNode) {
  return linearizeForSelection(ast, {
    opaqueContainers: resolveOpaqueContainers(undefined, undefined),
    containerTitle: rendererContainerTitle,
    rendersAsText: () => false,
  });
}

/**
 * The real canonical parser, loaded by file path on purpose: bun's module mocks
 * are process-wide and SupramarkCache.test.tsx stubs the
 * `@supramark/markdown-web` specifier for the whole run, so a specifier import
 * here would return that stub. Going through the dist entry bypasses it, which
 * keeps these cases on the canonical parser — the same one production uses —
 * regardless of test-file order. The node entry (not the browser one) is what
 * core's loader actually picks under bun; it instantiates its own wasm build,
 * whereas the browser entry expects a bundler to hand it an instance. (The stub
 * leaking across files is a pre-existing test-infrastructure issue, recorded
 * separately.)
 */
const realParser = (await import('../../../../crates/supramark-markdown/packages/web/dist/node.js')) as {
  parse: (markdown: string) => Promise<SupramarkRootNode>;
};

/**
 * Mirror `expandOpaqueContainers` with the real parser: containers parsed by
 * the canonical parser carry their body in `value` with empty children, and the
 * renderer fills children in before rendering. Doing that here keeps the AST
 * real without routing through the stubbed specifier.
 */
async function expandContainers(node: SupramarkNode): Promise<void> {
  const children = (node as { children?: SupramarkNode[] }).children;
  if (!Array.isArray(children)) return;
  for (const child of children) {
    const container = child as {
      type?: string;
      mode?: string;
      data?: unknown;
      value?: string;
      children?: SupramarkNode[];
    };
    if (
      container.type === 'container' &&
      container.mode === 'opaque' &&
      container.data == null &&
      typeof container.value === 'string' &&
      container.value.length > 0
    ) {
      const sub = await realParser.parse(container.value);
      container.children = sub.children;
      container.value = undefined;
    }
    await expandContainers(child);
  }
}

async function parseMarkdown(markdown: string): Promise<SupramarkRootNode> {
  const ast = await realParser.parse(markdown);
  await expandContainers(ast);
  return ast;
}

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

/** Render a real-parser AST — the component's own pipeline, minus the stub. */
async function renderMarkdown(markdown: string): Promise<ReactTestRenderer> {
  const ast = await parseMarkdown(markdown);
  let renderer: ReactTestRenderer | null = null;
  await act(async () => {
    renderer = create(React.createElement(Supramark, { ast, selection: {} }));
    await Promise.resolve();
  });
  return renderer as unknown as ReactTestRenderer;
}

/**
 * The SelectableBlock contract, against real parser output: every mounted
 * block's rendered text equals the concatenation of its units' text.
 */
async function expectContract(markdown: string): Promise<ReactTestRenderer> {
  const r = await renderMarkdown(markdown);
  const units = unitsFor(await parseMarkdown(markdown));
  const byId = new Map(units.map(unit => [unit.unitId, unit]));
  for (const inst of r.root.findAllByType(SelectableBlock)) {
    const unitIds = inst.props.unitIds as string[];
    const expected = unitIds
      .map(unitId => {
        const unit = byId.get(unitId);
        expect(unit).toBeDefined();
        return unit !== undefined && unit.kind === 'text' ? unit.text : '';
      })
      .join('');
    expect(flattenText(inst.props.children)).toBe(expected);
  }
  return r;
}

// Chinese sample built from code points: lint:cjk bans CJK literals in source
// (spells the four characters meaning "hello world").
const CJK_SAMPLE = String.fromCodePoint(0x4f60, 0x597d, 0x4e16, 0x754c);

describe('real Markdown through the renderer: selection contract', () => {
  const CASES: Array<[string, string]> = [
    ['inline mix (bold/emphasis/strike/inline code/link)', 'a **b** _c_ ~~d~~ `e` [f](https://example.com)'],
    ['heading', '## Title with **bold**'],
    ['tight list', '- one\n- two'],
    ['task list', '- [x] done\n- [ ] todo'],
    ['blockquote', '> quoted **text**'],
    ['code fence', '```ts\nconst x = 1;\n```'],
    ['table', '| a | b |\n| --- | --- |\n| 1 | 2 |'],
    ['footnote', 'see[^1]\n\n[^1]: note body'],
    ['cjk and emoji', CJK_SAMPLE + ' \u{1F31F} emoji'],
    ['inline html', 'a <span>b</span> c'],
    ['admonition container', ':::note\nbody text\n:::'],
  ];

  for (const [name, markdown] of CASES) {
    it(name, async () => {
      const r = await expectContract(markdown);
      expect(r.root.findAllByType(SelectableBlock).length).toBeGreaterThan(0);
    });
  }
});

describe('real Markdown: per-class selection semantics', () => {
  it('a footnote reference shows "[1]" on screen and hides the label syntax', async () => {
    const r = await renderMarkdown('see[^note]\n\n[^note]: body');
    const texts = textContents(r.root);
    expect(texts.some(text => text.includes('[1]'))).toBe(true);
    expect(texts.some(text => text.includes('[^note]'))).toBe(false);
  });

  it('inline HTML occupies no screen text and stays out of copies', async () => {
    const r = await expectContract('a <span>b</span> c');
    for (const inst of r.root.findAllByType(SelectableBlock)) {
      expect(flattenText(inst.props.children)).not.toContain('<span>');
    }
    const units = unitsFor(await parseMarkdown('a <span>b</span> c'));
    expect(serializeSelectionUnits(units, 'plainText')).not.toContain('<span>');
    expect(serializeSelectionUnits(units, 'source')).toContain('<span>');
  });

  it('list markers stay outside the selectable text', async () => {
    const r = await expectContract('- one\n- two');
    const texts = textContents(r.root);
    expect(texts).toContain('• ');
    expect(texts).not.toContain('• one');
  });

  it('admonition body paragraphs and its visible title are both selectable', async () => {
    const r = await expectContract(':::note My Title\nbody text\n:::');
    const blockTexts = r.root
      .findAllByType(SelectableBlock)
      .map(inst => flattenText(inst.props.children));
    expect(blockTexts).toContain('body text');
    expect(blockTexts.some(text => text.includes('My Title'))).toBe(true);
  });

  it('definition terms and descriptions are selectable', async () => {
    const r = await expectContract('Term\n: Description text');
    const blockTexts = r.root
      .findAllByType(SelectableBlock)
      .map(inst => flattenText(inst.props.children));
    expect(blockTexts).toContain('Term');
    expect(blockTexts).toContain('Description text');
  });

  it('a diagram is a hard boundary: selection from before it never leaks the source or crosses', async () => {
    const markdown = 'before\n\n```mermaid\ngraph TD;\n```\n\nafter';
    const units = unitsFor(await parseMarkdown(markdown));
    const first = units.find(unit => unit.kind === 'text' && unit.text.includes('before'));
    const last = units.find(unit => unit.kind === 'text' && unit.text.includes('after'));
    expect(first).toBeDefined();
    expect(last).toBeDefined();
    const resolved = resolveSelectionRange(units, {
      anchor: { nodeId: first!.nodeId, unitId: first!.unitId, offset: 0 },
      focus: { nodeId: last!.nodeId, unitId: last!.unitId, offset: 5 },
    });
    const plain = String(serializeSelectionUnits(resolved, 'plainText'));
    expect(plain).toContain('before');
    // Stopped at the boundary: neither the diagram source nor the text beyond.
    expect(plain).not.toContain('graph TD');
    expect(plain).not.toContain('after');
  });

  it('a paragraph hosting an unloadable inline image stays selectable and degrades to the whole block', async () => {
    const r = await renderMarkdown('a ![alt](relative/path.png) b');
    // The fallback label may diverge from the units; the length check then
    // drops the block's metrics and selection degrades to the whole block.
    // Either way the block registers, or a host-driven paragraph entry point
    // would snap onto a neighbouring block instead.
    expect(r.root.findAllByType(SelectableBlock)).toHaveLength(1);
    expect(textContents(r.root).some(text => text.includes('alt'))).toBe(true);
  });

  it('a nested unloadable image keeps the block selectable, matching the direct-child case', async () => {
    const r = await renderMarkdown('a [![alt](relative/path.png)](https://example.com) b');
    // `link > image` renders the same fallback label as a direct child; the
    // block registers and the length check drops its metrics, so both shapes
    // behave identically instead of one silently diverging.
    expect(r.root.findAllByType(SelectableBlock)).toHaveLength(1);
    expect(textContents(r.root).some(text => text.includes('alt'))).toBe(true);
  });

  it('the AST path mounts the selection layer', async () => {
    const r = await renderMarkdown('hello world');
    expect(r.root.findAllByType(SelectionRoot)).toHaveLength(1);
  });
});
for (const os of ['macos', 'windows']) {
  it(`${os} serializes a drag through rendered Markdown across two paragraphs`, async () => {
    const { Platform } = await import('react-native');
    const previous = Platform.OS;
    const payloads: string[] = [];
    const handle: { current: { getSelectedText(): string | null } | null } = { current: null };
    let renderer: ReactTestRenderer | undefined;
    try {
      Object.assign(Platform, { OS: os });
      const markdown = 'ii**WW**\n\nhello';
      const ast = await parseMarkdown(markdown);
      await act(async () => {
        renderer = create(
          <Supramark
            ast={ast}
            markdown={markdown}
            selection={{
              toolbarItems: [{ id: 'inspect', title: 'Show selection', format: 'plainText' }],
              onHandleReady: ready => {
                handle.current = ready;
              },
              onAction: request => payloads.push(request.text),
            }}
          />,
          {
            createNodeMock: () => ({
              measureInWindow: (callback: (x: number, y: number) => void) => callback(0, 0),
            }),
          }
        );
      });
      const root = renderer!.root.findByType(SelectionRoot).findAllByType('View')[0];
      act(() => {
        root.props.onLayout({ nativeEvent: { layout: { width: 200, height: 100 } } });
        renderer!.root.findAllByType(SelectableBlock).forEach((block, i) => {
          block.findAllByType('View')[0].props.onLayout({
            nativeEvent: { layout: { x: 0, y: i * 30, width: 100, height: 20 } },
          });
          const text = block.findAllByType('Text').find(node => node.props.onTextLayout)!;
          text.props.onTextLayout({ nativeEvent: { target: i + 1, lines: [{
            text: i ? 'hello' : 'iiWW', x: 0, y: 0, width: 40, height: 20,
            charXs: i ? [0, 8, 16, 24, 32, 40] : [0, 4, 8, 24, 40],
          }] } });
        });
      });
      const events = os === 'macos' ? ['onTouchStart', 'onTouchMove', 'onTouchEnd']
        : ['onPointerDown', 'onPointerMove', 'onPointerUp'];
      events.forEach((name, i) =>
        act(() =>
          root.props[name]({
            nativeEvent: {
              pageX: i ? 16 : 4,
              pageY: i ? 38 : 8,
              identifier: 1,
              target: 9,
              locationX: i ? 16 : 4,
              locationY: i ? 38 : 8,
              pointerId: 1,
              pointerType: 'mouse',
              button: 0,
              buttons: i === 2 ? 0 : 1,
            },
          })
        )
      );
      const action = renderer!.root.findByType('TouchableOpacity');
      expect(handle.current?.getSelectedText()).toBe('iWW\nhe');
      expect(handle.current?.getSelectedText()).toBe('iWW\nhe');
      act(() => action.props.onPress());
      expect(payloads).toEqual(['iWW\nhe']);
      expect(handle.current?.getSelectedText()).toBeNull();
    } finally {
      if (renderer) act(() => renderer?.unmount());
      Object.assign(Platform, { OS: previous });
    }
  });
}

type ReadableSelection = { getSelectedText(): string | null };
const zeroOriginView = () => ({
  measureInWindow: (callback: (x: number, y: number) => void) => callback(0, 0),
});

async function renderSelectableLink(
  handle: { current: ReadableSelection | null },
  line: { text: string; width: number; charXs?: number[]; charEndXs?: number[] } = { text: 'hello', width: 50 }
) {
  const markdown = `[${line.text}](https://example.com)`;
  const ast = await parseMarkdown(markdown);
  let renderer!: ReactTestRenderer;
  await act(async () => {
    renderer = create(
      <Supramark ast={ast} markdown={markdown} selection={{ onHandleReady: ready => { handle.current = ready; } }} />,
      { createNodeMock: zeroOriginView }
    );
  });
  const block = renderer.root.findByType(SelectableBlock);
  const layout = { x: 0, y: 0, width: line.width, height: 20 };
  act(() => {
    block.findAllByType('View')[0].props.onLayout({ nativeEvent: { layout } });
    const text = block.findAllByType('Text').find(n => n.props.onTextLayout)!;
    text.props.onTextLayout({ nativeEvent: { target: 9, lines: [{ ...layout, ...line }] } });
  });
  return renderer;
}

function linkGesture(renderer: ReactTestRenderer, start = 0) {
  const root = renderer.root.findByType(SelectionRoot).findAllByType('View')[0];
  const link = renderer.root.findByType(SelectableBlock).findAllByType('Text').find(n => n.props.onPress)!;
  let timestamp = 0;
  const event = (x: number) => ({
    nativeEvent: { target: 9, identifier: 1, pageX: x, pageY: 36, locationX: x, locationY: 8, button: 0, timestamp: ++timestamp },
  });
  return (moves: number[], end: number, pressFirst: boolean) => {
    act(() => root.props.onTouchStart(event(start)));
    for (const x of moves) act(() => root.props.onTouchMove(event(x)));
    const release = event(end);
    act(() => {
      if (pressFirst) link.props.onPress(release);
      root.props.onTouchEnd(release);
      if (!pressFirst) link.props.onPress(release);
    });
  };
}

it('macOS opens a link on click but suppresses the press associated with a text drag', async () => {
  const { Platform } = await import('react-native');
  const previous = Platform.OS;
  Object.assign(Platform, { OS: 'macos' });
  const handle: { current: ReadableSelection | null } = { current: null };
  let renderer: ReactTestRenderer | undefined;
  openUrlMock.mockClear();
  try {
    renderer = await renderSelectableLink(handle);
    const gesture = linkGesture(renderer);
    gesture([], 0, true);
    expect(openUrlMock).toHaveBeenCalledTimes(1);
    gesture([20], 30, true);
    expect(handle.current?.getSelectedText()).toBe('hel');
    gesture([30, 0], 0, false);
    gesture([], 30, true);
    expect(openUrlMock).toHaveBeenCalledTimes(1);
    gesture([], 0, true);
    expect(openUrlMock).toHaveBeenCalledTimes(2);
  } finally {
    if (renderer) act(() => renderer?.unmount());
    Object.assign(Platform, { OS: previous });
  }
});

it('macOS suppresses a link drag between visually distant carets with the same bidi offset', async () => {
  const { Platform } = await import('react-native');
  const previous = Platform.OS;
  Object.assign(Platform, { OS: 'macos' });
  const handle: { current: ReadableSelection | null } = { current: null };
  let renderer: ReactTestRenderer | undefined;
  openUrlMock.mockClear();
  try {
    // The trailing edge of 'a' and the leading edge of the RTL run both have logical offset 1.
    renderer = await renderSelectableLink(handle, {
      text: 'aאבz', width: 100, charXs: [0, 90, 50, 90, 100], charEndXs: [10, 50, 10, 100],
    });
    const gesture = linkGesture(renderer, 9);
    for (const pressFirst of [true, false]) {
      gesture([89], 89, pressFirst);
      expect(handle.current?.getSelectedText()).toBeNull();
      expect(openUrlMock).toHaveBeenCalledTimes(0);
      gesture([], 89, pressFirst);
      gesture([89], 9, pressFirst);
      expect(openUrlMock).toHaveBeenCalledTimes(0);
    }
    // Two points apart crosses the visual run boundary, but is within click tolerance.
    gesture([11], 11, true);
    expect(handle.current?.getSelectedText()).toBe('אב');
    expect(openUrlMock).toHaveBeenCalledTimes(1);
    gesture([], 9, false);
    expect(openUrlMock).toHaveBeenCalledTimes(2);
  } finally {
    if (renderer) act(() => renderer?.unmount());
    Object.assign(Platform, { OS: previous });
  }
});
