import { afterEach, beforeEach, describe, expect, mock, test } from 'bun:test';
import React from 'react';
import { create, act, type ReactTestRenderer } from 'react-test-renderer';

// react-native's JS entry contains Flow syntax bun cannot load, so its
// components are mocked as host strings: react-test-renderer can then render
// the tree and we can read props off it and invoke them. bun's `mock.module`
// registry is process-wide, so the mock lives here, in the only file that
// renders React.
mock.module('react-native', () => ({
  View: 'View',
  Text: 'Text',
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

const { SelectableBlock } = await import('../../coordinator/SelectableBlock');
const { SelectionContext, SelectionViewportContext } =
  await import('../../coordinator/SelectionContext');
const { SelectionRegistry } = await import('../../coordinator/registry');

type ContextValue = React.ContextType<typeof SelectionContext>;
type SelectionUnitLike = Parameters<InstanceType<typeof SelectionRegistry>['setUnits']>[0][number];

// A text unit whose `node` is never inspected by the registry or the block.
const textUnit = (unitId: string, text: string): SelectionUnitLike =>
  ({
    kind: 'text',
    unitId,
    nodeId: 'p1',
    text,
    node: { type: 'text', value: text },
  }) as SelectionUnitLike;

function makeContext(units: SelectionUnitLike[]): NonNullable<ContextValue> {
  const registry = new SelectionRegistry(units);
  return {
    registry,
    store: {} as NonNullable<ContextValue>['store'],
    registerBlock: block => {
      const registered = registry.register(block);
      return () => registry.unregister(block.nodeId, registered);
    },
    updateLayout: (nodeId, rect) => registry.updateLayout(nodeId, rect),
    refreshLayouts: () => {},
    registerViewport: (viewportId, offset) => registry.registerViewport(viewportId, offset),
    measureViewportLayout: (viewportId, _node, rect) =>
      registry.updateViewportLayout(viewportId, rect),
    updateViewportScroll: (viewportId, offset) => registry.updateViewportScroll(viewportId, offset),
    setViewportInteractionActive: () => {},
    cancelPendingGesture: () => {},
    selectWordInBlock: () => {},
    updateUnits: (nodeId, unitIds) => registry.updateUnits(nodeId, unitIds),
    setMetrics: (nodeId, metrics) => registry.setMetrics(nodeId, metrics),
    setContentOffset: (nodeId, offset) => registry.setContentOffset(nodeId, offset),
    toolbarItems: [],
    runToolbarItem: () => {},
  };
}

let renderer: ReactTestRenderer | null = null;

beforeEach(() => {
  renderer = null;
});

function renderBlock(
  children: React.ReactNode = 'hello',
  units: SelectionUnitLike[] = [textUnit('p1#0', 'hello')]
): ReactTestRenderer {
  let r!: ReactTestRenderer;
  act(() => {
    r = create(
      <SelectionContext.Provider value={makeContext(units)}>
        <SelectableBlock nodeId="p1" unitIds={['p1#0']}>
          {children}
        </SelectableBlock>
      </SelectionContext.Provider>
    );
  });
  renderer = r;
  return r;
}

describe('SelectableBlock rendering', () => {
  test('renders a plain Text, not a native selection component', () => {
    // The whole point of the self-drawn direction: no vendored native view, no
    // `selectable` prop, nothing on the platform side holding selection state.
    // A plain `<Text>` is what removes the Fabric-only / Android >= 0.85 floors.
    const r = renderBlock();
    const text = r.root.findByType('Text' as unknown as React.ElementType);
    expect('selectable' in text.props).toBe(false);
    // The two measurements the coordinator needs are both wired.
    expect(typeof text.props.onTextLayout).toBe('function');
    expect(typeof text.props.onLayout).toBe('function');
    expect(renderer).not.toBeNull();
  });

  test('onTextLayout publishes a line table into the registry', () => {
    const ctx = makeContext([textUnit('p1#0', 'hello world')]);
    const { registry } = ctx;
    let r!: ReactTestRenderer;
    act(() => {
      r = create(
        <SelectionContext.Provider value={ctx}>
          <SelectableBlock nodeId="p1" unitIds={['p1#0']}>
            hello world
          </SelectableBlock>
        </SelectionContext.Provider>
      );
    });
    renderer = r;
    const text = r.root.findByType('Text' as unknown as React.ElementType);

    act(() => {
      text.props.onLayout({ nativeEvent: { layout: { x: 4, y: 2, width: 100, height: 20 } } });
      text.props.onTextLayout({
        nativeEvent: {
          lines: [
            { text: 'hello ', x: 0, y: 0, width: 60, height: 20 },
            { text: 'world', x: 0, y: 20, width: 50, height: 20 },
          ],
        },
      });
    });

    const block = registry.getBlock('p1');
    expect(block?.contentOffset).toEqual({ x: 4, y: 2 });
    expect(block?.metrics?.textLength).toBe(11);
    expect(block?.metrics?.lines.map(l => [l.start, l.end])).toEqual([
      [0, 6],
      [6, 11],
    ]);
  });

  test('onLayout publishes a fallback rect while native root measurement is pending', () => {
    const ctx = {
      ...makeContext([textUnit('p1#0', 'hello')]),
      measureLayout: () => {},
    };
    let r!: ReactTestRenderer;
    act(() => {
      r = create(
        <SelectionContext.Provider value={ctx}>
          <SelectableBlock nodeId="p1" unitIds={['p1#0']}>
            hello
          </SelectableBlock>
        </SelectionContext.Provider>
      );
    });
    renderer = r;
    const view = r.root.findByType('View' as unknown as React.ElementType);

    act(() => {
      view.props.onLayout({ nativeEvent: { layout: { x: 8, y: 13, width: 55, height: 21 } } });
    });

    expect(ctx.registry.getBlock('p1')?.rect).toEqual({ x: 8, y: 13, w: 55, h: 21 });
  });

  test('nested text forwards its native long press in text-local coordinates', () => {
    const calls: unknown[] = [];
    const ctx = {
      ...makeContext([textUnit('p1#0', 'hello')]),
      selectWordInBlock: (...args: unknown[]) => calls.push(args),
    };
    let r!: ReactTestRenderer;
    act(() => {
      r = create(
        <SelectionContext.Provider value={ctx}>
          <SelectionViewportContext.Provider value="list">
            <SelectableBlock nodeId="p1" unitIds={['p1#0']}>
              hello
            </SelectableBlock>
          </SelectionViewportContext.Provider>
        </SelectionContext.Provider>
      );
    });
    renderer = r;
    const text = r.root.findByType('Text' as unknown as React.ElementType);

    act(() => {
      text.props.onLongPress({ nativeEvent: { locationX: 17, locationY: 9 } });
    });

    expect(calls).toEqual([['p1', { x: 17, y: 9 }]]);
  });

  test('nested onLayout never publishes a cell-local rect over root-space geometry', () => {
    const measured: unknown[][] = [];
    const ctx = {
      ...makeContext([textUnit('p1#0', 'hello')]),
      measureLayout: (...args: unknown[]) => measured.push(args),
    };
    let r!: ReactTestRenderer;
    act(() => {
      r = create(
        <SelectionContext.Provider value={ctx}>
          <SelectionViewportContext.Provider value="list">
            <SelectableBlock nodeId="p1" unitIds={['p1#0']}>
              hello
            </SelectableBlock>
          </SelectionViewportContext.Provider>
        </SelectionContext.Provider>
      );
    });
    renderer = r;
    const view = r.root.findByType('View' as unknown as React.ElementType);

    act(() => {
      view.props.onLayout({ nativeEvent: { layout: { x: 8, y: 96, width: 55, height: 21 } } });
    });

    expect(ctx.registry.getBlock('p1')?.rect).toBeUndefined();
    expect(measured).toHaveLength(1);
    expect(measured[0]?.[2]).toEqual({ x: 8, y: 96, w: 55, h: 21 });
  });

  test('remeasures a captured nested layout after block registration changes', () => {
    const measured: unknown[][] = [];
    const ctx = {
      ...makeContext([textUnit('p1#0', 'hello')]),
      measureLayout: (...args: unknown[]) => measured.push(args),
    };
    let r!: ReactTestRenderer;
    act(() => {
      r = create(
        <SelectionContext.Provider value={ctx}>
          <SelectionViewportContext.Provider value={undefined}>
            <SelectableBlock nodeId="p1" unitIds={['p1#0']}>
              hello
            </SelectableBlock>
          </SelectionViewportContext.Provider>
        </SelectionContext.Provider>
      );
    });
    renderer = r;
    const view = r.root.findByType('View' as unknown as React.ElementType);

    act(() => {
      view.props.onLayout({ nativeEvent: { layout: { x: 8, y: 96, width: 55, height: 21 } } });
    });
    expect(measured).toHaveLength(1);

    act(() => {
      r.update(
        <SelectionContext.Provider value={ctx}>
          <SelectionViewportContext.Provider value="list">
            <SelectableBlock nodeId="p1" unitIds={['p1#0']}>
              hello
            </SelectableBlock>
          </SelectionViewportContext.Provider>
        </SelectionContext.Provider>
      );
    });

    expect(measured).toHaveLength(2);
    expect(measured[1]?.[2]).toEqual({ x: 8, y: 96, w: 55, h: 21 });
  });
});

describe('SelectableBlock dev-mode text invariant', () => {
  const realDev = (globalThis as { __DEV__?: boolean }).__DEV__;
  const realWarn = console.warn;

  afterEach(() => {
    (globalThis as { __DEV__?: boolean }).__DEV__ = realDev;
    console.warn = realWarn;
  });

  function captureWarnings(fn: () => void): string[] {
    const seen: string[] = [];
    console.warn = (...args: unknown[]) => {
      seen.push(args.map(String).join(' '));
    };
    fn();
    return seen;
  }

  test('warns when the rendered text does not match the units summed length', () => {
    (globalThis as { __DEV__?: boolean }).__DEV__ = true;
    // The host renders a bullet the units do not contain: every native offset
    // past it is shifted by two characters.
    const warnings = captureWarnings(() =>
      renderBlock('\u2022 hello', [textUnit('p1#0', 'hello')])
    );
    expect(warnings.length).toBe(1);
    expect(warnings[0]).toContain('SelectableBlock "p1"');
    expect(warnings[0]).toContain('sum to 5');
    expect(warnings[0]).toContain('renders 7');
  });

  test('stays silent when they match', () => {
    (globalThis as { __DEV__?: boolean }).__DEV__ = true;
    const warnings = captureWarnings(() => renderBlock('hello', [textUnit('p1#0', 'hello')]));
    expect(warnings).toEqual([]);
  });

  test('stays silent in production', () => {
    (globalThis as { __DEV__?: boolean }).__DEV__ = false;
    const warnings = captureWarnings(() =>
      renderBlock('\u2022 hello', [textUnit('p1#0', 'hello')])
    );
    expect(warnings).toEqual([]);
  });

  test('stays silent when a unit is not in the stream yet', () => {
    // The block re-rendered before the root's `setUnits` effect: there is
    // nothing to compare against, and warning here would be pure noise.
    (globalThis as { __DEV__?: boolean }).__DEV__ = true;
    const warnings = captureWarnings(() => renderBlock('hello', []));
    expect(warnings).toEqual([]);
  });
});
