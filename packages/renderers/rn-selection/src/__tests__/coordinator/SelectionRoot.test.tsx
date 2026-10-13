import { afterEach, describe, expect, mock, test } from 'bun:test';
import React, { useEffect } from 'react';
import { create, act, type ReactTestRenderer } from 'react-test-renderer';
import type { SupramarkNode } from '@supramark/core';
import type { SelectionUnit } from '../../model';
import type { SelectionStore } from '../../coordinator/state';
import type { SelectionRootHandle } from '../../coordinator/SelectionRoot';

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
const { useEffect: _useEffect } = await import('react');
const { SelectionViewport } = await import('../../coordinator/SelectionViewport');
const { DEFAULT_LONG_PRESS_MS } = await import('../../coordinator/gesture');
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


function eventAt(x: number, y: number, location = { x: -500, y: -500 }, timestamp?: number) {
  return {
    nativeEvent: {
      // Deliberately child-space. The root must ignore locationX/Y because RN
      // reports them relative to the deepest target, not SelectionRoot.
      locationX: location.x,
      locationY: location.y,
      pageX: x,
      pageY: y,
      timestamp,
    },
  };
}

let renderer: ReactTestRenderer | null = null;

afterEach(() => {
  if (renderer !== null) {
    act(() => {
      renderer?.unmount();
    });
  }
  renderer = null;
});

function renderRootWithBlock(
  props: { gestures?: boolean; longPressMs?: number; selected?: boolean; handles?: boolean } = {},
  options?: Parameters<typeof create>[1]
) {
  let store: SelectionStore | null = null;

  const RegisterBlock: React.FC = () => {
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
  };

  let r!: ReactTestRenderer;
  act(() => {
    r = create(
      <SelectionRoot
        units={UNITS}
        overlay={false}
        handles={props.handles}
        toolbar={false}
        gestures={props.gestures}
        longPressMs={props.longPressMs}
      >
        <RegisterBlock />
      </SelectionRoot>,
      options
    );
  });
  renderer = r;

  if (store === null) throw new Error('test did not capture the selection store');
  if (props.selected !== false) {
    act(() => {
      store.beginAt({ nodeId: 'p1', unitId: 'p1#0', offset: 0 });
      store.extendTo({ nodeId: 'p1', unitId: 'p1#0', offset: 5 });
      store.commit();
    });
  }

  return { root: r.root.findAllByType('View' as unknown as React.ElementType)[0], store };
}

describe('SelectionRoot responder negotiation', () => {
  test('captures handle touches before a nested native scroller can take them', () => {
    const { root } = renderRootWithBlock();

    expect(root.props.onStartShouldSetResponder(eventAt(10, 94))).toBe(false);
    expect(root.props.onStartShouldSetResponderCapture(eventAt(10, 94))).toBe(true);
  });

  test('observes a block press without claiming it from a nested scroller', () => {
    const { root } = renderRootWithBlock({ selected: false });

    act(() => {
      root.props.onTouchStart(eventAt(40, 110, undefined, 1000));
    });
    expect(root.props.onStartShouldSetResponder(eventAt(40, 110))).toBe(false);
    expect(root.props.onStartShouldSetResponderCapture(eventAt(40, 110))).toBe(false);
    expect(root.props.onShouldBlockNativeResponder()).toBe(false);
  });

  test('does not claim touches outside selectable blocks so nested scroll views can scroll', () => {
    const { root } = renderRootWithBlock({ selected: false });

    expect(root.props.onStartShouldSetResponder(eventAt(40, 200))).toBe(false);
  });

  test('honours gestures=false even over a handle', () => {
    const { root } = renderRootWithBlock({ gestures: false });

    expect(root.props.onStartShouldSetResponder(eventAt(10, 94))).toBe(false);
  });

  test('long press resolves from page coordinates, not child location', async () => {
    const { root, store } = renderRootWithBlock({ longPressMs: 1, selected: false });

    await act(async () => {
      root.props.onResponderGrant(eventAt(40, 110));
      await new Promise(resolve => setTimeout(resolve, 5));
    });

    const snapshot = store.getSnapshot();
    expect(snapshot.phase).toBe('selected');
    expect(snapshot.range).toEqual({
      anchor: { nodeId: 'p1', unitId: 'p1#0', offset: 0 },
      focus: { nodeId: 'p1', unitId: 'p1#0', offset: 5 },
    });
  });

  test('blank long press inside a wide block but outside its text line clears selection', async () => {
    let store: SelectionStore | null = null;

    const RegisterWideBlock: React.FC = () => {
      const ctx = useSelectionContext();
      useEffect(() => {
        store = ctx.store;
        return ctx.registerBlock({
          nodeId: 'p1',
          unitIds: ['p1#0'],
          kind: 'text',
          rect: { x: 10, y: 100, w: 300, h: 20 },
          metrics: {
            textLength: 5,
            lines: [{ start: 0, end: 5, visibleEnd: 5, x: 0, y: 0, w: 50, h: 20 }],
          },
        });
      }, [ctx]);
      return null;
    };

    act(() => {
      renderer = create(
        <SelectionRoot
          units={UNITS}
          overlay={false}
          handles={false}
          toolbar={false}
          longPressMs={1}
        >
          <RegisterWideBlock />
        </SelectionRoot>
      );
    });
    if (store === null) throw new Error('test did not capture the selection store');
    act(() => {
      store.beginAt({ nodeId: 'p1', unitId: 'p1#0', offset: 0 });
      store.extendTo({ nodeId: 'p1', unitId: 'p1#0', offset: 5 });
      store.commit();
    });

    const root = renderer.root.findByType('View' as unknown as React.ElementType);
    await act(async () => {
      root.props.onResponderGrant(eventAt(250, 110));
      await new Promise(resolve => setTimeout(resolve, 5));
    });

    expect(store.getSnapshot().range).toBeNull();
    expect(store.getSnapshot().phase).toBe('idle');
  });

  test('long press picks the character under the finger rather than the nearest edge', async () => {
    let store: SelectionStore | null = null;
    const text = '!!!!';
    const units: SelectionUnit[] = [
      {
        kind: 'text',
        unitId: 'punct#0',
        nodeId: 'punct',
        text,
        node: NODE,
      },
    ];

    const RegisterPunctuationBlock: React.FC = () => {
      const ctx = useSelectionContext();
      useEffect(() => {
        store = ctx.store;
        return ctx.registerBlock({
          nodeId: 'punct',
          unitIds: ['punct#0'],
          kind: 'text',
          rect: { x: 10, y: 100, w: 40, h: 20 },
          metrics: {
            textLength: text.length,
            lines: [{ start: 0, end: 4, visibleEnd: 4, x: 0, y: 0, w: 40, h: 20 }],
          },
        });
      }, [ctx]);
      return null;
    };

    act(() => {
      renderer = create(
        <SelectionRoot
          units={units}
          overlay={false}
          handles={false}
          toolbar={false}
          longPressMs={1}
        >
          <RegisterPunctuationBlock />
        </SelectionRoot>
      );
    });
    if (store === null) throw new Error('test did not capture the selection store');

    const root = renderer.root.findByType('View' as unknown as React.ElementType);
    await act(async () => {
      root.props.onResponderGrant(eventAt(39, 110));
      await new Promise(resolve => setTimeout(resolve, 5));
    });

    expect(store.getSnapshot().range).toEqual({
      anchor: { nodeId: 'punct', unitId: 'punct#0', offset: 2 },
      focus: { nodeId: 'punct', unitId: 'punct#0', offset: 3 },
    });
  });

  test('touchEnd commits when Android does not deliver responderRelease', async () => {
    const { root, store } = renderRootWithBlock({ longPressMs: 1, selected: false });

    await act(async () => {
      root.props.onResponderGrant(eventAt(40, 110));
      await new Promise(resolve => setTimeout(resolve, 5));
    });
    act(() => {
      root.props.onResponderMove(eventAt(80, 110));
    });

    expect(store.getSnapshot().phase).toBe('selecting');

    act(() => {
      root.props.onTouchEnd(eventAt(80, 110));
    });

    expect(store.getSnapshot().phase).toBe('selected');
  });

  test('uses native event timestamps when Android flushes press callbacks late', () => {
    const { root, store } = renderRootWithBlock({ selected: false });

    act(() => {
      root.props.onResponderGrant(eventAt(40, 110, { x: -500, y: -500 }, 1000));
      root.props.onTouchEnd(eventAt(40, 110, { x: -500, y: -500 }, 1500));
    });

    const snapshot = store.getSnapshot();
    expect(snapshot.phase).toBe('selected');
    expect(snapshot.range).toEqual({
      anchor: { nodeId: 'p1', unitId: 'p1#0', offset: 0 },
      focus: { nodeId: 'p1', unitId: 'p1#0', offset: 5 },
    });
  });

  test('arms the root timer with the gesture default long-press threshold', () => {
    const { root } = renderRootWithBlock({ selected: false });
    const originalSetTimeout = globalThis.setTimeout;
    const originalClearTimeout = globalThis.clearTimeout;
    const delays: number[] = [];

    globalThis.setTimeout = ((...args: Parameters<typeof setTimeout>) => {
      delays.push(Number(args[1] ?? 0));
      return 1 as unknown as ReturnType<typeof setTimeout>;
    }) as typeof setTimeout;
    globalThis.clearTimeout = (() => undefined) as typeof clearTimeout;

    try {
      act(() => {
        root.props.onResponderGrant(eventAt(40, 110));
      });
    } finally {
      globalThis.setTimeout = originalSetTimeout;
      globalThis.clearTimeout = originalClearTimeout;
    }

    expect(delays).toEqual([DEFAULT_LONG_PRESS_MS]);
  });

  test('a pending press never blocks an enclosing scroll view', () => {
    const { root } = renderRootWithBlock({ selected: false });

    act(() => {
      root.props.onResponderGrant(eventAt(40, 110));
    });

    expect(root.props.onResponderTerminationRequest(eventAt(40, 110), { dx: 1, dy: 1 })).toBe(true);
    expect(root.props.onResponderTerminationRequest(eventAt(40, 130), { dx: 0, dy: 20 })).toBe(
      true
    );
  });

  test('a nested Android touch cancel still completes a stationary long press', async () => {
    const { root, store } = renderRootWithBlock({ selected: false, longPressMs: 1 });

    act(() => {
      root.props.onTouchStart(eventAt(40, 110));
      root.props.onTouchCancel();
    });

    await act(async () => {
      await new Promise(resolve => setTimeout(resolve, 5));
    });

    expect(store.getSnapshot().phase).toBe('selected');
    expect(root.props.onShouldBlockNativeResponder()).toBe(false);
  });

  test('a viewport scroll cancels a pending long press after native touch cancellation', async () => {
    let store: SelectionStore | null = null;

    const RegisterBlock: React.FC = () => {
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
    };

    act(() => {
      renderer = create(
        <SelectionRoot
          units={UNITS}
          overlay={false}
          handles={false}
          toolbar={false}
          longPressMs={1}
        >
          <SelectionViewport style={{ height: 80 }}>
            {React.createElement('ScrollView', null, <RegisterBlock />)}
          </SelectionViewport>
        </SelectionRoot>
      );
    });
    if (store === null) throw new Error('test did not capture the selection store');
    const root = renderer.root.findAllByType('View' as unknown as React.ElementType)[0];
    const scrollView = renderer.root.findByType('ScrollView' as unknown as React.ElementType);

    act(() => {
      root.props.onTouchStart(eventAt(40, 110));
      root.props.onTouchCancel();
      scrollView.props.onScroll({ nativeEvent: { contentOffset: { x: 0, y: 20 } } });
    });

    await act(async () => {
      await new Promise(resolve => setTimeout(resolve, 5));
    });

    expect(store.getSnapshot().phase).toBe('idle');
    expect(store.getSnapshot().range).toBeNull();
  });

  test('blocks native responders only after a long press becomes selection', async () => {
    const { root } = renderRootWithBlock({ selected: false, longPressMs: 1 });

    expect(root.props.onShouldBlockNativeResponder()).toBe(false);

    act(() => {
      root.props.onResponderGrant(eventAt(40, 110));
    });
    expect(root.props.onShouldBlockNativeResponder()).toBe(false);

    await act(async () => {
      await new Promise(resolve => setTimeout(resolve, 5));
    });
    expect(root.props.onShouldBlockNativeResponder()).toBe(true);

    act(() => {
      root.props.onResponderRelease(eventAt(40, 110));
    });
    expect(root.props.onShouldBlockNativeResponder()).toBe(false);
  });

  test('uses native measure page origin in the same coordinate space as touch pageX/Y', async () => {
    let store: SelectionStore | null = null;

    const RegisterMeasuredBlock: React.FC = () => {
      const ctx = useSelectionContext();
      useEffect(() => {
        store = ctx.store;
        return ctx.registerBlock({
          nodeId: 'p1',
          unitIds: ['p1#0'],
          kind: 'text',
          rect: { x: 10, y: 20, w: 60, h: 20 },
        });
      }, [ctx]);
      return null;
    };

    let r!: ReactTestRenderer;
    act(() => {
      r = create(
        <SelectionRoot
          units={UNITS}
          overlay={false}
          handles={false}
          toolbar={false}
          longPressMs={1}
        >
          <RegisterMeasuredBlock />
        </SelectionRoot>,
        {
          createNodeMock: element =>
            element.type === 'View'
              ? {
                  measure: (cb: (...args: number[]) => void) => cb(0, 0, 100, 100, 30, 80),
                }
              : null,
        }
      );
    });
    renderer = r;
    if (store === null) throw new Error('test did not capture the selection store');
    const root = r.root.findByType('View' as unknown as React.ElementType);

    await act(async () => {
      root.props.onLayout({ nativeEvent: { layout: { width: 100, height: 100 } } });
      root.props.onResponderGrant(eventAt(40, 100));
      await new Promise(resolve => setTimeout(resolve, 5));
    });

    expect(store.getSnapshot().phase).toBe('selected');
  });

  test('does not wait for async native measure before arming the long-press timer', () => {
    const { root } = renderRootWithBlock(
      { selected: false },
      {
        createNodeMock: element =>
          element.type === 'View'
            ? {
                measure: () => undefined,
              }
            : null,
      }
    );
    const originalSetTimeout = globalThis.setTimeout;
    const originalClearTimeout = globalThis.clearTimeout;
    const delays: number[] = [];

    globalThis.setTimeout = ((...args: Parameters<typeof setTimeout>) => {
      delays.push(Number(args[1] ?? 0));
      return 1 as unknown as ReturnType<typeof setTimeout>;
    }) as typeof setTimeout;
    globalThis.clearTimeout = (() => undefined) as typeof clearTimeout;

    try {
      act(() => {
        root.props.onResponderGrant(eventAt(40, 110));
      });
    } finally {
      globalThis.setTimeout = originalSetTimeout;
      globalThis.clearTimeout = originalClearTimeout;
    }

    expect(delays).toEqual([DEFAULT_LONG_PRESS_MS]);
  });

  test('measures block layout against SelectionRoot when a native handle is available', () => {
    let measuredAgainstRoot = false;
    let registry: ReturnType<typeof useSelectionContext>['registry'] | null = null;
    const blockHandle = {
      measureLayout: (
        relativeToNativeNode: unknown,
        onSuccess: (x: number, y: number, width: number, height: number) => void
      ) => {
        measuredAgainstRoot = relativeToNativeNode !== null;
        onSuccess(7, 9, 30, 11);
      },
    };

    const RegisterMeasuredBlock: React.FC = () => {
      const ctx = useSelectionContext();
      useEffect(() => {
        registry = ctx.registry;
        const dispose = ctx.registerBlock({
          nodeId: 'p1',
          unitIds: ['p1#0'],
          kind: 'text',
        });
        ctx.measureLayout?.('p1', blockHandle, { x: 1, y: 2, w: 3, h: 4 });
        return dispose;
      }, [ctx]);
      return null;
    };

    act(() => {
      renderer = create(
        <SelectionRoot units={UNITS} overlay={false} handles={false} toolbar={false}>
          <RegisterMeasuredBlock />
        </SelectionRoot>,
        {
          createNodeMock: element => (element.type === 'View' ? { measure: () => {} } : null),
        }
      );
    });

    expect(measuredAgainstRoot).toBe(true);
    expect(registry?.getBlock('p1')?.rect).toEqual({ x: 7, y: 9, w: 30, h: 11 });
  });

  test('refreshLayouts remeasures cached block layout after nested scroll', () => {
    let registry: ReturnType<typeof useSelectionContext>['registry'] | null = null;
    let refreshLayouts: (() => void) | null = null;
    let nextRect = { x: 7, y: 9, w: 30, h: 11 };
    const blockHandle = {
      measureLayout: (
        _relativeToNativeNode: unknown,
        onSuccess: (x: number, y: number, width: number, height: number) => void
      ) => {
        onSuccess(nextRect.x, nextRect.y, nextRect.w, nextRect.h);
      },
    };

    const RegisterMeasuredBlock: React.FC = () => {
      const ctx = useSelectionContext();
      useEffect(() => {
        registry = ctx.registry;
        refreshLayouts = ctx.refreshLayouts ?? null;
        const dispose = ctx.registerBlock({
          nodeId: 'p1',
          unitIds: ['p1#0'],
          kind: 'text',
        });
        ctx.measureLayout?.('p1', blockHandle, { x: 1, y: 2, w: 3, h: 4 });
        return dispose;
      }, [ctx]);
      return null;
    };

    act(() => {
      renderer = create(
        <SelectionRoot units={UNITS} overlay={false} handles={false} toolbar={false}>
          <RegisterMeasuredBlock />
        </SelectionRoot>,
        {
          createNodeMock: element => (element.type === 'View' ? { measure: () => {} } : null),
        }
      );
    });

    expect(registry?.getBlock('p1')?.rect).toEqual({ x: 7, y: 9, w: 30, h: 11 });
    nextRect = { x: 17, y: 19, w: 31, h: 12 };
    act(() => {
      refreshLayouts?.();
    });
    expect(registry?.getBlock('p1')?.rect).toEqual({ x: 17, y: 19, w: 31, h: 12 });
  });

  test('pairs page measurements for nested scrollers without reading window coordinates', () => {
    let registry: ReturnType<typeof useSelectionContext>['registry'] | null = null;
    let refreshLayouts: (() => void) | null = null;
    let targetPage = { x: 37, y: 59, w: 30, h: 11 };
    const rootPage = { x: 30, y: 50 };
    const blockHandle = {
      // RN measure reports parent-local coordinates first and page coordinates last.
      measure: (onSuccess: (...args: number[]) => void) => {
        onSuccess(900, 900, targetPage.w, targetPage.h, targetPage.x, targetPage.y);
      },
      // Any accidental window measurement would mix this unrelated origin into the rect.
      measureInWindow: () => {
        throw new Error('selection geometry must use page coordinates');
      },
      measureLayout: (
        _relativeToNativeNode: unknown,
        onSuccess: (x: number, y: number, width: number, height: number) => void
      ) => {
        onSuccess(999, 999, 999, 999);
      },
    };

    const RegisterMeasuredBlock: React.FC = () => {
      const ctx = useSelectionContext();
      useEffect(() => {
        registry = ctx.registry;
        refreshLayouts = ctx.refreshLayouts ?? null;
        const dispose = ctx.registerBlock({
          nodeId: 'p1',
          unitIds: ['p1#0'],
          kind: 'text',
        });
        ctx.measureLayout?.('p1', blockHandle, { x: 1, y: 2, w: 3, h: 4 });
        return dispose;
      }, [ctx]);
      return null;
    };

    act(() => {
      renderer = create(
        <SelectionRoot units={UNITS} overlay={false} handles={false} toolbar={false}>
          <RegisterMeasuredBlock />
        </SelectionRoot>,
        {
          createNodeMock: element =>
            element.type === 'View'
              ? {
                  measure: (onSuccess: (...args: number[]) => void) =>
                    onSuccess(800, 800, 100, 100, rootPage.x, rootPage.y),
                  measureInWindow: () => {
                    throw new Error('root must use page coordinates');
                  },
                }
              : null,
        }
      );
    });

    expect(registry?.getBlock('p1')?.rect).toEqual({ x: 7, y: 9, w: 30, h: 11 });
    targetPage = { x: 37, y: 19, w: 31, h: 12 };
    act(() => {
      refreshLayouts?.();
    });
    expect(registry?.getBlock('p1')?.rect).toEqual({ x: 7, y: -31, w: 31, h: 12 });
  });

  test('running a toolbar action dismisses the selection menu', () => {
    let store: SelectionStore | null = null;
    let runToolbarItem: ReturnType<typeof useSelectionContext>['runToolbarItem'] | null = null;
    let copied = '';

    const RegisterBlockAndAction: React.FC = () => {
      const ctx = useSelectionContext();
      useEffect(() => {
        store = ctx.store;
        runToolbarItem = ctx.runToolbarItem;
        return ctx.registerBlock({
          nodeId: 'p1',
          unitIds: ['p1#0'],
          kind: 'text',
          rect: { x: 10, y: 100, w: 60, h: 20 },
        });
      }, [ctx]);
      return null;
    };

    act(() => {
      renderer = create(
        <SelectionRoot
          units={UNITS}
          overlay={false}
          handles={false}
          toolbar={false}
          onCopy={req => {
            copied = req.text;
          }}
        >
          <RegisterBlockAndAction />
        </SelectionRoot>
      );
    });
    if (store === null || runToolbarItem === null) {
      throw new Error('test did not capture the selection context');
    }
    act(() => {
      store.beginAt({ nodeId: 'p1', unitId: 'p1#0', offset: 0 });
      store.extendTo({ nodeId: 'p1', unitId: 'p1#0', offset: 5 });
      store.commit();
      runToolbarItem({ id: 'copy', title: 'Copy', format: 'plainText' });
    });

    expect(copied).toBe('hello');
    expect(store.getSnapshot().phase).toBe('idle');
    expect(store.getSnapshot().range).toBeNull();
  });


  test('handle drags resolve from page coordinates through the root origin', () => {
    let store: SelectionStore | null = null;

    const RegisterMeasuredBlock: React.FC = () => {
      const ctx = useSelectionContext();
      useEffect(() => {
        store = ctx.store;
        return ctx.registerBlock({
          nodeId: 'p1',
          unitIds: ['p1#0'],
          kind: 'text',
          rect: { x: 10, y: 100, w: 60, h: 20 },
          metrics: {
            textLength: 5,
            lines: [{ start: 0, end: 5, visibleEnd: 5, x: 0, y: 0, w: 50, h: 20 }],
          },
        });
      }, [ctx]);
      return null;
    };

    act(() => {
      renderer = create(
        <SelectionRoot units={UNITS} overlay={false} toolbar={false}>
          <RegisterMeasuredBlock />
        </SelectionRoot>,
        {
          createNodeMock: element =>
            element.type === 'View'
              ? {
                  measure: (cb: (...args: number[]) => void) => cb(0, 0, 300, 500, 100, 200),
                }
              : null,
        }
      );
    });
    if (store === null) throw new Error('test did not capture the selection store');

    const root = renderer.root.findAllByType('View' as unknown as React.ElementType)[0];
    act(() => {
      root.props.onLayout({ nativeEvent: { layout: { width: 300, height: 500 } } });
      store.beginAt({ nodeId: 'p1', unitId: 'p1#0', offset: 0 });
      store.extendTo({ nodeId: 'p1', unitId: 'p1#0', offset: 5 });
      store.commit();
    });

    const handleKnobs = renderer.root
      .findAllByType('View' as unknown as React.ElementType)
      .filter(node => {
        const style = node.props.style as {
          backgroundColor?: string;
          width?: number;
          height?: number;
        };
        return style?.backgroundColor === '#3399ff' && style.width === 12 && style.height === 12;
      });
    expect(handleKnobs).toHaveLength(2);

    const endKnob = [...handleKnobs].sort(
      (a, b) => (a.props.style.top as number) - (b.props.style.top as number)
    )[1];
    act(() => {
      endKnob.props.onResponderGrant(eventAt(160, 326));
      endKnob.props.onResponderMove(eventAt(130, 310), { dx: 999, dy: 999 });
    });

    expect(store.getSnapshot().range).toEqual({
      anchor: { nodeId: 'p1', unitId: 'p1#0', offset: 0 },
      focus: { nodeId: 'p1', unitId: 'p1#0', offset: 2 },
    });
  });

  test('a selection viewport pauses its scroller for the lifetime of a handle drag', () => {
    let store: SelectionStore | null = null;
    const activity: boolean[] = [];

    const RegisterBlock: React.FC = () => {
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
    };

    act(() => {
      renderer = create(
        <SelectionRoot
          units={UNITS}
          overlay={false}
          toolbar={false}
          onGestureActiveChange={active => activity.push(active)}
        >
          <SelectionViewport style={{ height: 80 }}>
            {React.createElement('ScrollView', null, <RegisterBlock />)}
          </SelectionViewport>
        </SelectionRoot>
      );
    });
    if (store === null) throw new Error('test did not capture the selection store');
    act(() => {
      store.beginAt({ nodeId: 'p1', unitId: 'p1#0', offset: 0 });
      store.extendTo({ nodeId: 'p1', unitId: 'p1#0', offset: 5 });
      store.commit();
    });

    const scrollView = () =>
      renderer?.root.findByType('ScrollView' as unknown as React.ElementType);
    expect(scrollView()?.props.scrollEnabled).toBe(true);

    const knob = renderer.root.findAllByType('View' as unknown as React.ElementType).find(node => {
      const style = node.props.style as { backgroundColor?: string; width?: number };
      return style?.backgroundColor === '#3399ff' && style.width === 12;
    });
    if (knob === undefined) throw new Error('test did not find a handle knob');

    act(() => {
      knob.props.onResponderGrant(eventAt(10, 94));
    });
    expect(scrollView()?.props.scrollEnabled).toBe(false);
    expect(activity).toEqual([true]);

    act(() => {
      knob.props.onResponderRelease(eventAt(10, 94), { dx: 0, dy: 0 });
    });
    expect(scrollView()?.props.scrollEnabled).toBe(true);
    expect(activity).toEqual([true, false]);
  });

  test('a handle that starts outside a viewport cannot select into it', () => {
    const scopedUnits: SelectionUnit[] = [
      {
        kind: 'text',
        unitId: 'outside#0',
        nodeId: 'outside',
        text: 'outer',
        node: NODE,
      },
      {
        kind: 'text',
        unitId: 'inside#0',
        nodeId: 'inside',
        text: 'inner',
        node: NODE,
      },
    ];
    let store: SelectionStore | null = null;

    const RegisterBlocks: React.FC = () => {
      const ctx = useSelectionContext();
      useEffect(() => {
        store = ctx.store;
        const disposeOutside = ctx.registerBlock({
          nodeId: 'outside',
          unitIds: ['outside#0'],
          kind: 'text',
          rect: { x: 10, y: 10, w: 60, h: 20 },
        });
        const disposeInside = ctx.registerBlock({
          nodeId: 'inside',
          unitIds: ['inside#0'],
          kind: 'text',
          rect: { x: 10, y: 100, w: 60, h: 20 },
          viewportId: 'list',
        });
        return () => {
          disposeOutside();
          disposeInside();
        };
      }, [ctx]);
      return null;
    };

    act(() => {
      renderer = create(
        <SelectionRoot units={scopedUnits} overlay={false} toolbar={false}>
          <RegisterBlocks />
        </SelectionRoot>
      );
    });
    if (store === null) throw new Error('test did not capture the selection store');
    act(() => {
      store.beginAt({ nodeId: 'outside', unitId: 'outside#0', offset: 0 });
      store.extendTo({ nodeId: 'outside', unitId: 'outside#0', offset: 5 });
      store.commit();
    });

    const knobs = renderer.root
      .findAllByType('View' as unknown as React.ElementType)
      .filter(node => {
        const style = node.props.style as {
          backgroundColor?: string;
          width?: number;
          top?: number;
        };
        return style?.backgroundColor === '#3399ff' && style.width === 12;
      })
      .sort(
        (a, b) =>
          ((a.props.style as { top: number }).top ?? 0) -
          ((b.props.style as { top: number }).top ?? 0)
      );
    const endKnob = knobs[1];
    if (endKnob === undefined) throw new Error('test did not find the end handle');

    act(() => {
      endKnob.props.onResponderGrant(eventAt(70, 36));
      endKnob.props.onResponderMove(eventAt(50, 110), { dx: -20, dy: 74 });
    });

    expect(store.getSnapshot().range).toEqual({
      anchor: { nodeId: 'outside', unitId: 'outside#0', offset: 0 },
      focus: { nodeId: 'outside', unitId: 'outside#0', offset: 5 },
    });
  });

  test('a selection viewport locks only the enclosing scroller during its own touch', async () => {
    const activity: boolean[] = [];

    act(() => {
      renderer = create(
        <SelectionRoot
          units={UNITS}
          overlay={false}
          handles={false}
          toolbar={false}
          onGestureActiveChange={active => activity.push(active)}
        >
          <SelectionViewport style={{ height: 80 }}>
            {React.createElement('ScrollView')}
          </SelectionViewport>
        </SelectionRoot>
      );
    });

    const viewport = renderer.root
      .findAllByType('View' as unknown as React.ElementType)
      .find(
        node => typeof node.props.onTouchStart === 'function' && node.props.style !== undefined
      );
    const scrollView = renderer.root.findByType('ScrollView' as unknown as React.ElementType);
    if (viewport === undefined) throw new Error('test did not find the selection viewport');

    act(() => {
      viewport.props.onTouchStart(eventAt(20, 120));
    });
    expect(activity).toEqual([true]);
    expect(scrollView.props.scrollEnabled).toBe(true);

    await act(async () => {
      viewport.props.onTouchEnd(eventAt(20, 120));
      await new Promise(resolve => setTimeout(resolve, 1));
    });
    expect(activity).toEqual([true, false]);

    act(() => {
      scrollView.props.onMomentumScrollBegin(eventAt(20, 120));
      scrollView.props.onMomentumScrollEnd(eventAt(20, 120));
    });
    expect(activity).toEqual([true, false]);
  });

  test('momentum releases a swallowed viewport touch before the fling continues', () => {
    const activity: boolean[] = [];

    act(() => {
      renderer = create(
        <SelectionRoot
          units={UNITS}
          overlay={false}
          handles={false}
          toolbar={false}
          onGestureActiveChange={active => activity.push(active)}
        >
          <SelectionViewport style={{ height: 80 }}>
            {React.createElement('ScrollView')}
          </SelectionViewport>
        </SelectionRoot>
      );
    });

    const viewport = renderer.root
      .findAllByType('View' as unknown as React.ElementType)
      .find(node => typeof node.props.onTouchStart === 'function');
    const scrollView = renderer.root.findByType('ScrollView' as unknown as React.ElementType);
    if (viewport === undefined) throw new Error('test did not find the selection viewport');

    act(() => {
      viewport.props.onTouchStart(eventAt(20, 120));
      scrollView.props.onScrollBeginDrag(eventAt(20, 120));
      // Android sometimes omits the bubbling touch end/cancel. End-drag alone
      // cannot release while that stale touch flag remains set.
      scrollView.props.onScrollEndDrag(eventAt(20, 120));
    });
    expect(activity).toEqual([true]);

    act(() => {
      scrollView.props.onMomentumScrollBegin(eventAt(20, 120));
    });
    expect(activity).toEqual([true, false]);
  });
});

describe('content revision invalidation (D)', () => {
  const UNITS_B: SelectionUnit[] = [
    { kind: 'text', unitId: 'p2#0', nodeId: 'p2', text: 'world', node: NODE },
  ];

  async function mountWithProbe(
    units: readonly SelectionUnit[],
    onCopy?: (request: { id: string }) => void
  ): Promise<{
    renderer: ReactTestRenderer;
    ctx: () => ReturnType<typeof useSelectionContext>;
  }> {
    let ctx: ReturnType<typeof useSelectionContext> | null = null;
    function Probe() {
      ctx = useSelectionContext();
      return null;
    }
    let renderer: ReactTestRenderer | null = null;
    await act(async () => {
      renderer = create(
        React.createElement(SelectionRoot, { units, onCopy }, React.createElement(Probe))
      );
    });
    return {
      renderer: renderer as unknown as ReactTestRenderer,
      ctx: () => ctx as ReturnType<typeof useSelectionContext>,
    };
  }

  async function establishSelection(store: SelectionStore): Promise<void> {
    await act(async () => {
      store.beginAt({ nodeId: 'p1', offset: 0 });
      store.extendTo({ nodeId: 'p1', offset: 3 });
      store.commit();
    });
  }

  test('a new unit stream clears the current selection outright', async () => {
    const { renderer, ctx } = await mountWithProbe(UNITS);
    await establishSelection(ctx().store);
    expect(ctx().store.getSnapshot().range).not.toBeNull();

    await act(async () => {
      renderer.update(React.createElement(SelectionRoot, { units: UNITS_B }));
    });
    expect(ctx().store.getSnapshot().range).toBeNull();
    expect(ctx().store.getSnapshot().phase).toBe('idle');
  });

  test('a re-render with the same unit stream keeps the selection', async () => {
    const { renderer, ctx } = await mountWithProbe(UNITS);
    await establishSelection(ctx().store);

    await act(async () => {
      renderer.update(React.createElement(SelectionRoot, { units: UNITS }));
    });
    expect(ctx().store.getSnapshot().range).not.toBeNull();
  });

  test('a late toolbar action after invalidation copies nothing', async () => {
    const copies: string[] = [];
    const { renderer, ctx } = await mountWithProbe(UNITS, request => copies.push(request.id));
    await establishSelection(ctx().store);

    await act(async () => {
      renderer.update(
        React.createElement(SelectionRoot, {
          units: UNITS_B,
          onCopy: request => copies.push(request.id),
        })
      );
    });
    // The range is gone, so a stale action has nothing to serialize.
    await act(async () => {
      ctx().runToolbarItem({ id: 'copy', title: 'Copy', format: 'plainText' });
    });
    expect(copies).toHaveLength(0);
  });
});

describe('host-driven entries (handleRef + longPress)', () => {
  /** A registered block at (0,0)-(100,20) with one unit, plus handle/store probes. */
  async function mountDriver(
    props: Record<string, unknown> = {},
    // Give the native root a real page measurement instead of relying on a cached-origin fallback.
    nativeHandle: unknown = {
      measure: (cb: (...args: number[]) => void) => cb(0, 0, 100, 20, 0, 0),
    }
  ): Promise<{
    renderer: ReactTestRenderer;
    handle: () => SelectionRootHandle;
    store: () => SelectionStore;
  }> {
    let handle: unknown = null;
    let store: SelectionStore | null = null;
    const ctxRef: { current: ReturnType<typeof useSelectionContext> | null } = { current: null };
    function Probe() {
      const ctx = useSelectionContext();
      store = ctx.store;
      ctxRef.current = ctx;
      // Register the block exactly once, as SelectableBlock would.
      _useEffect(() => {
        const dispose = ctx.registerBlock({
          nodeId: 'p1',
          unitIds: ['p1#0'],
          kind: 'text',
          rect: { x: 0, y: 0, w: 100, h: 20 },
        });
        return dispose;
      }, [ctx]);
      return null;
    }
    let renderer: ReactTestRenderer | null = null;
    await act(async () => {
      renderer = create(
        React.createElement(
          SelectionRoot,
          {
            units: UNITS,
            gestures: false,
            handleRef: (h: unknown) => {
              handle = h;
            },
            ...props,
          },
          React.createElement(Probe)
        ),
        { createNodeMock: () => nativeHandle }
      );
    });
    return {
      renderer: renderer as unknown as ReactTestRenderer,
      handle: () => handle as SelectionRootHandle,
      store: () => store as SelectionStore,
    };
  }

  // A host entry from outside the tree notifies the store, so every call runs
  // inside act — otherwise React reports the subscription updates as unwrapped.
  async function selectAt(
    handle: SelectionRootHandle,
    x: number,
    y: number
  ): Promise<boolean> {
    let ok = false;
    await act(async () => {
      ok = await handle.trySelectParagraphAt(x, y);
    });
    return ok;
  }
  async function pageRect(handle: SelectionRootHandle): Promise<unknown> {
    let rect: unknown;
    await act(async () => {
      rect = await handle.measurePageRect();
    });
    return rect;
  }

  test('trySelectParagraphAt selects the whole block under the point', async () => {
    const { handle, store } = await mountDriver();
    // The native page origin is zero: the raw point lands inside the block.
    const ok = await selectAt(handle(), 50, 10);
    expect(ok).toBe(true);
    const snap = store().getSnapshot();
    expect(snap.phase).toBe('selected');
    expect(snap.range).not.toBeNull();
    expect(snap.units.length).toBeGreaterThan(0);
  });

  test('host selection and live root arbitration share page coordinates across window offsets and scrolling', async () => {
    // Deliberately separate parent-local, RN page and window origins as in an embedded Android surface.
    let pageY = 200;
    const { handle, store } = await mountDriver(
      {},
      {
        measure: (cb: (...args: number[]) => void) => cb(7, 9, 100, 20, 40, pageY),
        measureInWindow: (cb: (...args: number[]) => void) => cb(70, pageY + 48, 100, 20),
      }
    );
    expect(await pageRect(handle())).toEqual({ x: 40, y: 200, w: 100, h: 20 });
    expect(await selectAt(handle(), 90, 210)).toBe(true);
    expect(store().getSnapshot().range?.anchor.unitId).toBe('p1#0');
    // A host scroll moves the root without changing its own onLayout frame.
    pageY = 120;
    act(() => {
      handle().clearSelection();
    });
    expect(await pageRect(handle())).toEqual({ x: 40, y: 120, w: 100, h: 20 });
    expect(await selectAt(handle(), 90, 130)).toBe(true);
    expect(await selectAt(handle(), 90, 210)).toBe(false);
  });

  test('a window inset cannot redirect a page press to the paragraph below', async () => {
    // Two text rows separated by the same 48 points as the native window/page discrepancy.
    const units: SelectionUnit[] = [
      ...UNITS,
      { kind: 'text', unitId: 'p2#0', nodeId: 'p2', text: 'below', node: NODE },
    ];
    const handleRef = React.createRef<SelectionRootHandle>();
    let store: SelectionStore | undefined;
    // Register both paragraphs so the old window subtraction selects real, but wrong, text.
    function Paragraphs() {
      const ctx = useSelectionContext();
      useEffect(() => {
        store = ctx.store;
        const disposers = units.map((unit, index) =>
          ctx.registerBlock({
            nodeId: unit.nodeId,
            unitIds: [unit.unitId],
            kind: 'text',
            rect: { x: 0, y: index * 48, w: 100, h: 20 },
          })
        );
        return () => disposers.forEach(dispose => dispose());
      }, [ctx]);
      return null;
    }
    await act(async () => {
      renderer = create(
        <SelectionRoot
          units={units}
          handleRef={handleRef}
          overlay={false}
          handles={false}
          toolbar={false}
        >
          <Paragraphs />
        </SelectionRoot>,
        {
          createNodeMock: () => ({
            measure: (cb: (...args: number[]) => void) => cb(0, 0, 100, 68, 30, 80),
            measureInWindow: (cb: (...args: number[]) => void) => cb(30, 32, 100, 68),
          }),
        }
      );
    });
    await act(async () => {
      expect(await handleRef.current!.trySelectParagraphAt(50, 90)).toBe(true);
    });
    expect(store!.getSnapshot().range?.anchor.unitId).toBe('p1#0');
    expect(store!.getSnapshot().range?.focus.unitId).toBe('p1#0');
  });

  test('a press just outside the block edges snaps to the block it belongs to', async () => {
    const { handle, store } = await mountDriver();
    // 8 units past both edges: inside the tight (12) slack, resolves to the block.
    const ok = await selectAt(handle(), 108, 28);
    // Within reach of the blocks' own extent (not the root's box): chooseBlock
    // resolves it to the block it visually belongs to.
    expect(ok).toBe(true);
    expect(store().getSnapshot().phase).toBe('selected');
  });

  test('a press far beyond the blocks is rejected', async () => {
    const { handle, store } = await mountDriver();
    const ok = await selectAt(handle(), 500, 500);
    // Outside the blocks' extent by more than the slack: not ours, no selection.
    expect(ok).toBe(false);
    expect(store().getSnapshot().phase).toBe('idle');
  });

  test('measurePageRect reports null when the root cannot be measured', async () => {
    const { handle } = await mountDriver({}, null);
    // An unmounted native root must not select using a stale cached origin.
    expect(await pageRect(handle())).toBeNull();
    expect(await selectAt(handle(), 50, 10)).toBe(false);
  });

  test('a block whose units are not in the stream refuses the entry', async () => {
    const { handle, store } = await mountDriver({ units: [] });
    const ok = await selectAt(handle(), 50, 10);
    expect(ok).toBe(false);
    expect(store().getSnapshot().phase).toBe('idle');
  });

  test('a disabled root refuses the external entry', async () => {
    const { handle } = await mountDriver({ enabled: false });
    const ok = await selectAt(handle(), 50, 10);
    expect(ok).toBe(false);
  });

  test('clearSelection ends a host-started selection', async () => {
    const { handle, store } = await mountDriver();
    await selectAt(handle(), 50, 10);
    expect(store().getSnapshot().range).not.toBeNull();
    act(() => {
      handle().clearSelection();
    });
    expect(store().getSnapshot().range).toBeNull();
    expect(store().getSnapshot().phase).toBe('idle');
  });

  test('longPress: false never selects on hold, but a tap still clears', async () => {
    const { renderer, handle, store } = await mountDriver({
      longPress: false,
      longPressMs: 1,
      gestures: true,
    });
    const root = renderer.root
      .findAllByType('View' as unknown as React.ElementType)
      .find(node => typeof node.props.onTouchStart === 'function');
    if (root === undefined) throw new Error('test did not find the selection root');

    // Hold well past the threshold: with longPress off, nothing selects.
    act(() => {
      root.props.onTouchStart(eventAt(40, 10));
    });
    await act(async () => {
      await new Promise(resolve => setTimeout(resolve, 20));
    });
    act(() => {
      root.props.onTouchEnd(eventAt(40, 10));
    });
    expect(store().getSnapshot().phase).toBe('idle');

    // A programmatic selection, then a tap inside the root clears it.
    await selectAt(handle(), 50, 10);
    expect(store().getSnapshot().range).not.toBeNull();
    act(() => {
      root.props.onTouchStart(eventAt(40, 10));
      root.props.onTouchEnd(eventAt(40, 10));
    });
    expect(store().getSnapshot().range).toBeNull();
  });

  test('onSelectionChange reports the committed range and a final null on clear', async () => {
    const seen: Array<unknown> = [];
    const { handle } = await mountDriver({
      onSelectionChange: (range: unknown) => seen.push(range),
    });
    await selectAt(handle(), 50, 10);
    expect(seen.length).toBeGreaterThan(0);
    // The host relies on this notification to dismiss its own chrome.
    expect(seen[seen.length - 1]).not.toBeNull();
    act(() => {
      handle().clearSelection();
    });
    expect(seen[seen.length - 1]).toBeNull();
  });
});

// Native callbacks can outlive the document and the host action that requested them.
describe('review regressions: native entry lifetime', () => {
  // Mount a real nested SelectableBlock and capture deferred native measurements.
  async function mountDeferredRoot() {
    // Capture the imperative API exposed by the mounted root.
    const handleRef = React.createRef<SelectionRootHandle>();
    // Hold callbacks so tests can deliver native measurements after lifecycle changes.
    const callbacks: Array<(...args: number[]) => void> = [];
    let ctx!: ReturnType<typeof useSelectionContext>;
    // Keep the probe component stable so prop updates preserve the block registration.
    function Probe() {
      ctx = useSelectionContext();
      return null;
    }
    // Use the real block's Android Text handler rather than calling the context directly.
    const { SelectableBlock } = await import('../../coordinator/SelectableBlock');
    // Supply a nested viewport without requiring a native ScrollView in this unit test.
    const { SelectionViewportContext } = await import('../../coordinator/SelectionContext');
    // The viewport context enables Android's actual Text.onLongPress entry point.
    const children = (
      <>
        <Probe />
        <SelectionViewportContext.Provider value="nested">
          <SelectableBlock nodeId="p1" unitIds={['p1#0']}>
            hello
          </SelectableBlock>
        </SelectionViewportContext.Provider>
      </>
    );
    // Preserve the tree while changing only the root's document/interaction props.
    const tree = (props: Partial<React.ComponentProps<typeof SelectionRoot>> = {}) => (
      <SelectionRoot
        units={UNITS}
        handleRef={handleRef}
        overlay={false}
        handles={false}
        toolbar={false}
        {...props}
      >
        {children}
      </SelectionRoot>
    );
    await act(async () => {
      renderer = create(tree(), {
        createNodeMock: () => ({
          measure: (callback: (...args: number[]) => void) => callbacks.push(callback),
        }),
      });
    });
    ctx.registry.updateLayout('p1', { x: 0, y: 0, w: 100, h: 20 });
    return { handle: handleRef.current!, callbacks, ctx, tree };
  }

  test('Android nested Text long press observes the current longPress and gestures gates', async () => {
    // Deliver the native event itself, including after the option changes on the same root.
    const fixture = await mountDeferredRoot();
    for (const props of [{ longPress: false }, { gestures: false }, { longPress: true }]) {
      await act(async () => renderer!.update(fixture.tree(props)));
      // Native layout follows any context-driven block registration change.
      fixture.ctx.registry.updateLayout('p1', { x: 0, y: 0, w: 100, h: 20 });
      act(() =>
        renderer!.root.findByType('Text' as React.ElementType).props.onLongPress({
          nativeEvent: { locationX: 20, locationY: 10 },
        })
      );
      expect(fixture.ctx.store.getSnapshot().range !== null).toBe(props.longPress === true);
    }
  });

  test.each(['document', 'clear', 'disable', 'unmount'] as const)(
    'rejects an outstanding native measurement after %s',
    async reason => {
      // Reuse stable ids after replacement to expose accidental selection of the new text.
      const fixture = await mountDeferredRoot();
      // Start the host request before invalidating its document or interaction lifetime.
      const pending = fixture.handle.trySelectParagraphAt(50, 10);
      expect(fixture.callbacks).toHaveLength(1);
      await act(async () => {
        // Each transition ends the lifetime of the original host request.
        if (reason === 'document')
          renderer!.update(
            fixture.tree({
              units: [{ ...UNITS[0], kind: 'text', text: 'replacement' } as SelectionUnit],
            })
          );
        else if (reason === 'clear') fixture.handle.clearSelection();
        else if (reason === 'disable') renderer!.update(fixture.tree({ enabled: false }));
        else renderer!.unmount();
      });
      // Re-enabling must not resurrect a request made before the disable.
      if (reason === 'disable') await act(async () => renderer!.update(fixture.tree()));
      await act(async () => fixture.callbacks[0](0, 0, 100, 20, 0, 0));
      expect(await pending).toBe(false);
      expect(fixture.ctx.store.getSnapshot().range).toBeNull();
    }
  );

  test('a newer host request supersedes an older native measurement', async () => {
    // The older in-bounds result must not override a newer out-of-bounds request.
    const fixture = await mountDeferredRoot();
    // Queue an in-bounds request that would select if its stale callback were accepted.
    const older = fixture.handle.trySelectParagraphAt(50, 10);
    // The most recent request takes ownership even when its hit test returns false.
    const newer = fixture.handle.trySelectParagraphAt(500, 500);
    await act(async () => fixture.callbacks[1](0, 0, 100, 20, 0, 0));
    expect(await newer).toBe(false);
    await act(async () => fixture.callbacks[0](0, 0, 100, 20, 0, 0));
    expect(await older).toBe(false);
    expect(fixture.ctx.store.getSnapshot().range).toBeNull();
  });
});
