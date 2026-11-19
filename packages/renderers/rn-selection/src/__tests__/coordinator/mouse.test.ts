import { expect, test } from 'bun:test';
import { createSelectionStore } from '../../coordinator/state';
import { createMouseSelection } from '../../coordinator/mouse';
import type { SelectionUnit } from '../../model';

function setup() {
  const units: SelectionUnit[] = ['first', 'second'].map(text => ({
    kind: 'text',
    nodeId: text,
    unitId: text,
    text,
    node: { type: 'text', value: text },
  }));
  const store = createSelectionStore(() => units);
  const mouse = createMouseSelection(store);
  return { mouse, store };
}
const position = { pageX: 10, pageY: 20 };
const first = { nodeId: 'first', unitId: 'first', offset: 2 };
const second = { nodeId: 'second', unitId: 'second', offset: 3 };

test('a reverse cross-block drag selects immediately from the original character', () => {
  const { mouse, store } = setup();
  mouse.down(3, second, position);
  mouse.move(3, first, position);
  expect(store.getSnapshot().range).toEqual({ anchor: second, focus: first });
  mouse.up(3, first, position);
  expect(store.getSnapshot().phase).toBe('selected');
  expect(store.getSnapshot().units.map(u => u.text)).toEqual(['rst', 'sec']);
});

test('unrelated pointers and late events cannot extend a drag', () => {
  const { mouse, store } = setup();
  mouse.down(3, first, position);
  mouse.move(4, second, position);
  mouse.up(4, second, position);
  expect(store.getSnapshot().phase).toBe('idle');
  mouse.move(3, second, position);
  mouse.cancel();
  mouse.move(3, first, position);
  expect(store.getSnapshot().range).toEqual({ anchor: first, focus: second });
  expect(mouse.tracking()).toBe(false);
});

test('clicks, returning to the anchor and blank starts leave no range', () => {
  const { mouse, store } = setup();
  mouse.down(1, first, position);
  mouse.up(1, second, position);
  expect(store.getSnapshot().phase).toBe('selected');
  mouse.down(1, second, position);
  mouse.up(1, second, position);
  expect(store.getSnapshot().range).toBeNull();
  mouse.down(1, second, position);
  mouse.move(1, first, position);
  mouse.up(1, second, position);
  expect(store.getSnapshot().range).toBeNull();
  mouse.down(1, null, position);
  mouse.up(1, first, position);
  expect(store.getSnapshot().range).toBeNull();
});

test('click tolerance uses the full two-dimensional displacement, even without a text hit', () => {
  const { mouse, store } = setup();
  mouse.down(1, first, position);
  mouse.move(2, null, { pageX: 50, pageY: 50 });
  expect(mouse.suppressesPress(1, { pageX: 10, pageY: 24 })).toBe(false);
  mouse.move(1, null, { pageX: 13, pageY: 23 });
  expect(store.getSnapshot().range).toBeNull();
  mouse.up(1, first, { ...position, timestamp: 30 });
  expect(mouse.suppressesPress(1, { ...position, timestamp: 30 })).toBe(true);
  expect(mouse.suppressesPress(1, { ...position, timestamp: 31 })).toBe(false);
});
