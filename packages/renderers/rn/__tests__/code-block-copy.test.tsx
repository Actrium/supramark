// cjk-allow-file: the host action labels below are Chinese on purpose — they
// assert that a host can localize the icon-only buttons, which is the one string
// the library deliberately leaves to the host instead of guessing a language.
import { describe, expect, it, mock } from 'bun:test';
import React from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type {
  SupramarkCodeBlockContent,
  SupramarkCodeHighlighter,
  SupramarkRootNode,
} from '@supramark/core';
import type { SupramarkCodeActionLabels, SupramarkSelectionOptions } from '../src/Supramark';

import './support/mock-react-native';
import './support/mock-renderer';

// react-test-renderer needs the act environment to flush effects synchronously.
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT =
  true;

const { Supramark } = await import('../src/Supramark');

type SupramarkCodeNodeLike = {
  type: 'code';
  value: string;
  lang?: string;
  position?: {
    start: { line: number; column: number; utf16_offset: number };
    end: { line: number; column: number; utf16_offset: number };
  };
};

function codeAst(value: string, lang?: string, position?: SupramarkCodeNodeLike['position']) {
  return {
    type: 'root',
    ast_version: 2,
    diagnostics: [],
    children: [
      { type: 'code', value, ...(lang ? { lang } : {}), ...(position ? { position } : {}) },
    ],
  } as unknown as SupramarkRootNode;
}

interface RenderOptions {
  markdown?: string;
  onCopyCode?: (code: string, node: SupramarkCodeNodeLike) => void | Promise<void>;
  onFullscreenCode?: (content: SupramarkCodeBlockContent) => void;
  copyButton?: boolean;
  fullscreenButton?: boolean;
  codeBlockChrome?: boolean;
  codeActionLabels?: SupramarkCodeActionLabels;
  /** Exercise the action row together with the document selection layer. */
  selection?: SupramarkSelectionOptions;
  /** Exercise the highlighted and plain code-content branches. */
  codeHighlighter?: SupramarkCodeHighlighter;
}

async function renderAst(
  ast: SupramarkRootNode,
  options?: RenderOptions
): Promise<ReactTestRenderer> {
  let renderer: ReactTestRenderer | null = null;
  await act(async () => {
    renderer = create(
      React.createElement(Supramark, { ast, markdown: options?.markdown ?? '', ...options })
    );
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
  return renderer as unknown as ReactTestRenderer;
}

/** Every string rendered inside a <Text>, so a label can be asserted without counting nodes. */
function renderedTexts(renderer: ReactTestRenderer): string[] {
  return renderer.root
    .findAllByType('Text')
    .flatMap(node => (typeof node.props.children === 'string' ? [node.props.children] : []));
}

/** The action button carrying an accessible name. */
function actionButton(renderer: ReactTestRenderer, label: string) {
  return renderer.root.findByProps({ accessibilityLabel: label });
}

/** Every icon mounted for an action id, e.g. 'copy' | 'fullscreen'. */
function actionIcons(renderer: ReactTestRenderer, id: string) {
  return renderer.root.findAllByProps({ testID: `supramark-code-action-${id}` });
}

describe('code block actions (RN)', () => {
  it('keeps the card and language label when no handler is provided (no buttons)', async () => {
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'));
    expect(renderer.root.findAllByType('TouchableOpacity')).toHaveLength(0);
    // The header is not tied to a handler: only the buttons are.
    expect(renderedTexts(renderer)).toContain('ts');
    expect(renderedTexts(renderer)).toContain('const x = 1\n');
  });

  it('renders the copy button when onCopyCode is provided and invokes it on press', async () => {
    const onCopyCode = mock(() => undefined);
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), { onCopyCode });
    const btn = actionButton(renderer, 'Copy code');
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
    expect(renderedTexts(renderer)).toContain('ts');
  });

  it('omits the copy button when copyButton is false even if onCopyCode is provided', async () => {
    const onCopyCode = mock(() => undefined);
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), {
      onCopyCode,
      copyButton: false,
    });
    expect(renderer.root.findAllByType('TouchableOpacity')).toHaveLength(0);
    // The header itself stays — only the button is switched off.
    expect(renderedTexts(renderer)).toContain('ts');
  });

  it('keeps the copy icon after a successful press (no success state)', async () => {
    const onCopyCode = mock(() => undefined);
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), { onCopyCode });

    await act(async () => {
      actionButton(renderer, 'Copy code').props.onPress();
    });

    // The button has one state only: the icon and the accessible name stay put.
    expect(actionIcons(renderer, 'copy')).toHaveLength(1);
    expect(actionButton(renderer, 'Copy code')).toBeTruthy();
  });

  it('does not schedule a timer after a copy resolves', async () => {
    let resolveCopy: (() => void) | undefined;
    const onCopyCode = () =>
      new Promise<void>(resolve => {
        resolveCopy = resolve;
      });
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), { onCopyCode });
    await act(async () => renderer.root.findByType('TouchableOpacity').props.onPress());

    // Nothing tracks the promise's settlement any more, so resolving (here or
    // after unmount) must not touch a timer.
    const originalSetTimeout = globalThis.setTimeout;
    const timer = mock(() => 0 as unknown as ReturnType<typeof setTimeout>);
    globalThis.setTimeout = timer as unknown as typeof setTimeout;
    try {
      resolveCopy?.();
      await Promise.resolve();
      await Promise.resolve();
      expect(timer).not.toHaveBeenCalled();
    } finally {
      globalThis.setTimeout = originalSetTimeout;
    }
  });

  it('omits the action buttons when the code block has no language', async () => {
    const onCopyCode = mock(() => undefined);
    const onFullscreenCode = mock(() => undefined);
    const renderer = await renderAst(codeAst('foo\n'), { onCopyCode, onFullscreenCode });
    expect(renderer.root.findAllByType('TouchableOpacity')).toHaveLength(0);
    // The card stays; only the header (and with it the buttons) is language-gated.
    expect(renderedTexts(renderer)).toContain('foo\n');
  });

  it('a rejected onCopyCode keeps the copy icon and raises no unhandledRejection', async () => {
    const rejections: unknown[] = [];
    const onUnhandled = (reason: unknown): void => {
      rejections.push(reason);
    };
    process.on('unhandledRejection', onUnhandled);
    try {
      const onCopyCode = () => Promise.reject(new Error('host denied'));
      const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), { onCopyCode });

      await act(async () => {
        actionButton(renderer, 'Copy code').props.onPress();
      });
      // Flush the rejected promise so the internal catch path settles.
      await act(async () => {});

      expect(actionIcons(renderer, 'copy')).toHaveLength(1);
      expect(actionButton(renderer, 'Copy code')).toBeTruthy();
      await new Promise(resolve => setTimeout(resolve, 10));
      expect(rejections).toHaveLength(0);
    } finally {
      process.removeListener('unhandledRejection', onUnhandled);
    }
  });

  it('exposes accessibilityRole="button" and an accessible name per action', async () => {
    const onCopyCode = mock(() => undefined);
    const onFullscreenCode = mock(() => undefined);
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), {
      onCopyCode,
      onFullscreenCode,
    });
    for (const label of ['Copy code', 'Open code fullscreen']) {
      const btn = actionButton(renderer, label);
      expect(btn.props.accessibilityRole).toBe('button');
      // An icon-only control is unlabelled for screen readers without this.
      expect(btn.props.accessibilityLabel).toBe(label);
    }
  });

  it('paints each icon as monochrome vector geometry, hidden from assistive tech', async () => {
    const onCopyCode = mock(() => undefined);
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), { onCopyCode });
    const svg = actionIcons(renderer, 'copy')[0];
    expect(svg.props.viewBox).toBe('0 0 24 24');
    // The button's label is the accessible name; the glyph must not repeat it.
    expect(svg.props.accessibilityElementsHidden).toBe(true);
    expect(svg.props.importantForAccessibility).toBe('no-hide-descendants');
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
    // The header only carries the divider colour in the dark theme — never a
    // background, which would show as a seam against the container.
    expect(darkThemeStyles.codeBlockHeader).not.toHaveProperty('backgroundColor');
    expect(darkThemeStyles.codeBlockHeader?.borderBottomColor).toBe('rgba(255, 255, 255, 0.12)');
    // The action button has no background in either theme.
    expect(darkThemeStyles.codeAction).toBeUndefined();
    expect(darkThemeStyles.codeActionIcon?.color).toBe('#ffffff');
  });

  it('sizes an icon-only button as a touch target', async () => {
    const { defaultStyles } = await import('../src/styles');
    // An icon leaves no text to give the button width, so the target is explicit.
    expect(defaultStyles.codeAction.minWidth).toBeGreaterThanOrEqual(24);
    expect(defaultStyles.codeAction.minHeight).toBeGreaterThanOrEqual(24);
    expect(defaultStyles.codeActionIcon.fontSize).toBeGreaterThan(0);
    // CodeBlock gives each button 3px of horizontal hit slop, so the row's gap
    // has to stay at least twice that or two neighbouring targets overlap and a
    // tap in the gap can land on the wrong action.
    expect(defaultStyles.codeActions.gap).toBeGreaterThanOrEqual(6);
  });

  it('keeps RN clipboard-free when copyButton is explicitly true without a handler', async () => {
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), { copyButton: true });
    expect(renderer.root.findAllByType('TouchableOpacity')).toHaveLength(0);
  });
});

describe('code block fullscreen action (RN)', () => {
  it('renders no fullscreen button without a handler', async () => {
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), {
      onCopyCode: mock(() => undefined),
    });
    expect(
      renderer.root.findAllByProps({ accessibilityLabel: 'Open code fullscreen' })
    ).toHaveLength(0);
  });

  it('hands the host the code, the fence and the node', async () => {
    const onFullscreenCode = mock((_content: SupramarkCodeBlockContent) => undefined);
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), { onFullscreenCode });

    await act(async () => {
      actionButton(renderer, 'Open code fullscreen').props.onPress();
    });

    expect(onFullscreenCode).toHaveBeenCalledTimes(1);
    const content = onFullscreenCode.mock.calls[0][0];
    expect(content.code).toBe('const x = 1\n');
    expect(content.lang).toBe('ts');
    expect(content.node).toMatchObject({ type: 'code', value: 'const x = 1\n' });
    // No source was supplied, so the fence is the canonical rebuild.
    expect(content.fenced).toBe('```ts\nconst x = 1\n```');
  });

  it("slices the author's own fence out of the source when it is available", async () => {
    const onFullscreenCode = mock((_content: SupramarkCodeBlockContent) => undefined);
    // A four-backtick fence around content that itself contains ``` — the case a
    // rebuilt three-backtick fence cannot express.
    const markdown = '````md\n```ts\nconst x = 1\n```\n````\n';
    const ast = codeAst('```ts\nconst x = 1\n```\n', 'md', {
      start: { line: 1, column: 1, utf16_offset: 0 },
      // The fence runs to the closing marker, exclusive of the trailing newline.
      end: { line: 5, column: 5, utf16_offset: markdown.trimEnd().length },
    }) as SupramarkRootNode;
    const renderer = await renderAst(ast, { markdown, onFullscreenCode });

    await act(async () => {
      actionButton(renderer, 'Open code fullscreen').props.onPress();
    });

    expect(onFullscreenCode.mock.calls[0][0].fenced).toBe('````md\n```ts\nconst x = 1\n```\n````');
  });

  it('omits the fullscreen button when fullscreenButton is false', async () => {
    const onFullscreenCode = mock(() => undefined);
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), {
      onFullscreenCode,
      fullscreenButton: false,
    });
    expect(renderer.root.findAllByType('TouchableOpacity')).toHaveLength(0);
    expect(renderedTexts(renderer)).toContain('ts');
  });

  it('shows both actions side by side when both handlers exist', async () => {
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), {
      onCopyCode: mock(() => undefined),
      onFullscreenCode: mock(() => undefined),
    });
    expect(renderer.root.findAllByType('TouchableOpacity')).toHaveLength(2);
    expect(actionIcons(renderer, 'copy')).toHaveLength(1);
    expect(actionIcons(renderer, 'fullscreen')).toHaveLength(1);
  });

  it('drops the whole card when codeBlockChrome is false', async () => {
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), {
      onCopyCode: mock(() => undefined),
      onFullscreenCode: mock(() => undefined),
      codeBlockChrome: false,
    });
    expect(renderer.root.findAllByType('TouchableOpacity')).toHaveLength(0);
    // No header either: the bare view is the plain pre-formatted text.
    expect(renderedTexts(renderer)).toEqual(['const x = 1\n']);
  });
});

describe('code block action labels (RN)', () => {
  it('uses the host labels for the accessible names', async () => {
    const renderer = await renderAst(codeAst('const x = 1\n', 'ts'), {
      onCopyCode: mock(() => undefined),
      onFullscreenCode: mock(() => undefined),
      codeActionLabels: { copy: '复制代码', fullscreen: '全屏查看代码' },
    });
    expect(actionButton(renderer, '复制代码')).toBeTruthy();
    expect(actionButton(renderer, '全屏查看代码')).toBeTruthy();
  });
});

// The merge must compose the code-action shell and selectable body in both render branches.
describe('code actions with document selection', () => {
  it.each([false, true])(
    'keeps source selectable and the action header separate (highlight=%s)',
    async highlighted => {
      // Keep the source simple enough to compare plain and highlighted text exactly.
      const source = 'const x = 1;';
      // Record the host action without using a platform clipboard.
      const onCopyCode = mock(() => undefined);
      // Supply real highlight tokens to exercise the second renderCodeContent branch.
      const codeHighlighter: SupramarkCodeHighlighter | undefined = highlighted
        ? async () => ({ lines: [{ tokens: [{ text: source, color: '#a00' }] }] })
        : undefined;
      // Mount both features on the same code block, with selection chrome disabled for clarity.
      const renderer = await renderAst(codeAst(source, 'ts'), {
        onCopyCode,
        onFullscreenCode: mock(() => undefined),
        codeHighlighter,
        selection: { overlay: false, handles: false, toolbar: false },
      });
      // Inspect the actual bound units produced by the mounted renderer.
      const { SelectableBlock, SelectionRoot, serializeSelectionUnits } =
        await import('@supramark/rn-selection');
      // The body has one selection registration; header labels and buttons have none.
      const block = renderer.root.findByType(SelectableBlock);
      expect(block.findAllByType('TouchableOpacity')).toHaveLength(0);
      // Restrict serialization to this block's bindings so chrome cannot enter copied text.
      const units = renderer.root.findByType(SelectionRoot).props.units;
      expect(
        serializeSelectionUnits(
          units.filter((unit: { unitId: string }) => block.props.unitIds.includes(unit.unitId))
        )
      ).toBe(source);
      // Read nested highlight Text leaves without counting the same characters twice.
      const textLeaves = (node: ReactTestRenderer['root'] | string): string =>
        typeof node === 'string' ? node : node.children.map(textLeaves).join('');
      expect(textLeaves(block)).toBe(source);
      await act(async () => actionButton(renderer, 'Copy code').props.onPress());
      expect(onCopyCode).toHaveBeenCalledTimes(1);
      expect(onCopyCode.mock.calls[0][0]).toBe(source);
      // The button keeps its single state; the action itself is what changed.
      expect(renderer.root.findByProps({ accessibilityLabel: 'Copy code' })).toBeTruthy();
      await act(async () => renderer.unmount());
    }
  );
});
