import type { SelectionPoint } from '../model';
import type { SelectionStore } from './state';

function samePoint(a: SelectionPoint, b: SelectionPoint): boolean {
  return a.nodeId === b.nodeId && a.unitId === b.unitId && a.offset === b.offset;
}

/** The down position remains the anchor, including when the drag reverses direction. */
export function createMouseSelection(store: SelectionStore) {
  let press: { id: string | number; anchor: SelectionPoint; focus: SelectionPoint } | null = null;
  function move(id: string | number, point: SelectionPoint | null) {
    if (!press || press.id !== id || !point || samePoint(press.focus, point)) return;
    if (!store.getSnapshot().range) store.beginAt(press.anchor);
    press.focus = point;
    store.extendTo(point);
  }
  function cancel() {
    if (!press) return;
    press = null;
    if (store.getSnapshot().units.length) store.commit();
    else store.clear();
  }
  return {
    tracking: () => press !== null,
    owns: (id: string | number) => press?.id === id,
    down(id: string | number, anchor: SelectionPoint | null) {
      if (store.getSnapshot().range) store.clear();
      press = anchor ? { id, anchor, focus: anchor } : null;
    },
    move,
    up(id: string | number, point: SelectionPoint | null) {
      if (press?.id !== id) return;
      move(id, point);
      cancel();
    },
    cancel,
  };
}
