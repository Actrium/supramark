import { beforeEach, describe, expect, mock, test } from 'bun:test';
import React from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { SupramarkRootNode } from '@supramark/core';

import './support/mock-react-native';

// react-test-renderer requires the act environment flag to flush effects predictably.
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT =
  true;

const parserState = {
  calls: 0,
  lastRoot: null as SupramarkRootNode | null,
};

// Replace only the Rust parser package boundary so @supramark/core stays unmodified
// and this file cannot leak a core-module mock into other renderer tests.
const markdownParserModule = {
  parse: async (markdown: string): Promise<SupramarkRootNode> => {
    parserState.calls += 1;
    const root: SupramarkRootNode = markdown.startsWith('diagram document')
      ? {
          type: 'root',
          children: [
            {
              type: 'diagram',
              engine: 'mermaid',
              code: 'graph TD; A-->B;',
              fence_closed: true,
            },
          ],
        }
      : {
          type: 'root',
          children: [
            {
              type: 'paragraph',
              children: [{ type: 'text', value: markdown }],
            },
          ],
        };
    // Keep the exact object handed to the document cache so tests can assert
    // on the identity that getOrCreate retains.
    parserState.lastRoot = root;
    return root;
  },
};
mock.module('@supramark/markdown-web', () => markdownParserModule);
mock.module('@supramark/markdown-web/node', () => markdownParserModule);

// react-native / react-native-svg host mocks are registered once, process-wide,
// via './support/mock-react-native' (imported above). Per-file mocks here would
// clobber that shared surface — see the clobbering bug fixed in #90.

const { Supramark } = await import('../src/Supramark');
const { clearReactNativeRendererCaches } = await import('../src/renderCache');

// Reuse one config identity just like a host-level exported Supramark config.
const enabledCacheConfig = {
  options: {
    cache: true,
  },
  diagram: {
    defaultCache: {
      enabled: true,
      maxSize: 10,
      ttl: 60_000,
    },
  },
};

/** Mounts a Supramark document and waits for its asynchronous parse to settle. */
async function renderDocument(
  markdown: string,
  sourceState: 'streaming' | 'complete',
  config = enabledCacheConfig
): Promise<ReactTestRenderer> {
  let renderer: ReactTestRenderer | null = null;
  await act(async () => {
    renderer = create(
      React.createElement(Supramark, {
        markdown,
        sourceState,
        config,
      })
    );
    await Promise.resolve();
    await Promise.resolve();
  });
  return renderer as unknown as ReactTestRenderer;
}

/** Unmounts a renderer inside act so pending effects are cancelled cleanly. */
async function unmount(renderer: ReactTestRenderer): Promise<void> {
  await act(async () => {
    renderer.unmount();
  });
}

/** Detects Supramark's unstyled raw-Markdown parsing fallback. */
function hasRawMarkdownFallback(renderer: ReactTestRenderer, markdown: string): boolean {
  return renderer.root
    .findAllByType('Text' as never)
    .some(node => node.props.style === undefined && node.children.includes(markdown));
}

describe('Supramark completed-document cache', () => {
  beforeEach(() => {
    clearReactNativeRendererCaches();
    parserState.calls = 0;
  });

  test('restores a completed parsed document synchronously after remount', async () => {
    const markdown = 'cached paragraph';
    const firstRenderer = await renderDocument(markdown, 'complete');
    expect(parserState.calls).toBe(1);
    await unmount(firstRenderer);

    let secondRenderer: ReactTestRenderer | null = null;
    act(() => {
      secondRenderer = create(
        React.createElement(Supramark, {
          markdown,
          sourceState: 'complete',
          config: enabledCacheConfig,
        })
      );
    });

    expect(parserState.calls).toBe(1);
    expect(hasRawMarkdownFallback(secondRenderer as unknown as ReactTestRenderer, markdown)).toBe(
      false
    );
    await unmount(secondRenderer as unknown as ReactTestRenderer);
  });

  // #217: the root stored by getOrCreate must be the deep-frozen snapshot, so
  // every later cache hit hands consumers a read-only AST.
  test('stores the AST root frozen after getOrCreate', async () => {
    const renderer = await renderDocument('frozen paragraph', 'complete');
    expect(parserState.calls).toBe(1);
    expect(parserState.lastRoot).not.toBeNull();
    expect(Object.isFrozen(parserState.lastRoot)).toBe(true);
    await unmount(renderer);
  });

  test('shares completed documents across equivalent inline config objects', async () => {
    const markdown = 'inline config paragraph';
    const firstRenderer = await renderDocument(markdown, 'complete', {
      options: { cache: true },
    });
    expect(parserState.calls).toBe(1);
    await unmount(firstRenderer);

    let secondRenderer: ReactTestRenderer | null = null;
    act(() => {
      secondRenderer = create(
        React.createElement(Supramark, {
          markdown,
          sourceState: 'complete',
          config: { options: { cache: true } },
        })
      );
    });

    expect(parserState.calls).toBe(1);
    expect(hasRawMarkdownFallback(secondRenderer as unknown as ReactTestRenderer, markdown)).toBe(
      false
    );
    await unmount(secondRenderer as unknown as ReactTestRenderer);
  });

  test('does not cache a streaming document version', async () => {
    const markdown = 'growing paragraph';
    const streamingRenderer = await renderDocument(markdown, 'streaming');
    await unmount(streamingRenderer);

    const completeRenderer = await renderDocument(markdown, 'complete');
    expect(parserState.calls).toBe(2);
    await unmount(completeRenderer);
  });

  test('diagram.defaultCache retains a completed diagram document without enabling global cache', async () => {
    const diagramOnlyCacheConfig = {
      features: [{ id: '@supramark/feature-mermaid', enabled: false }],
      diagram: {
        defaultCache: {
          enabled: true,
          maxSize: 10,
          ttl: 60_000,
        },
      },
    };
    const firstRenderer = await renderDocument(
      'diagram document',
      'complete',
      diagramOnlyCacheConfig
    );
    await unmount(firstRenderer);
    const secondRenderer = await renderDocument(
      'diagram document',
      'complete',
      diagramOnlyCacheConfig
    );

    expect(parserState.calls).toBe(1);
    await unmount(secondRenderer);
  });

  test('an explicit diagram policy still retains diagram documents when global cache is false', async () => {
    const createConfig = () => ({
      options: { cache: false },
      features: [{ id: '@supramark/feature-mermaid', enabled: false }],
      diagram: {
        defaultCache: {
          enabled: true,
          maxSize: 10,
          ttl: 60_000,
        },
      },
    });
    const firstRenderer = await renderDocument('diagram document', 'complete', createConfig());
    await unmount(firstRenderer);
    const secondRenderer = await renderDocument('diagram document', 'complete', createConfig());

    expect(parserState.calls).toBe(1);
    await unmount(secondRenderer);
  });

  test('an enabled engine cache retains its diagram documents when global cache is false', async () => {
    const createConfig = () => ({
      options: { cache: false },
      features: [{ id: '@supramark/feature-mermaid', enabled: false }],
      diagram: {
        engines: {
          mermaid: {
            cache: {
              enabled: true,
              maxSize: 10,
              ttl: 60_000,
            },
          },
        },
      },
    });
    const firstRenderer = await renderDocument('diagram document', 'complete', createConfig());
    await unmount(firstRenderer);
    const secondRenderer = await renderDocument('diagram document', 'complete', createConfig());

    expect(parserState.calls).toBe(1);
    await unmount(secondRenderer);
  });

  test('diagram.defaultCache does not retain a pure-text document by itself', async () => {
    const diagramOnlyCacheConfig = {
      diagram: {
        defaultCache: {
          enabled: true,
          maxSize: 10,
          ttl: 60_000,
        },
      },
    };
    const firstRenderer = await renderDocument(
      'plain document',
      'complete',
      diagramOnlyCacheConfig
    );
    await unmount(firstRenderer);
    const secondRenderer = await renderDocument(
      'plain document',
      'complete',
      diagramOnlyCacheConfig
    );

    expect(parserState.calls).toBe(2);
    await unmount(secondRenderer);
  });

  test('an enabled engine cache retains documents containing that engine', async () => {
    const engineOnlyCacheConfig = {
      features: [{ id: '@supramark/feature-mermaid', enabled: false }],
      diagram: {
        engines: {
          mermaid: {
            cache: {
              enabled: true,
              maxSize: 10,
              ttl: 60_000,
            },
          },
        },
      },
    };
    const firstRenderer = await renderDocument(
      'diagram document',
      'complete',
      engineOnlyCacheConfig
    );
    await unmount(firstRenderer);
    const secondRenderer = await renderDocument(
      'diagram document',
      'complete',
      engineOnlyCacheConfig
    );

    expect(parserState.calls).toBe(1);
    await unmount(secondRenderer);
  });

  test('uses the strictest engine maxBytes for the shared parsed-document cache', async () => {
    const config = {
      features: [{ id: '@supramark/feature-mermaid', enabled: false }],
      diagram: {
        engines: {
          mermaid: {
            cache: { enabled: true, maxSize: 10, maxBytes: 100 },
          },
          dot: {
            cache: { enabled: true, maxSize: 10, maxBytes: 1_000 },
          },
        },
      },
    };

    const first = await renderDocument('diagram document one', 'complete', config);
    await unmount(first);
    const second = await renderDocument('diagram document two', 'complete', config);
    await unmount(second);
    expect(parserState.calls).toBe(2);

    const firstAgain = await renderDocument('diagram document one', 'complete', config);
    expect(parserState.calls).toBe(3);
    await unmount(firstAgain);
  });

  test('does not retain parsed documents when caching is disabled', async () => {
    const markdown = 'uncached paragraph';
    const disabledConfig = {
      diagram: {
        defaultCache: {
          enabled: false,
        },
      },
    };
    const firstRenderer = await renderDocument(markdown, 'complete', disabledConfig);
    await unmount(firstRenderer);
    const secondRenderer = await renderDocument(markdown, 'complete', disabledConfig);

    expect(parserState.calls).toBe(2);
    await unmount(secondRenderer);
  });
});
