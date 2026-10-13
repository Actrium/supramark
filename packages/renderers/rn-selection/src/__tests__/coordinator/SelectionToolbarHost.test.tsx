import { afterEach, describe, expect, mock, test } from 'bun:test';
import React, { useEffect } from 'react';
import { act, create, type ReactTestRenderer, type TestInstance } from 'react-test-renderer';
import type { SupramarkNode } from '@supramark/core';
import type { SelectionUnit } from '../../model';
import type { SelectionStore } from '../../coordinator/state';
import type { SelectionToolbarItem } from '../../coordinator/toolbar';

mock.module('react-native', () => ({
  Platform: { OS: 'android' },
  View: 'View',
  Text: 'Text',
  ScrollView: 'ScrollView',
  TouchableOpacity: 'TouchableOpacity',
  PanResponder: {
    create: (config: Record<string, unknown>) => ({
      panHandlers: {
        onStartShouldSetResponder: config.onStartShouldSetPanResponder,
        onStartShouldSetResponderCapture: config.onStartShouldSetPanResponderCapture,
        onMoveShouldSetResponder: config.onMoveShouldSetPanResponder,
        onMoveShouldSetResponderCapture: config.onMoveShouldSetPanResponderCapture,
        onResponderGrant: config.onPanResponderGrant,
        onResponderMove: config.onPanResponderMove,
        onResponderRelease: config.onPanResponderRelease,
        onResponderTerminate: config.onPanResponderTerminate,
        onResponderTerminationRequest: config.onPanResponderTerminationRequest,
        onShouldBlockNativeResponder: config.onShouldBlockNativeResponder,
      },
    }),
  },
  StyleSheet: { absoluteFill: {}, create: (s: unknown) => s },
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT =
  true;

const { SelectionRoot } = await import('../../coordinator/SelectionRoot');
const { SelectionToolbarHost } = await import('../../coordinator/SelectionToolbarHost');
const { SelectionToolbarView } = await import('../../coordinator/SelectionToolbar');
const { useSelectionContext } = await import('../../coordinator/useDocumentSelection');

const NODE = { type: 'text', value: 'hello' } as SupramarkNode;

const UNITS: SelectionUnit[] = [
  {
    kind: 'text',
    unitId: 'p1#0',
    nodeId: 'p1',
    text: 'hello',
    node: NODE,
  },
];

const COPY_ITEM: SelectionToolbarItem = { id: 'copy', title: 'Copy', format: 'plainText' };

// Window frames answered by the mocked measureInWindow; tests rewrite them before mounting.
let hostWindowRect = { x: 0, y: 50, w: 360, h: 600 };
let anchorWindowRect = { x: 100, y: 200, w: 20, h: 20 };
// 'hold' parks measurement callbacks so a pending refresh can be observed
// mid-flight, before its frame state lands.
let measureMode: 'auto' | 'hold' = 'auto';
const heldMeasures: Array<{
  answer: (x: number, y: number, w: number, h: number) => void;
  rect: [number, number, number, number];
}> = [];

function createNodeMock(element: { type: unknown; props: Record<string, unknown> }) {
  if (element.type !== 'View') return null;
  const isHost = element.props.testID === 'toolbar-host';
  return {
    measureInWindow: (answer: (x: number, y: number, w: number, h: number) => void) => {
      const frame = isHost ? hostWindowRect : anchorWindowRect;
      if (measureMode === 'hold') heldMeasures.push({ answer, rect: [frame.x, frame.y, frame.w, frame.h] });
      else answer(frame.x, frame.y, frame.w, frame.h);
    },
  };
}

function releaseHeldMeasures(): void {
  while (heldMeasures.length > 0) {
    const { answer, rect } = heldMeasures.shift()!;
    answer(rect[0], rect[1], rect[2], rect[3]);
  }
}

// Each released callback resumes one await inside the host's refresh, which may
// itself request the next measurement; release until the queue stays empty.
async function drainHeldMeasures(): Promise<void> {
  await act(async () => {
    while (heldMeasures.length > 0) {
      releaseHeldMeasures();
      await new Promise(resolve => setTimeout(resolve, 0));
    }
  });
}

let renderer: ReactTestRenderer | null = null;

afterEach(() => {
  if (renderer !== null) {
    act(() => {
      renderer?.unmount();
    });
  }
  renderer = null;
  measureMode = 'auto';
  heldMeasures.length = 0;
  hostWindowRect = { x: 0, y: 50, w: 360, h: 600 };
  anchorWindowRect = { x: 100, y: 200, w: 20, h: 20 };
});

interface HostedHarness {
  store: () => SelectionStore;
  context: () => ReturnType<typeof useSelectionContext>;
}

/** One short message inside a portal host, with its block and store captured. */
async function mountHostedSelection(): Promise<HostedHarness> {
  let store: SelectionStore | null = null;
  let context: ReturnType<typeof useSelectionContext> | null = null;

  function Block() {
    const ctx = useSelectionContext();
    useEffect(() => {
      store = ctx.store;
      context = ctx;
      return ctx.registerBlock({
        nodeId: 'p1',
        unitIds: ['p1#0'],
        kind: 'text',
        rect: { x: 0, y: 0, w: 20, h: 20 },
      });
    }, [ctx]);
    return null;
  }

  await act(async () => {
    renderer = create(
      <SelectionToolbarHost testID="toolbar-host">
        <SelectionRoot
          units={UNITS}
          handles={false}
          overlay={false}
          toolbarItems={[COPY_ITEM]}
          onCopy={() => undefined}
        >
          <Block />
        </SelectionRoot>
      </SelectionToolbarHost>,
      { createNodeMock }
    );
    await Promise.resolve();
  });
  if (store === null || context === null) {
    throw new Error('test did not capture the selection context');
  }
  const capturedStore: SelectionStore = store;
  const capturedContext = context;
  return { store: () => capturedStore, context: () => capturedContext };
}

async function selectAll(harness: HostedHarness): Promise<void> {
  await act(async () => {
    harness.store().beginAt({ nodeId: 'p1', unitId: 'p1#0', offset: 0 });
    harness.store().extendTo({ nodeId: 'p1', unitId: 'p1#0', offset: 5 });
    harness.store().commit();
  });
}

async function feedMenuLayout(width: number, height: number): Promise<void> {
  const button = renderer!.root.findByType('TouchableOpacity');
  await act(async () =>
    button.parent!.props.onLayout({ nativeEvent: { layout: { width, height } } })
  );
}

function hostedToolbarView(): TestInstance {
  return renderer!.root.findByType(SelectionToolbarView);
}

describe('SelectionToolbarHost portal', () => {
  test('a clear and republish inside one commit never unmounts the hosted toolbar', async () => {
    const harness = await mountHostedSelection();
    await selectAll(harness);
    await feedMenuLayout(64, 36);
    // The first publish rendered the bar through the portal at the initial geometry.
    expect(renderer!.root.findAllByType('TouchableOpacity')).toHaveLength(1);
    expect(hostedToolbarView().props.rects).toEqual([{ x: 100, y: 150, w: 20, h: 20 }]);
    expect(hostedToolbarView().children[0].props.style.opacity).toBe(1);

    // Park every measurement, then bump the registry: the toolbar's publish
    // effect re-runs, and its cleanup (clear) and body (republish) hit the
    // portal back to back within the same commit.
    measureMode = 'hold';
    await act(async () => {
      harness.context().updateLayout('p1', { x: 0, y: 40, w: 20, h: 20 });
    });
    // Let the deferred clear's microtask run while the republished measurement
    // is still parked: the version check must drop it instead of clearing frames.
    await act(async () => {
      await new Promise(resolve => setTimeout(resolve, 0));
    });

    // The bar from the previous commit is still on the tree with its old geometry.
    expect(renderer!.root.findAllByType('TouchableOpacity')).toHaveLength(1);
    expect(hostedToolbarView().props.rects).toEqual([{ x: 100, y: 150, w: 20, h: 20 }]);

    // Releasing the parked measurements applies the republished entry in place.
    await drainHeldMeasures();
    expect(renderer!.root.findAllByType('TouchableOpacity')).toHaveLength(1);
    expect(hostedToolbarView().props.rects).toEqual([{ x: 100, y: 190, w: 20, h: 20 }]);
    // Without re-feeding the menu layout the bar is already placed, so the
    // frame update reused the mounted view instead of remounting it.
    expect(hostedToolbarView().children[0].props.style.opacity).toBe(1);
  });

  test('clearing the selection unmounts the hosted toolbar', async () => {
    const harness = await mountHostedSelection();
    await selectAll(harness);
    await feedMenuLayout(64, 36);
    expect(renderer!.root.findAllByType('TouchableOpacity')).toHaveLength(1);

    await act(async () => {
      harness.store().clear();
    });
    // The publish effect's cleanup and body both clear; the surviving deferred
    // clear is the one that empties the frame list.
    await act(async () => {
      await new Promise(resolve => setTimeout(resolve, 0));
    });

    expect(renderer!.root.findAllByType('TouchableOpacity')).toHaveLength(0);
    const overlay = renderer!.root
      .findAllByType('View' as unknown as React.ElementType)
      .find(node => (node.props.style as { zIndex?: number } | undefined)?.zIndex === 1000);
    expect(overlay).toBeDefined();
    expect(overlay!.children).toHaveLength(0);
  });

  test('without a host the toolbar renders locally inside SelectionRoot and stays actionable', async () => {
    let store: SelectionStore | null = null;
    const copies: string[] = [];

    function Block() {
      const ctx = useSelectionContext();
      useEffect(() => {
        store = ctx.store;
        return ctx.registerBlock({
          nodeId: 'p1',
          unitIds: ['p1#0'],
          kind: 'text',
          rect: { x: 10, y: 100, w: 60, h: 20 },
        });
      }, [ctx]);
      return null;
    }

    await act(async () => {
      renderer = create(
        <SelectionRoot
          units={UNITS}
          overlay={false}
          handles={false}
          toolbarItems={[COPY_ITEM]}
          onCopy={request => copies.push(request.text)}
        >
          <Block />
        </SelectionRoot>
      );
      await Promise.resolve();
    });
    if (store === null) throw new Error('test did not capture the selection store');
    // Nothing is rendered before a selection exists.
    expect(renderer!.root.findAllByType('TouchableOpacity')).toHaveLength(0);

    await act(async () => {
      store!.beginAt({ nodeId: 'p1', unitId: 'p1#0', offset: 0 });
      store!.extendTo({ nodeId: 'p1', unitId: 'p1#0', offset: 5 });
      store!.commit();
    });
    const button = renderer!.root.findByType('TouchableOpacity');
    // No portal is mounted, so the bar lives in the root's own subtree.
    const root = renderer!.root.findAllByType('View' as unknown as React.ElementType)[0];
    let ancestor: TestInstance | null = button;
    while (ancestor !== null && ancestor !== root) ancestor = ancestor.parent;
    expect(ancestor).toBe(root);

    // Feeding the root's own viewport places the locally rendered bar.
    const rootView = renderer!.root.findAllByType('View' as unknown as React.ElementType)[0];
    await act(async () =>
      rootView.props.onLayout({ nativeEvent: { layout: { width: 360, height: 600 } } })
    );
    await feedMenuLayout(64, 36);
    expect(button.parent!.parent!.props.style.opacity).toBe(1);

    await act(async () => button.props.onPress());
    expect(copies).toEqual(['hello']);
    expect(store.getSnapshot().phase).toBe('idle');
    expect(store.getSnapshot().range).toBeNull();
    expect(renderer!.root.findAllByType('TouchableOpacity')).toHaveLength(0);
  });

  test('a host offset from its document root translates portal geometry by root minus host', async () => {
    hostWindowRect = { x: 30, y: 50, w: 360, h: 600 };
    const harness = await mountHostedSelection();
    await selectAll(harness);
    await feedMenuLayout(64, 36);

    const view = hostedToolbarView();
    // Root-space rects (0,0,20,20) moved by the root's window frame (100,200)
    // relative to the host's own window frame (30,50).
    expect(view.props.rects).toEqual([{ x: 70, y: 150, w: 20, h: 20 }]);
    expect(view.props.viewport).toEqual({ x: 0, y: 0, w: 360, h: 600 });
    const positioned = view.children[0];
    // centred on x=80 inside a 360-wide host: 80 - 32 clamped to the margins.
    expect(positioned.props.style.left).toBe(48);
    expect(positioned.props.style.width).toBe(344);
    expect(positioned.props.style.opacity).toBe(1);
  });
});


describe('toolbar hit testing while hidden', () => {
  test('unmeasured and offscreen menus do not intercept touches', async () => {
    let tree: ReactTestRenderer;
    const render = (y: number) => (
      <SelectionToolbarView
        viewport={{ w: 360, h: 600 }}
        rects={[{ x: 20, y, w: 80, h: 20 }]}
        items={[COPY_ITEM]}
        run={() => {}}
      />
    );
    await act(async () => { tree = create(render(100)); });
    const surface = () => tree.root.findAllByType('View')[0];
    expect(surface().props.pointerEvents).toBe('none');
    await act(async () => {
      tree.root.findAllByType('View')[1].props.onLayout({
        nativeEvent: { layout: { width: 80, height: 40 } },
      });
    });
    expect(surface().props.pointerEvents).toBe('box-none');
    await act(async () => { tree.update(render(-100)); });
    expect(surface().props.style.opacity).toBe(0);
    expect(surface().props.pointerEvents).toBe('none');
    await act(async () => { tree.unmount(); });
  });
});
