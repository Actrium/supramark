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
  onCopyCode?: (code: string, node: SupramarkCodeNodeLike) => void;
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
  });

  it('omits the button when the code block has no language even if onCopyCode is provided', async () => {
    const onCopyCode = mock(() => undefined);
    const renderer = await renderAst(codeAst('foo\n'), { onCopyCode });
    expect(renderer.root.findAllByType('TouchableOpacity')).toHaveLength(0);
    // code content still renders
    const text = renderer.root.findByType('Text');
    expect(text.props.children).toBe('foo\n');
  });
});
