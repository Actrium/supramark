import React from 'react';
import { afterEach, describe, expect, mock, test } from 'bun:test';
import { Window } from 'happy-dom';
import { createRoot, type Root } from 'react-dom/client';
import { renderToStaticMarkup } from 'react-dom/server';
import type { SupramarkRootNode } from '@supramark/core';
import { Supramark } from '../src/Supramark';

type TestAct = (callback: () => void | Promise<void>) => Promise<void>;
const act = (React as typeof React & { act: TestAct }).act;
const browser = new Window();
const writeText = mock(() => Promise.resolve());
Object.assign(globalThis, {
  window: browser,
  document: browser.document,
  navigator: { ...browser.navigator, clipboard: { writeText } } as typeof browser.navigator,
  HTMLElement: browser.HTMLElement,
  Event: browser.Event,
  Node: browser.Node,
});
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT =
  true;

let root: Root | null = null;
type TestContainer = ReturnType<typeof browser.document.createElement>;

afterEach(async () => {
  if (root) {
    await act(async () => root?.unmount());
    root = null;
  }
  browser.document.body.replaceChildren();
});

function createContainer(): TestContainer {
  const container = browser.document.createElement('div');
  browser.document.body.appendChild(container);
  root = createRoot(container as unknown as HTMLDivElement);
  return container;
}

interface RenderOpts {
  onCopyCode?: (code: string, node: { type: 'code' }) => void | Promise<void>;
  copyButton?: boolean;
  theme?: 'tailwind' | 'minimal';
}

async function renderCode(
  value: string,
  lang: string | undefined,
  opts?: RenderOpts
): Promise<TestContainer> {
  const container = createContainer();
  const children = [{ type: 'code', value, ...(lang ? { lang } : {}) }];
  const ast = {
    type: 'root',
    ast_version: 2,
    diagnostics: [],
    children,
  } as SupramarkRootNode;
  await act(async () => {
    root?.render(<Supramark markdown="" ast={ast} {...opts} />);
  });
  return container;
}

function findButton(container: TestContainer): HTMLButtonElement | null {
  const buttons = container.getElementsByTagName('button');
  return buttons.length > 0 ? (buttons[0] as unknown as HTMLButtonElement) : null;
}

async function click(button: HTMLButtonElement): Promise<void> {
  await act(async () => {
    button.dispatchEvent(new browser.Event('click', { bubbles: true }));
  });
}

describe('code block copy button (web)', () => {
  test('renders a copy button by default and keeps the language class', async () => {
    const container = await renderCode('const x = 1\n', 'ts');
    const button = findButton(container);
    expect(button).not.toBeNull();
    expect(button?.textContent).toContain('Copy');
    expect(container.innerHTML).toContain('language-ts');
  });

  test('shows the language label in a header row beside the button', async () => {
    const container = await renderCode('const x = 1\n', 'ts');
    // The header carries the language text and the button as siblings, so the
    // button never overlays a code line.
    expect(container.innerHTML).toContain('>ts<');
    const button = findButton(container)!;
    const header = button.parentElement;
    expect(header?.textContent).toContain('ts');
    expect(header?.textContent).toContain('Copy');
  });

  test('marks the header and language label as non-selectable', async () => {
    const container = await renderCode('const x = 1\n', 'ts');
    // A select-all on the block must not sweep the language label or button
    // text into the clipboard. The default (empty classNames) path uses inline
    // user-select: none on the header, the language label, and the button.
    expect(container.innerHTML).toMatch(/user-select: none/);
  });

  test('omits the button when copyButton is false', async () => {
    const container = await renderCode('const x = 1\n', 'ts', { copyButton: false });
    expect(findButton(container)).toBeNull();
    expect(container.innerHTML).toContain('const x = 1');
  });

  test('clicking the button writes the code via navigator.clipboard and shows Copied', async () => {
    writeText.mockClear();
    const container = await renderCode('const x = 1\n', 'ts');
    const button = findButton(container)!;
    await click(button);
    expect(writeText).toHaveBeenCalledTimes(1);
    expect(writeText.mock.calls[0][0]).toBe('const x = 1\n');
    expect(button.textContent).toContain('Copied');
    expect(button.getAttribute('aria-label')).toBe('Copied code');
  });

  test('waits for the clipboard write before reporting success', async () => {
    let resolveWrite: (() => void) | undefined;
    writeText.mockImplementationOnce(
      () =>
        new Promise<void>(resolve => {
          resolveWrite = resolve;
        })
    );
    const container = await renderCode('const x = 1\n', 'ts');
    const button = findButton(container)!;

    await click(button);
    expect(button.textContent).toBe('Copy');
    expect(button.getAttribute('aria-label')).toBe('Copy code');

    await act(async () => resolveWrite?.());
    expect(button.textContent).toBe('Copied');
    expect(button.getAttribute('aria-label')).toBe('Copied code');
  });

  test('does not schedule feedback after an in-flight copy unmounts', async () => {
    let resolveWrite: (() => void) | undefined;
    writeText.mockImplementationOnce(
      () =>
        new Promise<void>(resolve => {
          resolveWrite = resolve;
        })
    );
    const container = await renderCode('const x = 1\n', 'ts');
    await click(findButton(container)!);
    await act(async () => root?.unmount());
    root = null;

    // Resolving after cleanup must not create the 1.5-second label timer.
    const originalSetTimeout = globalThis.setTimeout;
    const feedbackTimer = mock(() => 0 as unknown as ReturnType<typeof setTimeout>);
    globalThis.setTimeout = feedbackTimer as unknown as typeof setTimeout;
    try {
      resolveWrite?.();
      await Promise.resolve();
      await Promise.resolve();
      expect(feedbackTimer).not.toHaveBeenCalled();
    } finally {
      globalThis.setTimeout = originalSetTimeout;
    }
  });

  test('still resets Copied after the host replaces onCopyCode', async () => {
    const firstHandler = mock(() => undefined);
    const container = await renderCode('const x = 1\n', 'ts', { onCopyCode: firstHandler });
    const button = findButton(container)!;
    await click(button);
    expect(button.textContent).toBe('Copied');

    const ast = {
      type: 'root',
      ast_version: 2,
      diagnostics: [],
      children: [{ type: 'code', value: 'const x = 1\n', lang: 'ts' }],
    } as SupramarkRootNode;
    const replacementHandler = mock(() => undefined);
    await act(async () => {
      root?.render(<Supramark markdown="" ast={ast} onCopyCode={replacementHandler} />);
    });
    await act(async () => {
      await new Promise(resolve => setTimeout(resolve, 1600));
    });

    expect(findButton(container)?.textContent).toBe('Copy');
    expect(findButton(container)?.getAttribute('aria-label')).toBe('Copy code');
  });

  test('onCopyCode overrides the default clipboard call', async () => {
    writeText.mockClear();
    const onCopyCode = mock(() => undefined);
    const container = await renderCode('const x = 1\n', 'ts', { onCopyCode });
    const button = findButton(container)!;
    await click(button);
    expect(onCopyCode).toHaveBeenCalledTimes(1);
    expect(onCopyCode.mock.calls[0][0]).toBe('const x = 1\n');
    expect(writeText).not.toHaveBeenCalled();
    expect(button.textContent).toContain('Copied');
  });

  test('omits the button when the code block has no language (indented / language-less fence)', async () => {
    const container = await renderCode('foo\n', undefined);
    expect(findButton(container)).toBeNull();
    expect(container.innerHTML).toContain('foo');
  });

  test('a rejected writeText leaves the label as Copy (no fake success, no unhandledRejection)', async () => {
    const rejections: unknown[] = [];
    const onUnhandled = (reason: unknown): void => {
      rejections.push(reason);
    };
    process.on('unhandledRejection', onUnhandled);
    try {
      writeText.mockImplementationOnce(() => Promise.reject(new Error('denied')));
      const container = await renderCode('const x = 1\n', 'ts');
      const button = findButton(container);
      if (!button) {
        throw new Error('copy button not rendered');
      }
      await click(button);
      // Flush the rejected promise so the catch path settles before asserting.
      await act(async () => {});
      expect(button.textContent).toBe('Copy');
      expect(button.getAttribute('aria-label')).toBe('Copy code');
      await new Promise(resolve => setTimeout(resolve, 10));
      expect(rejections).toHaveLength(0);
    } finally {
      process.removeListener('unhandledRejection', onUnhandled);
    }
  });

  test('a rejected onCopyCode leaves the label as Copy and raises no unhandledRejection', async () => {
    const rejections: unknown[] = [];
    const onUnhandled = (reason: unknown): void => {
      rejections.push(reason);
    };
    process.on('unhandledRejection', onUnhandled);
    try {
      const onCopyCode = () => Promise.reject(new Error('host denied'));
      const container = await renderCode('const x = 1\n', 'ts', { onCopyCode });
      const button = findButton(container);
      if (!button) {
        throw new Error('copy button not rendered');
      }
      await click(button);
      await act(async () => {});
      expect(button.textContent).toBe('Copy');
      expect(button.getAttribute('aria-label')).toBe('Copy code');
      await new Promise(resolve => setTimeout(resolve, 10));
      expect(rejections).toHaveLength(0);
    } finally {
      process.removeListener('unhandledRejection', onUnhandled);
    }
  });

  test('copyButton false with a fenced language yields one bare pre without style', async () => {
    const container = await renderCode('const x = 1\n', 'ts', { copyButton: false });
    const rootElement = container.firstElementChild as unknown as HTMLElement;
    const pres = container.getElementsByTagName('pre');
    expect(pres.length).toBe(1);
    expect(pres[0].getAttribute('style')).toBeNull();
    expect(pres[0].getAttribute('class')).toBeNull();
    expect(rootElement.innerHTML).toBe('<pre><code class="language-ts">const x = 1\n</code></pre>');
    expect(findButton(container)).toBeNull();
  });

  test('tailwind theme keeps the standalone chrome on the bare pre (copyButton false)', async () => {
    const container = await renderCode('const x = 1\n', 'ts', {
      copyButton: false,
      theme: 'tailwind',
    });
    const pre = container.getElementsByTagName('pre')[0] as unknown as HTMLElement;
    expect(pre.className).toBe('bg-gray-100 dark:bg-gray-800 rounded-md p-4 mb-4 overflow-x-auto');
  });

  test('tailwind theme moves the chrome to the container and zeroes the headered body', async () => {
    const container = await renderCode('const x = 1\n', 'ts', { theme: 'tailwind' });
    const pre = container.getElementsByTagName('pre')[0] as unknown as HTMLElement;
    expect(pre.className).toBe('m-0 p-4 overflow-x-auto');
    const wrapper = pre.parentElement as unknown as HTMLElement | null;
    expect(wrapper?.className).toBe('bg-gray-100 dark:bg-gray-800 rounded-md mb-4 overflow-hidden');
  });

  test('tailwind theme keeps card chrome on a language-less code block', async () => {
    const container = await renderCode('plain\n', undefined, { theme: 'tailwind' });
    const pre = container.getElementsByTagName('pre')[0] as unknown as HTMLElement;
    const wrapper = pre.parentElement as unknown as HTMLElement | null;
    expect(pre.className).toBe('m-0 p-4 overflow-x-auto');
    expect(wrapper?.className).toBe('bg-gray-100 dark:bg-gray-800 rounded-md mb-4 overflow-hidden');
    expect(wrapper?.getElementsByTagName('button')).toHaveLength(0);
  });

  test('minimal theme wires every code-block hook without inline fallback styles', async () => {
    const container = await renderCode('const x = 1\n', 'ts', { theme: 'minimal' });
    const pre = container.getElementsByTagName('pre')[0] as unknown as HTMLElement;
    const wrapper = pre.parentElement as unknown as HTMLElement;
    const header = wrapper.firstElementChild as unknown as HTMLElement;
    const button = findButton(container)!;

    expect(wrapper.className).toBe('sm-code-block-container');
    expect(header.className).toBe('sm-code-block-header');
    expect((header.firstElementChild as unknown as HTMLElement).className).toBe(
      'sm-code-block-lang'
    );
    expect(button.className).toBe('sm-code-btn');
    expect((button.firstElementChild as unknown as HTMLElement).className).toBe('sm-code-btn-text');
    expect(pre.className).toBe('sm-code-block-body');
    expect(wrapper.getAttribute('style')).toBeNull();
    expect(header.getAttribute('style')).toBeNull();
    expect(button.getAttribute('style')).toBeNull();
    expect(pre.getAttribute('style')).toBeNull();
  });

  // Pins the default (copyButton on) DOM shape: the conformance harness runs
  // with copyButton={false}, so without this test the DOM the library emits by
  // default would be unmeasured.
  test('default DOM shape: every code block gets the card; only a language gets the header', async () => {
    const withLang = await renderCode('const x = 1\n', 'ts');
    const root = withLang.firstElementChild as unknown as HTMLElement;
    const card = root.firstElementChild as unknown as HTMLElement;
    expect(card.tagName).toBe('DIV');
    // Card chrome is inline. happy-dom strips var() values from its CSSOM, so
    // the --sm-code-bg background cannot be asserted here; assert the rest of
    // the inline chrome instead (source: INLINE_CONTAINER_STYLE).
    const cardStyle = card.getAttribute('style') ?? '';
    expect(cardStyle).toContain('border-radius: 4px');
    expect(cardStyle).toContain('overflow: hidden');
    // Header: language label + exactly one button.
    const header = card.firstElementChild as unknown as HTMLElement;
    expect(header.tagName).toBe('DIV');
    expect(header.textContent).toContain('ts');
    expect(header.getElementsByTagName('button').length).toBe(1);
    // Body pre is a direct child of the card.
    const bodyPre = card.getElementsByTagName('pre')[0] as unknown as HTMLElement;
    expect(bodyPre.parentElement).toBe(card);

    // Language-less fence: card without header or button, so adjacent blocks
    // with and without a language no longer look unrelated.
    const noLang = await renderCode('plain\n', undefined);
    const noLangRoot = noLang.firstElementChild as unknown as HTMLElement;
    const noLangCard = noLangRoot.firstElementChild as unknown as HTMLElement;
    expect(noLangCard.tagName).toBe('DIV');
    expect(noLangCard.children.length).toBe(1);
    expect(noLangCard.firstElementChild?.tagName).toBe('PRE');
    expect(noLangCard.getElementsByTagName('button').length).toBe(0);
  });

  test('server output exposes overridable color variables and matches initial hydration', () => {
    const ast = {
      type: 'root',
      ast_version: 2,
      diagnostics: [],
      children: [{ type: 'code', value: 'const x = 1\n', lang: 'ts' }],
    } as SupramarkRootNode;
    const html = renderToStaticMarkup(<Supramark markdown="" ast={ast} />);

    expect(html).toContain('background-color:var(--sm-code-bg, #f5f5f5)');
    expect(html).toContain('color:var(--sm-code-lang-color, rgba(0, 0, 0, 0.55))');
    expect(html).toContain('background-color:var(--sm-code-btn-bg, rgba(0, 0, 0, 0.5))');
    expect(html).toContain('<button type="button"');
  });

  test('hides the button but keeps the header when no clipboard API and no onCopyCode exist', async () => {
    const nav = globalThis.navigator as Navigator & { clipboard?: unknown };
    const originalClipboard = nav.clipboard;
    nav.clipboard = undefined;
    try {
      const container = await renderCode('const x = 1\n', 'ts');
      // An inert button is worse than none: the effect hides it post-mount.
      expect(findButton(container)).toBeNull();
      expect(container.innerHTML).toContain('>ts<');
    } finally {
      nav.clipboard = originalClipboard;
    }
  });

  test('hides the button when clipboard exists without a writeText function', async () => {
    const nav = globalThis.navigator as Navigator & { clipboard?: unknown };
    const originalClipboard = nav.clipboard;
    nav.clipboard = {};
    try {
      const container = await renderCode('const x = 1\n', 'ts');
      expect(findButton(container)).toBeNull();
      expect(container.innerHTML).toContain('>ts<');
    } finally {
      nav.clipboard = originalClipboard;
    }
  });
});
