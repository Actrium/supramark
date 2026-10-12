import { describe, expect, it, mock } from 'bun:test';
import React from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { SupramarkRootNode } from '@supramark/core';

import './support/mock-react-native';
import './support/mock-renderer';

// react-test-renderer needs the act environment to flush effects synchronously.
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT =
  true;

const { Supramark } = await import('../src/Supramark');

function codeAst(value: string, lang?: string): SupramarkRootNode {
  return {
    type: 'root',
    ast_version: 2,
    diagnostics: [],
    children: [{ type: 'code', value, ...(lang ? { lang } : {}) }] as SupramarkRootNode['children'],
  } as SupramarkRootNode;
}

interface RenderOptions {
  onCopyCode?: (code: string, node: SupramarkCodeNodeLike) => void | Promise<void>;
  copyButton?: boolean;
}

type SupramarkCodeNodeLike = { type: 'code'; value: string; lang?: string };

async function renderAst(
  ast: SupramarkRootNode,
  options?: RenderOptions
): Promise<ReactTestRenderer> {
  let renderer: ReactTestRenderer | null = null;
  await act(async () => {
    renderer = create(React.createElement(Supramark, { ast, markdown: '', ...options }));
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
  return renderer as unknown as ReactTestRenderer;
}

describe('code block copy button (RN)', () => {
  it('omits the copy button when no onCopyCode is provided (RN stays clipboard-free)', async () => {
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'));
    expect(renderer.root.findAllByType('TouchableOpacity')).toHaveLength(0);
    // code content still renders
    const text = renderer.root.findByType('Text');
    expect(text.props.children).toBe('const x = 1\n');
  });

  it('renders the copy button when onCopyCode is provided and invokes it on press', async () => {
    const onCopyCode = mock(() => undefined);
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), { onCopyCode });
    const btn = renderer.root.findByType('TouchableOpacity');
    expect(typeof btn.props.onPress).toBe('function');

    await act(async () => {
      btn.props.onPress();
    });

    expect(onCopyCode).toHaveBeenCalledTimes(1);
    expect(onCopyCode.mock.calls[0][0]).toBe('const x = 1\n');
    expect(onCopyCode.mock.calls[0][1]).toMatchObject({ type: 'code' });
  });

  it('shows the language label in a header row beside the button', async () => {
    const onCopyCode = mock(() => undefined);
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), { onCopyCode });
    // The header carries the language Text and the button as siblings.
    const texts = renderer.root.findAllByType('Text');
    expect(texts.some(t => t.props.children === 'ts')).toBe(true);
  });

  it('omits the button when copyButton is false even if onCopyCode is provided', async () => {
    const onCopyCode = mock(() => undefined);
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), {
      onCopyCode,
      copyButton: false,
    });
    expect(renderer.root.findAllByType('TouchableOpacity')).toHaveLength(0);
  });

  it('shows the Copy label initially and switches to Copied after pressing', async () => {
    const onCopyCode = mock(() => undefined);
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), { onCopyCode });
    const btn = renderer.root.findByType('TouchableOpacity');
    const labelBefore = btn.children[0];
    expect(labelBefore.props.children).toBe('Copy');

    await act(async () => {
      btn.props.onPress();
    });

    const labelAfter = renderer.root.findByType('TouchableOpacity').children[0];
    expect(labelAfter.props.children).toBe('Copied');
    expect(renderer.root.findByType('TouchableOpacity').props.accessibilityLabel).toBe(
      'Copied code'
    );
  });

  it('waits for the host callback before reporting success', async () => {
    let resolveCopy: (() => void) | undefined;
    const onCopyCode = () =>
      new Promise<void>(resolve => {
        resolveCopy = resolve;
      });
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), { onCopyCode });
    const btn = renderer.root.findByType('TouchableOpacity');

    await act(async () => btn.props.onPress());
    expect(renderer.root.findByType('TouchableOpacity').children[0].props.children).toBe('Copy');
    expect(renderer.root.findByType('TouchableOpacity').props.accessibilityLabel).toBe('Copy code');

    await act(async () => resolveCopy?.());
    expect(renderer.root.findByType('TouchableOpacity').children[0].props.children).toBe('Copied');
    expect(renderer.root.findByType('TouchableOpacity').props.accessibilityLabel).toBe(
      'Copied code'
    );
  });

  it('does not schedule feedback after an in-flight copy unmounts', async () => {
    let resolveCopy: (() => void) | undefined;
    const onCopyCode = () =>
      new Promise<void>(resolve => {
        resolveCopy = resolve;
      });
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), { onCopyCode });
    await act(async () => renderer.root.findByType('TouchableOpacity').props.onPress());
    await act(async () => renderer.unmount());

    // Resolving after cleanup must not create the 1.5-second label timer.
    const originalSetTimeout = globalThis.setTimeout;
    const feedbackTimer = mock(() => 0 as unknown as ReturnType<typeof setTimeout>);
    globalThis.setTimeout = feedbackTimer as unknown as typeof setTimeout;
    try {
      resolveCopy?.();
      await Promise.resolve();
      await Promise.resolve();
      expect(feedbackTimer).not.toHaveBeenCalled();
    } finally {
      globalThis.setTimeout = originalSetTimeout;
    }
  });

  it('omits the button when the code block has no language even if onCopyCode is provided', async () => {
    const onCopyCode = mock(() => undefined);
    const renderer = await renderAst(codeAst('foo\n'), { onCopyCode });
    expect(renderer.root.findAllByType('TouchableOpacity')).toHaveLength(0);
    // code content still renders
    const text = renderer.root.findByType('Text');
    expect(text.props.children).toBe('foo\n');
  });

  it('a rejected onCopyCode leaves the label as Copy and raises no unhandledRejection', async () => {
    const rejections: unknown[] = [];
    const onUnhandled = (reason: unknown): void => {
      rejections.push(reason);
    };
    process.on('unhandledRejection', onUnhandled);
    try {
      const onCopyCode = () => Promise.reject(new Error('host denied'));
      const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), { onCopyCode });
      const btn = renderer.root.findByType('TouchableOpacity');

      await act(async () => {
        btn.props.onPress();
      });
      // Flush the rejected promise so the internal catch path settles.
      await act(async () => {});

      const label = renderer.root.findByType('TouchableOpacity').children[0];
      expect(label.props.children).toBe('Copy');
      expect(renderer.root.findByType('TouchableOpacity').props.accessibilityLabel).toBe(
        'Copy code'
      );
      await new Promise(resolve => setTimeout(resolve, 10));
      expect(rejections).toHaveLength(0);
    } finally {
      process.removeListener('unhandledRejection', onUnhandled);
    }
  });

  it('exposes accessibilityRole="button" so screen readers announce the control', async () => {
    const onCopyCode = mock(() => undefined);
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), { onCopyCode });
    const btn = renderer.root.findByType('TouchableOpacity');
    expect(btn.props.accessibilityRole).toBe('button');
    expect(btn.props.accessibilityLabel).toBe('Copy code');
  });

  it('keeps the card chrome on the container and off the body (no seam)', async () => {
    const onCopyCode = mock(() => undefined);
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), { onCopyCode });
    const { defaultStyles } = await import('../src/styles');
    // Container owns background + radius; header and body stay transparent so
    // no tint seam shows between header and code body.
    expect(defaultStyles.codeBlockContainer.backgroundColor).toBe('#f5f5f5');
    expect(defaultStyles.codeBlockHeader).not.toHaveProperty('backgroundColor');
    expect(defaultStyles.codeBlockBody).not.toHaveProperty('backgroundColor');
    expect(defaultStyles.codeBlockBody).not.toHaveProperty('borderRadius');
    expect(defaultStyles.codeBlockContainer).not.toHaveProperty('marginBottom');
    expect(renderer.root.findByType('TouchableOpacity')).toBeTruthy();
  });

  it('puts the dark background on the shared container instead of the header', async () => {
    const { darkThemeStyles } = await import('../src/styles');
    expect(darkThemeStyles.codeBlockContainer?.backgroundColor).toBe('#2d2d2d');
    expect(darkThemeStyles.codeBlockHeader).toBeUndefined();
  });

  it('keeps RN clipboard-free when copyButton is explicitly true without a handler', async () => {
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), { copyButton: true });
    expect(renderer.root.findAllByType('TouchableOpacity')).toHaveLength(0);
  });
});
