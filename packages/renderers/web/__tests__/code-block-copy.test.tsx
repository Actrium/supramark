import React from 'react';
import { afterEach, describe, expect, mock, test } from 'bun:test';
import { Window } from 'happy-dom';
import { createRoot, type Root } from 'react-dom/client';
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
  onCopyCode?: (code: string, node: { type: 'code' }) => void;
  copyButton?: boolean;
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
});
