// cjk-allow-file: the host action labels below are Chinese on purpose — they
// assert that a host can localize the icon-only buttons, which is the one string
// the library deliberately leaves to the host instead of guessing a language.
import React from 'react';
import { afterEach, describe, expect, mock, test } from 'bun:test';
import { Window } from 'happy-dom';
import { createRoot, type Root } from 'react-dom/client';
import { renderToStaticMarkup } from 'react-dom/server';
import type { SupramarkRootNode } from '@supramark/core';
import { Supramark } from '../src/Supramark';
import type { SupramarkCodeActionLabels } from '../src/Supramark';

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
  codeBlockChrome?: boolean;
  codeActionLabels?: SupramarkCodeActionLabels;
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

// Lookups walk the tree by tag name on purpose: happy-dom's `querySelector` is
// unusable under `bun test` (its SelectorParser reaches for a `window.SyntaxError`
// the test global does not provide, and throws on every selector, valid or not).
/** Every action button in the container, in document order. */
function findActions(container: TestContainer): HTMLButtonElement[] {
  return Array.from(container.getElementsByTagName('button')) as unknown as HTMLButtonElement[];
}

/** The action button carrying an accessible name. */
function findAction(container: TestContainer, label: string): HTMLButtonElement | null {
  return findActions(container).find(button => button.getAttribute('aria-label') === label) ?? null;
}

/** The element marked as the icon for an action: 'copy' | 'fullscreen'. */
function findIcon(container: TestContainer, id: string): Element | null {
  const elements = Array.from(container.getElementsByTagName('*')) as unknown as Element[];
  return (
    elements.find(element => element.getAttribute('data-supramark-code-action') === id) ?? null
  );
}

async function click(button: HTMLButtonElement): Promise<void> {
  await act(async () => {
    button.dispatchEvent(new browser.Event('click', { bubbles: true }));
  });
}

describe('code block actions (web)', () => {
  test('renders the copy button by default and keeps the language class', async () => {
    const container = await renderCode('const x = 1\n', 'ts');
    const button = findAction(container, 'Copy code');
    expect(button).not.toBeNull();
    expect(container.innerHTML).toContain('language-ts');
    // The button is icon-only: the accessible name carries what text used to.
    expect(findIcon(container, 'copy')).not.toBeNull();
  });

  test('shows the language label in a header row beside the button', async () => {
    const container = await renderCode('const x = 1\n', 'ts');
    // The header carries the language text and the action row as siblings, so
    // the button never overlays a code line.
    expect(container.innerHTML).toContain('>ts<');
    const button = findAction(container, 'Copy code')!;
    // button → actions row → header; the extra level is what keeps the buttons
    // clear of the language label.
    const header = button.parentElement?.parentElement;
    expect(header?.tagName).toBe('DIV');
    expect(header?.textContent).toContain('ts');
  });

  test('marks the header and language label as non-selectable', async () => {
    const container = await renderCode('const x = 1\n', 'ts');
    // A select-all on the block must not sweep the language label or the action
    // buttons into the clipboard. The default (empty classNames) path uses
    // inline user-select: none on the header, the language label, and the buttons.
    expect(container.innerHTML).toMatch(/user-select: none/);
  });

  test('omits the copy button when copyButton is false, keeping the header', async () => {
    const container = await renderCode('const x = 1\n', 'ts', { copyButton: false });
    expect(findActions(container)).toHaveLength(0);
    expect(container.innerHTML).toContain('>ts<');
    expect(container.innerHTML).toContain('const x = 1');
  });

  test('drops the entire card when codeBlockChrome is false', async () => {
    const container = await renderCode('const x = 1\n', 'ts', { codeBlockChrome: false });
    const rootElement = container.firstElementChild as unknown as HTMLElement;
    const pres = container.getElementsByTagName('pre');
    expect(pres.length).toBe(1);
    expect(pres[0].getAttribute('style')).toBeNull();
    expect(pres[0].getAttribute('class')).toBeNull();
    expect(rootElement.innerHTML).toBe('<pre><code class="language-ts">const x = 1\n</code></pre>');
    expect(findActions(container)).toHaveLength(0);
  });

  test('clicking the button writes the code via navigator.clipboard and swaps the icon', async () => {
    writeText.mockClear();
    const container = await renderCode('const x = 1\n', 'ts');
    const button = findAction(container, 'Copy code')!;
    expect(findIcon(container, 'copy')).not.toBeNull();

    await click(button);

    expect(writeText).toHaveBeenCalledTimes(1);
    expect(writeText.mock.calls[0][0]).toBe('const x = 1\n');
    // The button has one state only: the icon and the accessible name stay put.
    expect(findIcon(container, 'copy')).not.toBeNull();
    expect(findAction(container, 'Copy code')).not.toBeNull();
  });

  test('does not schedule a timer after the clipboard write resolves', async () => {
    let resolveWrite: (() => void) | undefined;
    writeText.mockImplementationOnce(
      () =>
        new Promise<void>(resolve => {
          resolveWrite = resolve;
        })
    );
    const container = await renderCode('const x = 1\n', 'ts');
    const button = findAction(container, 'Copy code')!;
    await click(button);
    await act(async () => root?.unmount());
    root = null;

    // Nothing tracks the promise's settlement any more, so resolving after
    // cleanup must not touch a timer.
    const originalSetTimeout = globalThis.setTimeout;
    const timer = mock(() => 0 as unknown as ReturnType<typeof setTimeout>);
    globalThis.setTimeout = timer as unknown as typeof setTimeout;
    try {
      resolveWrite?.();
      await Promise.resolve();
      await Promise.resolve();
      expect(timer).not.toHaveBeenCalled();
    } finally {
      globalThis.setTimeout = originalSetTimeout;
    }
  });

  test('onCopyCode overrides the default clipboard call', async () => {
    writeText.mockClear();
    const onCopyCode = mock(() => undefined);
    const container = await renderCode('const x = 1\n', 'ts', { onCopyCode });
    await click(findAction(container, 'Copy code')!);
    expect(onCopyCode).toHaveBeenCalledTimes(1);
    expect(onCopyCode.mock.calls[0][0]).toBe('const x = 1\n');
    expect(writeText).not.toHaveBeenCalled();
    expect(findIcon(container, 'copy')).not.toBeNull();
  });

  test('omits the action buttons when the code block has no language (indented / language-less fence)', async () => {
    const container = await renderCode('foo\n', undefined);
    expect(findActions(container)).toHaveLength(0);
    expect(container.innerHTML).toContain('foo');
  });

  test('a rejected writeText keeps the copy icon (no fake success, no unhandledRejection)', async () => {
    const rejections: unknown[] = [];
    const onUnhandled = (reason: unknown): void => {
      rejections.push(reason);
    };
    process.on('unhandledRejection', onUnhandled);
    try {
      writeText.mockImplementationOnce(() => Promise.reject(new Error('denied')));
      const container = await renderCode('const x = 1\n', 'ts');
      const button = findAction(container, 'Copy code');
      if (!button) {
        throw new Error('copy button not rendered');
      }
      await click(button);
      // Flush the rejected promise so the catch path settles before asserting.
      await act(async () => {});
      expect(findIcon(container, 'copy')).not.toBeNull();
      expect(findAction(container, 'Copy code')).not.toBeNull();
      await new Promise(resolve => setTimeout(resolve, 10));
      expect(rejections).toHaveLength(0);
    } finally {
      process.removeListener('unhandledRejection', onUnhandled);
    }
  });

  test('a rejected onCopyCode keeps the copy icon and raises no unhandledRejection', async () => {
    const rejections: unknown[] = [];
    const onUnhandled = (reason: unknown): void => {
      rejections.push(reason);
    };
    process.on('unhandledRejection', onUnhandled);
    try {
      const onCopyCode = () => Promise.reject(new Error('host denied'));
      const container = await renderCode('const x = 1\n', 'ts', { onCopyCode });
      const button = findAction(container, 'Copy code');
      if (!button) {
        throw new Error('copy button not rendered');
      }
      await click(button);
      await act(async () => {});
      expect(findIcon(container, 'copy')).not.toBeNull();
      await new Promise(resolve => setTimeout(resolve, 10));
      expect(rejections).toHaveLength(0);
    } finally {
      process.removeListener('unhandledRejection', onUnhandled);
    }
  });
});

// The fullscreen action is React Native only: a mobile code block is the one
// that needs to escape a narrow column. Web must not grow the button back.
describe('code block actions: no fullscreen action on web', () => {
  test('renders exactly one action button, the copy one', async () => {
    const container = await renderCode('const x = 1\n', 'ts');
    expect(findActions(container)).toHaveLength(1);
    expect(findAction(container, 'Open code fullscreen')).toBeNull();
    expect(findIcon(container, 'fullscreen')).toBeNull();
    // No prop on web can bring it back — the renderer keeps no handler for it.
    expect(container.innerHTML).not.toContain('fullscreen');
  });

  test('gives the icon-only button a native hover tooltip and an accessible name', async () => {
    const container = await renderCode('const x = 1\n', 'ts');
    // The icon shows no words, so `title` is the only pointer affordance and
    // `aria-label` the only programmatic name.
    const button = findAction(container, 'Copy code')!;
    expect(button.getAttribute('title')).toBe('Copy code');
    expect(button.getAttribute('aria-label')).toBe('Copy code');
  });

  test('hides the decorative icons from assistive technology', async () => {
    const container = await renderCode('const x = 1\n', 'ts');
    const icon = findIcon(container, 'copy')!;
    // The button owns the name; an announced unlabelled graphic would double it.
    expect(icon.getAttribute('aria-hidden')).toBe('true');
    expect(icon.getAttribute('viewBox')).toBe('0 0 24 24');
    // currentColor lets the theme tint the glyph through the button colour.
    const path = icon.firstElementChild!;
    expect(path.getAttribute('stroke')).toBe('currentColor');
  });
});

describe('code block action labels (web)', () => {
  test('uses the host labels for the tooltip and the accessible name', async () => {
    const container = await renderCode('const x = 1\n', 'ts', {
      codeActionLabels: { copy: '复制代码' },
    });
    expect(findAction(container, '复制代码')?.getAttribute('title')).toBe('复制代码');
    expect(findAction(container, '复制代码')?.getAttribute('aria-label')).toBe('复制代码');
  });
});

describe('code block chrome (web)', () => {
  test('tailwind theme keeps the standalone chrome on the bare pre (chrome off)', async () => {
    const container = await renderCode('const x = 1\n', 'ts', {
      codeBlockChrome: false,
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

  test('tailwind theme styles the action row and buttons', async () => {
    const container = await renderCode('const x = 1\n', 'ts', { theme: 'tailwind' });
    const button = findAction(container, 'Copy code')!;
    expect(button.className).toContain('inline-flex');
    const actions = button.parentElement as unknown as HTMLElement;
    expect(actions.className).toBe('flex items-center gap-1.5');
  });

  test('minimal theme wires every code-block hook without inline fallback styles', async () => {
    const container = await renderCode('const x = 1\n', 'ts', { theme: 'minimal' });
    const pre = container.getElementsByTagName('pre')[0] as unknown as HTMLElement;
    const wrapper = pre.parentElement as unknown as HTMLElement;
    const header = wrapper.firstElementChild as unknown as HTMLElement;
    const button = findAction(container, 'Copy code')!;

    expect(wrapper.className).toBe('sm-code-block-container');
    expect(header.className).toBe('sm-code-block-header');
    expect((header.firstElementChild as unknown as HTMLElement).className).toBe(
      'sm-code-block-lang'
    );
    expect((header.lastElementChild as unknown as HTMLElement).className).toBe('sm-code-actions');
    expect(button.className).toBe('sm-code-action');
    expect(pre.className).toBe('sm-code-block-body');
    expect(wrapper.getAttribute('style')).toBeNull();
    expect(header.getAttribute('style')).toBeNull();
    expect(button.getAttribute('style')).toBeNull();
    expect(pre.getAttribute('style')).toBeNull();
  });

  // Pins the default (chrome on) DOM shape: the conformance harness runs with
  // codeBlockChrome={false}, so without this test the DOM the library emits by
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
    // Header: language label + exactly one button (copy; no fullscreen handler).
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
    expect(html).toContain('color:var(--sm-code-action-color, rgba(0, 0, 0, 0.55))');
    // Web has no header divider, and the button carries no resting background —
    // a bare <button> would otherwise paint the UA's `buttonface`.
    expect(html).not.toContain('border-bottom');
    expect(html).toContain('background:none');
    expect(html).toContain('<button type="button"');
  });

  test('starts with no resting background (the hover tint is not applied yet)', async () => {
    // The hover tint itself is not asserted here: React synthesises
    // onPointerEnter/onPointerLeave from pointerover/pointerout, and that
    // synthesis does not fire under happy-dom (tried mouse and pointer events),
    // so the hovered state cannot be driven from this harness. The resting
    // state — the part that used to be wrong (a bare <button> paints the UA's
    // `buttonface`) — is covered above and in the server-output test.
    const container = await renderCode('const x = 1\n', 'ts');
    const button = findAction(container, 'Copy code')!;
    expect(button.getAttribute('style')).toContain('background: none');
    expect(button.getAttribute('style')).not.toContain('hover-bg');
  });

  test('hides the button but keeps the header when no clipboard API and no onCopyCode exist', async () => {
    const nav = globalThis.navigator as Navigator & { clipboard?: unknown };
    const originalClipboard = nav.clipboard;
    nav.clipboard = undefined;
    try {
      const container = await renderCode('const x = 1\n', 'ts');
      // An inert button is worse than none: the effect hides it post-mount.
      expect(findActions(container)).toHaveLength(0);
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
      expect(findActions(container)).toHaveLength(0);
      expect(container.innerHTML).toContain('>ts<');
    } finally {
      nav.clipboard = originalClipboard;
    }
  });
});
