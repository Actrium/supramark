import { describe, expect, test } from 'bun:test';
import type React from 'react';
import type { SupramarkNode, SupramarkContainerNode, SupramarkRootNode } from '@supramark/core';
import { renderToStaticMarkup } from 'react-dom/server';
import { parse } from '@supramark/core';
// Direct module import: the package index also re-exports the RN runtime,
// whose react-native import the bun test runtime cannot load.
import { renderVideoContainerWeb } from '../src/runtime.web.js';

function findContainers(
  node: SupramarkNode,
  out: SupramarkContainerNode[] = []
): SupramarkContainerNode[] {
  const n = node as SupramarkNode & { children?: SupramarkNode[] };
  if (n && typeof n === 'object') {
    if (n.type === 'container') out.push(n as SupramarkContainerNode);
    for (const child of n.children ?? []) findContainers(child, out);
  }
  return out;
}

async function renderMarkdown(markdown: string): Promise<string> {
  const root: SupramarkRootNode = await parse(markdown);
  const [video] = findContainers(root);
  const element = renderVideoContainerWeb({
    node: video,
    key: 0,
    classNames: {},
    renderChildren: () => null,
  }) as React.ReactElement;
  return renderToStaticMarkup(element);
}

/** Renders a hand-built container to exercise renderer-boundary validation. */
function renderContainerData(data: Record<string, unknown>): string {
  const node = {
    type: 'container',
    name: 'video',
    mode: 'opaque',
    data,
    value: '{}',
    children: [],
  } as unknown as SupramarkContainerNode;
  const element = renderVideoContainerWeb({
    node,
    key: 0,
    classNames: {},
    renderChildren: () => null,
  }) as React.ReactElement;
  return renderToStaticMarkup(element);
}

describe('renderVideoContainerWeb', () => {
  test('renders a native <video> with poster, controls and aria-label', async () => {
    const html = await renderMarkdown(
      ':::video\n{"src": "https://example.com/a.mp4", "poster": "https://example.com/c.jpg", "title": "Demo"}\n:::\n'
    );

    expect(html).toContain('<video');
    expect(html).toContain('src="https://example.com/a.mp4"');
    expect(html).toContain('poster="https://example.com/c.jpg"');
    expect(html).toContain('controls=""');
    expect(html).toContain('aria-label="Demo"');
    // No visible caption on web either — the title lives on aria-label only.
    expect(html).not.toContain('>Demo<');
  });

  test('maps autoplay/loop/muted onto the element and defaults controls on', async () => {
    const html = await renderMarkdown(
      ':::video\n{"src": "https://example.com/a.mp4", "autoplay": true, "muted": true, "loop": true}\n:::\n'
    );

    expect(html).toContain('autoPlay=""');
    expect(html).toContain('muted=""');
    expect(html).toContain('loop=""');
    expect(html).toContain('controls=""');
  });

  test('preloads metadata when no poster so the browser shows the first frame', async () => {
    const html = await renderMarkdown(':::video\n{"src": "https://example.com/a.mp4"}\n:::\n');

    expect(html).toContain('preload="metadata"');
  });

  test('clamps width to a percentage of the container', async () => {
    const html = await renderMarkdown(
      ':::video\n{"src": "https://example.com/a.mp4", "width": 60}\n:::\n'
    );

    expect(html).toContain('width:60%');
  });

  test('renders an error card for invalid JSON instead of failing the document', async () => {
    const html = await renderMarkdown(':::video\n{bad json}\n:::\n');

    expect(html).toContain('Video config error');
    expect(html).toContain('{bad json}');
  });

  test('renders an error card when src is missing', async () => {
    const html = await renderMarkdown(':::video\n{"title": "No src"}\n:::\n');

    expect(html).toContain('Missing src config');
  });

  test('wrong-typed boolean flags never become truthy HTML attributes', async () => {
    const html = await renderMarkdown(
      ':::video\n{"src": "https://example.com/a.mp4", "autoplay": "false", "loop": 1, "muted": 0, "controls": "false"}\n:::\n'
    );

    expect(html).not.toContain('autoPlay');
    expect(html).not.toContain('loop=""');
    expect(html).not.toContain('muted=""');
    expect(html).toContain('controls=""');
  });

  test('drops a poster URL with a script-capable scheme', async () => {
    const html = await renderMarkdown(
      ':::video\n{"src": "https://example.com/a.mp4", "poster": "javascript:alert(1)"}\n:::\n'
    );

    expect(html).not.toContain('javascript:');
    expect(html).not.toContain('poster=');
    expect(html).toContain('preload="metadata"');
  });

  test('keeps a relative poster URL', async () => {
    const html = await renderMarkdown(
      ':::video\n{"src": "video.mp4", "poster": "./cover.jpg"}\n:::\n'
    );

    expect(html).toContain('poster="./cover.jpg"');
  });

  test('ignores wrong-typed parser error fields in a hand-built AST', () => {
    const html = renderContainerData({
      src: 'https://example.com/a.mp4',
      parseError: { message: 'hostile' },
      rawConfig: ['hostile'],
    });

    expect(html).toContain('<video');
    expect(html).not.toContain('hostile');
  });
});
