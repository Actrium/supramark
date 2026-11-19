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
const first = { nodeId: 'first', unitId: 'first', offset: 2 };
const second = { nodeId: 'second', unitId: 'second', offset: 3 };

test('a reverse cross-block drag selects immediately from the original character', () => {
  const { mouse, store } = setup();
  mouse.down(3, second);
  mouse.move(3, first);
  expect(store.getSnapshot().range).toEqual({ anchor: second, focus: first });
  mouse.up(3, first);
  expect(store.getSnapshot().phase).toBe('selected');
  expect(store.getSnapshot().units.map(u => u.text)).toEqual(['rst', 'sec']);
});

test('unrelated pointers and late events cannot extend a drag', () => {
  const { mouse, store } = setup();
  mouse.down(3, first);
  mouse.move(4, second);
  mouse.up(4, second);
  expect(store.getSnapshot().phase).toBe('idle');
  mouse.move(3, second);
  mouse.cancel();
  mouse.move(3, first);
  expect(store.getSnapshot().range).toEqual({ anchor: first, focus: second });
  expect(mouse.tracking()).toBe(false);
});

test('clicks, returning to the anchor and blank starts leave no range', () => {
  const { mouse, store } = setup();
  mouse.down(1, first);
  mouse.up(1, second);
  expect(store.getSnapshot().phase).toBe('selected');
  mouse.down(1, second);
  mouse.up(1, second);
  expect(store.getSnapshot().range).toBeNull();
  mouse.down(1, second);
  mouse.move(1, first);
  mouse.up(1, second);
  expect(store.getSnapshot().range).toBeNull();
  mouse.down(1, null);
  mouse.up(1, first);
  expect(store.getSnapshot().range).toBeNull();
});
