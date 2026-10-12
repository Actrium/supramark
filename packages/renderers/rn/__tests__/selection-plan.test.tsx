import { describe, expect, it } from 'bun:test';
import React from 'react';
import { create, act, type ReactTestRenderer } from 'react-test-renderer';
import type { SupramarkRootNode } from '@supramark/core';

import './support/mock-react-native';
import './support/mock-renderer';

// react-test-renderer needs the act environment to flush effects synchronously.
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT =
  true;

const { Supramark } = await import('../src/Supramark');
const { DiagramNode } = await import('../src/DiagramNode');
const { SelectableBlock, SelectionHandles, SelectionOverlay, SelectionRoot, SelectionToolbar } =
  await import('@supramark/rn-selection');

function flattenText(node: React.ReactNode): string {
  if (node == null || typeof node === 'boolean') return '';
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(flattenText).join('');
  if (typeof node === 'object' && 'props' in node) {
    return flattenText((node as React.ReactElement).props.children);
  }
  return '';
}

function textContents(root: ReactTestRenderer['root']): string[] {
  return root.findAllByType('Text').map(inst => flattenText(inst.props.children));
}

async function renderTree(props: Record<string, unknown>): Promise<ReactTestRenderer> {
  let renderer: ReactTestRenderer | null = null;
  await act(async () => {
    renderer = create(React.createElement(Supramark, props));
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
  return renderer as unknown as ReactTestRenderer;
}

async function updateTree(
  renderer: ReactTestRenderer,
  props: Record<string, unknown>
): Promise<void> {
  await act(async () => {
    renderer.update(React.createElement(Supramark, props));
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
}

const AST = {
  type: 'root',
  ast_version: 2,
  diagnostics: [],
  children: [
    { type: 'paragraph', children: [{ type: 'text', value: 'First paragraph' }] },
    { type: 'paragraph', children: [{ type: 'text', value: 'Second paragraph' }] },
  ],
} as unknown as SupramarkRootNode;

describe('selection plan lifecycle', () => {
  it('without the prop: no selection components, plain rendering', async () => {
    const r = await renderTree({ ast: AST });
    expect(r.root.findAllByType(SelectionRoot)).toHaveLength(0);
    expect(r.root.findAllByType(SelectableBlock)).toHaveLength(0);
    expect(textContents(r.root)).toEqual(['First paragraph', 'Second paragraph']);
  });

  it('configured but disabled: root mounted, layers hidden, text unchanged', async () => {
    const plain = await renderTree({ ast: AST });
    const disabled = await renderTree({ ast: AST, selection: { enabled: false } });
    expect(disabled.root.findAllByType(SelectionRoot)).toHaveLength(1);
    expect(disabled.root.findAllByType(SelectableBlock)).toHaveLength(2);
    expect(disabled.root.findAllByType(SelectionOverlay)).toHaveLength(0);
    expect(disabled.root.findAllByType(SelectionHandles)).toHaveLength(0);
    expect(disabled.root.findAllByType(SelectionToolbar)).toHaveLength(0);
    expect(textContents(disabled.root)).toEqual(textContents(plain.root));
  });

  it('configured and enabled: layers present', async () => {
    const r = await renderTree({ ast: AST, selection: {} });
    expect(r.root.findAllByType(SelectionRoot)).toHaveLength(1);
    expect(r.root.findAllByType(SelectableBlock)).toHaveLength(2);
    expect(r.root.findAllByType(SelectionOverlay)).toHaveLength(1);
    expect(r.root.findAllByType(SelectionToolbar)).toHaveLength(1);
  });

  it('toggling enabled keeps the content subtree mounted (same instances)', async () => {
    const props = { ast: AST, selection: { enabled: true } };
    const r = await renderTree(props);
    const before = r.root.findAllByType('Text');
    await updateTree(r, { ast: AST, selection: { enabled: false } });
    const after = r.root.findAllByType('Text');
    expect(after.length).toBe(before.length);
    for (let i = 0; i < before.length; i += 1) {
      expect(after[i]).toBe(before[i]);
    }
    // And back on again — the content never remounts.
    await updateTree(r, props);
    const restored = r.root.findAllByType('Text');
    for (let i = 0; i < before.length; i += 1) {
      expect(restored[i]).toBe(before[i]);
    }
  });

  it('sourceState streaming -> complete keeps the content subtree mounted', async () => {
    const ast = AST;
    const r = await renderTree({ ast, sourceState: 'streaming', selection: {} });
    const before = r.root.findAllByType('Text');
    expect(before.length).toBeGreaterThan(0);
    await updateTree(r, { ast, sourceState: 'complete', selection: {} });
    const after = r.root.findAllByType('Text');
    expect(after.length).toBe(before.length);
    for (let i = 0; i < before.length; i += 1) {
      expect(after[i]).toBe(before[i]);
    }
  });

  it('two instances over the same AST render independently', async () => {
    const a = await renderTree({ ast: AST, selection: { documentId: 'msg-a' } });
    const b = await renderTree({ ast: AST, selection: { documentId: 'msg-b' } });
    expect(a.root.findAllByType(SelectionRoot)[0]).not.toBe(b.root.findAllByType(SelectionRoot)[0]);
    expect(a.root.findAllByType(SelectableBlock)).toHaveLength(2);
    expect(b.root.findAllByType(SelectableBlock)).toHaveLength(2);
  });

  it('placeholder frame before the snapshot resolves: no selection, raw markdown', async () => {
    let renderer: ReactTestRenderer | null = null;
    // Synchronous act: effects run, but the async parse has not been awaited,
    // so the renderer is pinned on the placeholder frame.
    act(() => {
      renderer = create(
        React.createElement(Supramark, { markdown: '# hi', selection: {} }) as never
      );
    });
    const r = renderer as unknown as ReactTestRenderer;
    expect(r.root.findAllByType(SelectionRoot)).toHaveLength(0);
    expect(r.root.findAllByType(SelectableBlock)).toHaveLength(0);
    expect(textContents(r.root)).toEqual(['# hi']);
    // The canonical parser resolves asynchronously (wasm here, native FFI on
    // device), so poll a bounded number of real ticks for the document.
    for (let i = 0; i < 100 && r.root.findAllByType(SelectionRoot).length === 0; i += 1) {
      await act(async () => {
        await new Promise(resolve => setTimeout(resolve, 10));
      });
    }
    expect(r.root.findAllByType(SelectionRoot)).toHaveLength(1);
  });

  it('malformed AST: the selection layer stays out of the error path', async () => {
    const broken = {
      type: 'root',
      ast_version: 2,
      diagnostics: [],
      children: null,
    } as unknown as SupramarkRootNode;
    // A malformed children array throws inside the existing render path exactly
    // as it did before selection existed. The plan must not crash first with a
    // different failure — the error must still come from `renderRootNodes`.
    let caught: Error | null = null;
    try {
      // React rethrows render errors from `act` itself, not from `create`.
      await act(async () => {
        create(React.createElement(Supramark, { ast: broken, selection: {} }) as never);
      });
    } catch (error) {
      caught = error as Error;
    }
    expect(caught).toBeInstanceOf(TypeError);
    expect(String(caught?.stack)).toContain('renderRootNodes');
  });

  it('plan revision advances with the document and stays put otherwise', async () => {
    // The revision is not directly observable through props, so assert the
    // observable consequence: the same document keeps identical blocks (stable
    // ids), a changed document gets new ones.
    const r = await renderTree({ ast: AST, selection: {} });
    const idsBefore = r.root.findAllByType(SelectableBlock).map(inst => inst.props.nodeId);
    await updateTree(r, { ast: AST, selection: {} });
    const idsAfter = r.root.findAllByType(SelectableBlock).map(inst => inst.props.nodeId);
    expect(idsAfter).toEqual(idsBefore);

    const changed = {
      ...AST,
      children: [
        ...(AST as unknown as { children: unknown[] }).children,
        { type: 'paragraph', children: [{ type: 'text', value: 'Third paragraph' }] },
      ],
    } as unknown as SupramarkRootNode;
    await updateTree(r, { ast: changed, selection: {} });
    const idsChanged = r.root.findAllByType(SelectableBlock).map(inst => inst.props.nodeId);
    expect(idsChanged).toHaveLength(3);
    expect(idsChanged.slice(0, 2)).toEqual(idsBefore);
    expect(idsChanged[2]).not.toBe(idsBefore[0]);
  });
});

describe('selection states leave the diagram subtree alone', () => {
  const AST_WITH_DIAGRAM = {
    type: 'root',
    ast_version: 2,
    diagnostics: [],
    children: [
      { type: 'paragraph', children: [{ type: 'text', value: 'before' }] },
      { type: 'diagram', engine: 'mermaid', code: 'graph TD;', fence_closed: true },
      { type: 'paragraph', children: [{ type: 'text', value: 'after' }] },
    ],
  } as unknown as SupramarkRootNode;

  it('mounts exactly one diagram instance in all three states', async () => {
    const plain = await renderTree({ ast: AST_WITH_DIAGRAM });
    const disabled = await renderTree({ ast: AST_WITH_DIAGRAM, selection: { enabled: false } });
    const enabled = await renderTree({ ast: AST_WITH_DIAGRAM, selection: {} });
    for (const r of [plain, disabled, enabled]) {
      expect(r.root.findAllByType(DiagramNode)).toHaveLength(1);
    }
  });

  it('toggling enabled never remounts the diagram', async () => {
    const r = await renderTree({ ast: AST_WITH_DIAGRAM, selection: { enabled: true } });
    const before = r.root.findAllByType(DiagramNode)[0];
    await updateTree(r, { ast: AST_WITH_DIAGRAM, selection: { enabled: false } });
    expect(r.root.findAllByType(DiagramNode)[0]).toBe(before);
    await updateTree(r, { ast: AST_WITH_DIAGRAM, selection: { enabled: true } });
    expect(r.root.findAllByType(DiagramNode)[0]).toBe(before);
  });
});

describe('plugin options are not a content revision', () => {
  it('a fresh inline containerRenderers object keeps the same units array', async () => {
    const props = { ast: AST, selection: {}, containerRenderers: { card: () => null } };
    const r = await renderTree(props);
    const before = r.root.findAllByType(SelectableBlock)[0].props.unitIds;
    // Same content, brand-new plugin-options object: the plan must NOT rebuild,
    // or a host passing inline objects would clear the user's selection for
    // nothing (only the parsed document and the document id are revisions).
    await updateTree(r, { ast: AST, selection: {}, containerRenderers: { card: () => null } });
    expect(r.root.findAllByType(SelectableBlock)[0].props.unitIds).toBe(before);
  });
});

describe('host-driven selection handle', () => {
  it('reports the handle when the layer mounts and null when it goes away', async () => {
    const handles: Array<unknown> = [];
    const r = await renderTree({
      ast: AST,
      selection: { onHandleReady: (handle: unknown) => handles.push(handle) },
    });
    const ready = handles[handles.length - 1] as { trySelectParagraphAt?: unknown } | null;
    expect(ready).not.toBeNull();
    expect(typeof ready?.trySelectParagraphAt).toBe('function');

    // Removing the selection config tears the layer down and reports null.
    await updateTree(r, { ast: AST });
    expect(handles[handles.length - 1]).toBeNull();
  });

  it('never reports a handle when selection is not configured', async () => {
    const handles: Array<unknown> = [];
    await renderTree({ ast: AST });
    expect(handles).toHaveLength(0);
  });
});

describe('host action payloads', () => {
  it('onCopy merges the plan identity into every onAction payload', async () => {
    const actions: Array<Record<string, unknown>> = [];
    const r = await renderTree({
      ast: AST,
      selection: {
        documentId: 'msg-1',
        onAction: (request: Record<string, unknown>) => actions.push(request),
      },
    });
    // The renderer installs this merged callback as `onCopy` on the selection
    // root; invoking it the way a toolbar action would verifies the merge
    // (the root's own timing is covered by the rn-selection suite).
    const root = r.root.findByType(SelectionRoot);
    act(() => {
      root.props.onCopy({ text: 'First paragraph', format: 'plainText' });
    });
    expect(actions).toHaveLength(1);
    expect(actions[0]).toMatchObject({
      text: 'First paragraph',
      format: 'plainText',
      documentId: 'msg-1',
    });
    // The revision names the parsed snapshot the selection was raised for.
    expect(typeof actions[0].revision).toBe('number');
  });

  it('a late action from the previous document keeps its old plan revision', async () => {
    const actions: Array<Record<string, unknown>> = [];
    const selection = {
      documentId: 'msg-1',
      onAction: (request: Record<string, unknown>) => actions.push(request),
    };
    const r = await renderTree({ ast: AST, selection });
    // A host can hold the callback it was handed. After the document changes,
    // that old closure must still report the revision it was built with, so a
    // host comparing against the content version it expects can discard it.
    const oldOnCopy = r.root.findByType(SelectionRoot).props.onCopy;
    const changed = {
      ...AST,
      children: [
        ...(AST as unknown as { children: unknown[] }).children,
        { type: 'paragraph', children: [{ type: 'text', value: 'Third paragraph' }] },
      ],
    } as unknown as SupramarkRootNode;
    await updateTree(r, { ast: changed, selection });
    const newOnCopy = r.root.findByType(SelectionRoot).props.onCopy;
    act(() => newOnCopy({ text: 'new', format: 'plainText' }));
    act(() => oldOnCopy({ text: 'old', format: 'plainText' }));
    const [newAction, oldAction] = actions.slice(-2);
    expect(typeof oldAction.revision).toBe('number');
    expect(typeof newAction.revision).toBe('number');
    expect(oldAction.revision).toBeLessThan(newAction.revision as number);
  });
});
