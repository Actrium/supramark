import { beforeEach, describe, expect, test } from 'bun:test';
import React from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { SupramarkRootNode } from '@supramark/core';

import { openUrlMock } from './support/mock-react-native';
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

/** Flattens the object/array style forms accepted by React Native. */
const flattenStyle = (style: unknown): Record<string, unknown> => {
  // Resolve nested RN style arrays in declaration order for assertions.
  if (Array.isArray(style)) {
    return Object.assign({}, ...style.filter(Boolean).map(flattenStyle));
  }
  return style && typeof style === 'object' ? (style as Record<string, unknown>) : {};
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
  beforeEach(() => {
    openUrlMock.mockClear();
  });

  test('host-injected renderer turns the opaque video node into a poster card', async () => {
    const renderer = await renderVideo();
    const text = flattenText(renderer.toJSON());
    const hostTypes = collectHostTypes(renderer.toJSON());

    // The raw JSON body is never rendered; the title survives only as the
    // tappable region's accessibility label (no visible caption).
    expect(text).not.toContain('"src"');
    expect(text).not.toContain('Product demo');
    const pressable = renderer.root.findByProps({ accessibilityRole: 'button' });
    expect(pressable.props.accessibilityLabel).toBe('Play video: Product demo');

    // The poster image is rendered as a host Image with a centered play badge.
    expect(hostTypes.has('Image')).toBe(true);
    expect(text).toContain('▶');
    const playIcon = renderer.root.find(
      item => item.type === 'Text' && item.children.includes('▶')
    );
    const badgeStyle = flattenStyle(playIcon.parent?.props.style);
    expect(badgeStyle).toMatchObject({
      position: 'absolute',
      top: 0,
      right: 0,
      bottom: 0,
      left: 0,
      alignItems: 'center',
      justifyContent: 'center',
      pointerEvents: 'none',
    });
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
    expect(openUrlMock).not.toHaveBeenCalled();
  });

  test('uses the resolved dark document styles for placeholder and error cards', async () => {
    const noPosterAst = {
      ...videoAst,
      children: [
        {
          ...videoAst.children[0],
          data: { src: 'https://example.com/demo.mp4' },
        },
      ],
    } as SupramarkRootNode;
    let placeholderRenderer: ReactTestRenderer | null = null;
    await act(async () => {
      placeholderRenderer = create(
        React.createElement(Supramark, {
          ast: noPosterAst,
          theme: 'dark',
          containerRenderers: { video: renderVideoContainerRN as never },
        })
      );
      await Promise.resolve();
    });
    const placeholder = placeholderRenderer!.root.find(
      item => item.type === 'View' && flattenStyle(item.props.style).aspectRatio === 16 / 9
    );
    expect(flattenStyle(placeholder.props.style).backgroundColor).toBe('#1a1a1a');

    const missingSrcAst = {
      ...videoAst,
      children: [{ ...videoAst.children[0], data: {} }],
    } as SupramarkRootNode;
    let errorRenderer: ReactTestRenderer | null = null;
    await act(async () => {
      errorRenderer = create(
        React.createElement(Supramark, {
          ast: missingSrcAst,
          theme: 'dark',
          containerRenderers: { video: renderVideoContainerRN as never },
        })
      );
      await Promise.resolve();
    });
    const errorTitle = errorRenderer!.root.find(
      item => item.type === 'Text' && item.children.includes('⚠️ Missing src config')
    );
    expect(flattenStyle(errorTitle.parent?.props.style).backgroundColor).toBe('#3a2225');
    expect(flattenStyle(errorTitle.props.style).color).toBe('#e8a1a8');
  });

  test('does not expose the full source URL as the fallback accessibility label', async () => {
    const noTitleAst = {
      ...videoAst,
      children: [
        {
          ...videoAst.children[0],
          data: { src: 'https://example.com/private/path/demo.mp4?token=secret' },
        },
      ],
    } as SupramarkRootNode;
    let renderer: ReactTestRenderer | null = null;
    await act(async () => {
      renderer = create(
        React.createElement(Supramark, {
          ast: noTitleAst,
          containerRenderers: { video: renderVideoContainerRN as never },
        })
      );
      await Promise.resolve();
    });

    const pressable = renderer!.root.findByProps({ accessibilityRole: 'button' });
    expect(pressable.props.accessibilityLabel).toBe('Play video');
  });

  test('refuses non-http schemes in the Linking fallback', async () => {
    const unsafeAst = {
      ...videoAst,
      children: [{ ...videoAst.children[0], data: { src: 'tel:+15551234567' } }],
    } as SupramarkRootNode;
    let renderer: ReactTestRenderer | null = null;
    await act(async () => {
      renderer = create(
        React.createElement(Supramark, {
          ast: unsafeAst,
          containerRenderers: { video: renderVideoContainerRN as never },
        })
      );
      await Promise.resolve();
    });

    const pressable = renderer!.root.findByProps({ accessibilityRole: 'button' });
    await act(async () => {
      pressable.props.onPress();
    });
    expect(openUrlMock).not.toHaveBeenCalled();
  });

  test('drops a poster URL with a script-capable scheme', async () => {
    const unsafePosterAst = {
      ...videoAst,
      children: [
        {
          ...videoAst.children[0],
          data: { src: 'https://example.com/demo.mp4', poster: 'javascript:alert(1)' },
        },
      ],
    } as SupramarkRootNode;
    let renderer: ReactTestRenderer | null = null;
    await act(async () => {
      renderer = create(
        React.createElement(Supramark, {
          ast: unsafePosterAst,
          containerRenderers: { video: renderVideoContainerRN as never },
        })
      );
      await Promise.resolve();
    });

    expect(collectHostTypes(renderer!.toJSON()).has('Image')).toBe(false);
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

describe(':::video hostile config (RN)', () => {
  test('non-string src degrades to the missing-src card instead of crashing', async () => {
    const hostileAst = {
      type: 'root',
      children: [
        {
          type: 'container',
          name: 'video',
          mode: 'opaque',
          // Hand-built ASTs may bypass parser validation; a numeric src must
          // not crash videoFileName and take the whole document down.
          data: { src: 123, width: '120%' },
          value: '{}',
          children: [],
        },
      ],
    } as unknown as SupramarkRootNode;
    let renderer: ReactTestRenderer | null = null;
    await act(async () => {
      renderer = create(
        React.createElement(Supramark, {
          ast: hostileAst,
          containerRenderers: { video: renderVideoContainerRN as never },
        })
      );
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    const text = flattenText(renderer!.toJSON());
    expect(text).toContain('Missing src config');
  });

  test('ignores wrong-typed parser error fields instead of rendering unsafe values', async () => {
    const hostileAst = {
      ...videoAst,
      children: [
        {
          ...videoAst.children[0],
          data: {
            src: 'https://example.com/demo.mp4',
            parseError: { message: 'hostile' },
            rawConfig: ['hostile'],
          },
        },
      ],
    } as unknown as SupramarkRootNode;
    let renderer: ReactTestRenderer | null = null;
    await act(async () => {
      renderer = create(
        React.createElement(Supramark, {
          ast: hostileAst,
          containerRenderers: { video: renderVideoContainerRN as never },
        })
      );
      await Promise.resolve();
    });

    expect(collectHostTypes(renderer!.toJSON()).has('Pressable')).toBe(true);
    expect(flattenText(renderer!.toJSON())).not.toContain('hostile');
  });
});
