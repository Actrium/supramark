import { afterEach, expect, mock, test } from 'bun:test';
import React, { useEffect } from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { SelectionUnit } from '../../model';
import type { SelectionStore } from '../../coordinator/state';
import type { SelectionRootHandle } from '../../coordinator/SelectionRoot';
import { buildTextMetrics } from '../../metrics';

const platform = { OS: 'android' };
let scroll = 0;
const measureTarget = (
  tag: number,
  callback: (x: number, y: number, w: number, h: number) => void
) => {
  if (tag === 42) callback(92, 135 - scroll, 100, 80);
};
mock.module('react-native', () => ({
  Platform: platform,
  UIManager: { measureInWindow: measureTarget },
  View: 'View',
  Text: 'Text',
  TouchableOpacity: 'TouchableOpacity',
  PanResponder: { create: () => ({ panHandlers: {} }) },
  StyleSheet: { absoluteFill: {}, create: (styles: unknown) => styles },
}));
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
const { SelectionViewport } = await import('../../coordinator/SelectionViewport');
const { SelectionViewportContext } = await import('../../coordinator/SelectionContext');
const { SelectionRoot } = await import('../../coordinator/SelectionRoot');
const { useSelectionContext } = await import('../../coordinator/useDocumentSelection');

const units: SelectionUnit[] = ['iiiiW', 'helloWORLD'].map(text => ({
  kind: 'text',
  unitId: text,
  nodeId: text,
  text,
  node: { type: 'text', value: text },
}));
let tree: ReactTestRenderer;
afterEach(() => {
  act(() => tree?.unmount());
  platform.OS = 'android';
  scroll = 0;
});

const firstLine = { x: 0, y: 0, width: 40, height: 20 };
const metrics = [
  buildTextMetrics([{ ...firstLine, text: 'iiiiW', charXs: [0, 3, 6, 9, 12, 40] }]),
  buildTextMetrics([{ ...firstLine, text: 'hello' }, { ...firstLine, y: 20, text: 'WORLD' }]),
];

function mountDesktop(os: string, documentUnits = units, lineMetrics = metrics, nested = false) {
  platform.OS = os;
  let store!: SelectionStore;
  const handle = React.createRef<SelectionRootHandle>();
  function Blocks() {
    const ctx = useSelectionContext();
    const viewportId = React.useContext(SelectionViewportContext);
    useEffect(() => {
      store = ctx.store;
      const disposers = documentUnits.map(({ nodeId, unitId }, i) =>
        ctx.registerBlock({
          kind: 'text',
          viewportId,
          nodeId,
          unitIds: [unitId],
          rect: { x: 12, y: i * 30, w: 100, h: lineMetrics[i].lines.length * 20 },
          metrics: lineMetrics[i],
        })
      );
      return () => disposers.forEach(dispose => dispose());
    }, [ctx, viewportId]);
    return null;
  }
  act(() => {
    tree = create(
      <SelectionRoot units={documentUnits} toolbar={false} handleRef={handle}>
        {nested ? (
          <SelectionViewport>
            {React.createElement('ScrollView', null, <Blocks />)}
          </SelectionViewport>
        ) : (
          <Blocks />
        )}
      </SelectionRoot>,
      {
        createNodeMock: () => ({
          getBoundingClientRect: () => ({ x: 80, y: 130 - scroll }),
          measureInWindow: (callback: (x: number, y: number, w: number, h: number) => void) =>
            callback(80, 130 - scroll, 150, 80),
        }),
      }
    );
  });
  return { handle, store };
}

function input(phase: 'Start' | 'Move' | 'End', x: number, y: number, button = 0, buttons = 1) {
  const handler = platform.OS === 'macos'
    ? 'onTouch' + phase
    : { Start: 'onPointerDown', Move: 'onPointerMove', End: 'onPointerUp' }[phase];
  const root = tree.root.findAllByType('View')[0];
  act(() =>
    root.props[handler]({
      nativeEvent: {
        pageX: 92 + x,
        pageY: 130 + y - scroll + (platform.OS === 'macos' ? 28 : 0),
        button,
        buttons,
        target: 77,
        locationX: x,
        locationY: y - 5,
        changedTouches: [{ identifier: 2, target: 42, locationX: x, locationY: y - 5 }],
        identifier: 2,
        pointerId: 2,
        pointerType: 'mouse',
      },
    })
  );
}

for (const os of ['macos', 'windows']) {
  test(`${os} maps a primary drag to text in an offset root and ignores hover and secondary presses`, () => {
    const { handle, store } = mountDesktop(os);
    input('Start', 6, 6, 2);
    input('Move', 16, 36, 2);
    input('End', 16, 36, 2);
    expect(store.getSnapshot().phase).toBe('idle');
    input('Start', 6, 6);
    if (os === 'windows') input('Move', 40, 36, -1, 0);
    expect(store.getSnapshot().phase).toBe('idle');
    input('Move', 16, 36);
    expect(store.getSnapshot().phase).toBe('selecting');
    expect(store.getSnapshot().units.map(unit => unit.text)).toEqual(['iiW', 'he']);
    input('End', 24, 36, 0, 0);
    expect(store.getSnapshot().phase).toBe('selected');
    expect(store.getSnapshot().units.map(unit => unit.text)).toEqual(['iiW', 'hel']);
    expect(handle.current!.getSelectedText()).toBe('iiWhel');
    expect(handle.current!.getSelectedText()).toBe('iiWhel');
    expect(store.getSnapshot().phase).toBe('selected');
    input('Start', 70, 6);
    expect(handle.current!.getSelectedText()).toBeNull();
    input('Move', 16, 36);
    input('End', 16, 36);
    expect(handle.current!.getSelectedText()).toBe('he');
  });

  test(`${os} starts a drag at either end of the pressed visual line`, () => {
    const { handle, store } = mountDesktop(os);
    input('Start', -6, 56);
    input('End', 16, 56);
    expect(handle.current!.getSelectedText()).toBe('WO');
    input('Start', 105, 56);
    input('End', 16, 56);
    expect(handle.current!.getSelectedText()).toBe('RLD');
    input('Start', 70, 36);
    input('End', 16, 56);
    expect(handle.current!.getSelectedText()).toBe('WO');
    input('Start', 105, 56);
    input('End', 92, 56);
    expect(store.getSnapshot().range).toBeNull();
    input('Start', -6, 25);
    input('End', 16, 36);
    expect(store.getSnapshot().range).toBeNull();
  });
}

test('macOS uses the physical target on every phase in a titled, scrolling window', () => {
  const { handle } = mountDesktop('macos');
  input('Start', 6, 6);
  scroll = 24;
  input('Move', 16, 36);
  expect(handle.current!.getSelectedText()).toBe('iiWhe');
  scroll = 42;
  input('End', 16, 56);
  expect(handle.current!.getSelectedText()).toBe('iiWhelloWO');
});

test('macOS copies RTL and mixed-direction ranges in logical order from either visual gutter', () => {
  const document = ['אבג', 'aאבz'].map(text => ({
    kind: 'text' as const,
    unitId: text,
    nodeId: text,
    text,
    node: { type: 'text' as const, value: text },
  }));
  const lines = [
    buildTextMetrics([{ ...firstLine, width: 30, text: 'אבג', charXs: [30, 20, 10, 0] }]),
    buildTextMetrics([
      { ...firstLine, text: 'aאבz', charXs: [0, 30, 20, 30, 40], charEndXs: [10, 20, 10, 40] },
    ]),
  ];
  const { handle } = mountDesktop('macos', document, lines);
  input('Start', -6, 6);
  input('End', 20, 6);
  expect(handle.current!.getSelectedText()).toBe('בג');
  input('Start', 50, 6);
  input('End', 10, 6);
  expect(handle.current!.getSelectedText()).toBe('אב');
  input('Start', -6, 36);
  input('End', 20, 36);
  expect(handle.current!.getSelectedText()).toBe('aא');
});

test('macOS nested viewport follows the root drag and re-enables wheel scrolling on release', () => {
  const { handle } = mountDesktop('macos', units, metrics, true);
  const viewport = tree.root.findByType(SelectionViewport).findAllByType('View')[0];
  act(() =>
    viewport.props.onLayout({ nativeEvent: { layout: { x: 0, y: 0, width: 150, height: 80 } } })
  );
  const scroller = tree.root.findByType('ScrollView');
  expect(scroller.props.scrollEnabled).toBe(true);
  input('Start', 0, 6);
  input('Move', 9, 6);
  expect(scroller.props.scrollEnabled).toBe(false);
  input('End', 9, 6);
  expect(handle.current!.getSelectedText()).toBe('iii');
  expect(scroller.props.scrollEnabled).toBe(true);
  act(() => scroller.props.onScroll({ nativeEvent: { contentOffset: { x: 0, y: 30 } } }));
  input('Start', 0, 6);
  input('End', 16, 6);
  expect(handle.current!.getSelectedText()).toBe('he');
});
