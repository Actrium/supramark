import { afterEach, expect, mock, test } from 'bun:test';
import React, { useEffect } from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { SelectionUnit } from '../../model';
import type { SelectionStore } from '../../coordinator/state';
import type { SelectionRootHandle } from '../../coordinator/SelectionRoot';
import { buildTextMetrics } from '../../metrics';

const platform = { OS: 'android' };
mock.module('react-native', () => ({
  Platform: platform,
  View: 'View',
  Text: 'Text',
  TouchableOpacity: 'TouchableOpacity',
  PanResponder: { create: () => ({ panHandlers: {} }) },
  StyleSheet: { absoluteFill: {}, create: (styles: unknown) => styles },
}));
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
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
});

const firstLine = { x: 0, y: 0, width: 40, height: 20 };
const metrics = [
  buildTextMetrics([{ ...firstLine, text: 'iiiiW', charXs: [0, 3, 6, 9, 12, 40] }]),
  buildTextMetrics([{ ...firstLine, text: 'hello' }, { ...firstLine, y: 20, text: 'WORLD' }]),
];

function mountDesktop(os: string) {
  platform.OS = os;
  let store!: SelectionStore;
  const handle = React.createRef<SelectionRootHandle>();
  function Blocks() {
    const ctx = useSelectionContext();
    useEffect(() => {
      store = ctx.store;
      const disposers = units.map(({ nodeId, unitId }, i) =>
        ctx.registerBlock({
          kind: 'text',
          nodeId,
          unitIds: [unitId],
          rect: { x: 12, y: i * 30, w: 100, h: metrics[i].lines.length * 20 },
          metrics: metrics[i],
        })
      );
      return () => disposers.forEach(dispose => dispose());
    }, [ctx]);
    return null;
  }
  act(() => {
    tree = create(
      <SelectionRoot units={units} toolbar={false} handleRef={handle}>
        <Blocks />
      </SelectionRoot>,
      { createNodeMock: () => ({ getBoundingClientRect: () => ({ x: 80, y: 130 }) }) }
    );
  });
  return { handle, store };
}

function input(phase: 'Start' | 'Move' | 'End', x: number, y: number, button = 0, buttons = 1) {
  const handler = platform.OS === 'macos'
    ? 'onTouch' + phase
    : { Start: 'onPointerDown', Move: 'onPointerMove', End: 'onPointerUp' }[phase];
  const root = tree.root.findAllByType('View')[0];
  act(() => root.props[handler]({
    nativeEvent: {
      pageX: 80 + 12 + x, pageY: 130 + y, button, buttons,
      identifier: 2, pointerId: 2, pointerType: 'mouse',
    },
  }));
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
