import { describe, expect, test } from 'bun:test';
import React from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { SupramarkRootNode } from '@supramark/core';

import './support/mock-react-native';
import './support/mock-renderer';

// React's test renderer requires this flag before effects can be flushed through act().
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT =
  true;

// Import after the react-native mock is registered: the feature package's RN
// runtime statically imports react-native, whose Flow-typed entry bun can't load.
const { renderVideoContainerRN } = await import('@supramark/feature-video');
const { Supramark } = await import('../src/Supramark');

// Match the opaque node shape returned by the native parser for :::video with
// a valid JSON body: structured data, empty children, raw body kept on value.
const videoAst: SupramarkRootNode = {
  type: 'root',
  children: [
    {
      type: 'container',
      name: 'video',
      mode: 'opaque',
      data: {
        src: 'https://example.com/demo.mp4',
        poster: 'https://example.com/cover.jpg',
        title: 'Product demo',
      },
      value: '{"src":"https://example.com/demo.mp4"}',
      children: [],
    },
  ],
} as SupramarkRootNode;

// Test-renderer JSON nodes keep children as a sibling of props, not inside it.
interface TestRendererNode {
  type: string;
  props: Record<string, unknown>;
  children?: TestRendererNode[] | null;
}

const asJsonNodes = (node: unknown): TestRendererNode[] =>
  Array.isArray(node) ? (node as TestRendererNode[]) : node ? [node as TestRendererNode] : [];

// Read string leaves from host Text nodes without depending on native text measurement.
const flattenText = (node: unknown): string => {
  if (node == null || typeof node === 'boolean') return '';
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(flattenText).join('');
  if (typeof node === 'object' && 'type' in (node as Record<string, unknown>)) {
    return flattenText((node as TestRendererNode).children);
  }
  return '';
};

// Collect host component types (e.g. 'Image', 'Text') from the rendered tree.
const collectHostTypes = (node: unknown, out: Set<string> = new Set()): Set<string> => {
  for (const item of asJsonNodes(node)) {
    out.add(item.type);
    collectHostTypes(item.children, out);
  }
  return out;
};

const renderVideo = async (): Promise<ReactTestRenderer> => {
  let renderer: ReactTestRenderer | null = null;
  await act(async () => {
    renderer = create(
      React.createElement(Supramark, {
        ast: videoAst,
        containerRenderers: { video: renderVideoContainerRN as never },
      })
    );
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
  return renderer as unknown as ReactTestRenderer;
};

describe(':::video container rendering (RN)', () => {
  test('host-injected renderer turns the opaque video node into a poster card', async () => {
    const renderer = await renderVideo();
    const text = flattenText(renderer.toJSON());
    const hostTypes = collectHostTypes(renderer.toJSON());

    // The caption is rendered instead of the raw JSON body.
    expect(text).toContain('Product demo');
    expect(text).not.toContain('"src"');

    // The poster image is rendered as a host Image.
    expect(hostTypes.has('Image')).toBe(true);
  });

  test('onVideoPress prop reaches the container renderer and fires on tap', async () => {
    const events: Array<{ src: string; poster?: string; title?: string }> = [];
    let renderer: ReactTestRenderer | null = null;
    await act(async () => {
      renderer = create(
        React.createElement(Supramark, {
          ast: videoAst,
          containerRenderers: { video: renderVideoContainerRN as never },
          onVideoPress: event => events.push(event),
        })
      );
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    const pressable = renderer!.root.findByProps({ accessibilityRole: 'button' });
    await act(async () => {
      pressable.props.onPress();
    });

    expect(events).toEqual([
      {
        src: 'https://example.com/demo.mp4',
        poster: 'https://example.com/cover.jpg',
        title: 'Product demo',
      },
    ]);
  });

  test('missing renderer renders an empty generic container (documented fallback)', async () => {
    let renderer: ReactTestRenderer | null = null;
    await act(async () => {
      renderer = create(React.createElement(Supramark, { ast: videoAst }));
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    const text = flattenText(renderer!.toJSON());

    // No renderer → generic block with no params and empty children → blank.
    expect(text).toBe('');
  });
});
