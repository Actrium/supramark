import type { SelectionPoint } from '../model';
import type { SelectionStore } from './state';

export interface PointerPosition {
  pageX: number;
  pageY: number;
  timestamp?: number;
}

interface MousePress {
  id: string | number;
  anchor: SelectionPoint;
  focus: SelectionPoint;
  origin: PointerPosition;
  dragged: boolean;
}

const CLICK_TOLERANCE_POINTS = 4; // React Native window coordinates are in logical points.

function outsideClickTolerance(origin: PointerPosition, current: PointerPosition): boolean {
  return (
    Math.hypot(current.pageX - origin.pageX, current.pageY - origin.pageY) > CLICK_TOLERANCE_POINTS
  );
}

function samePoint(a: SelectionPoint, b: SelectionPoint): boolean {
  return a.nodeId === b.nodeId && a.unitId === b.unitId && a.offset === b.offset;
}

/** The down position remains the anchor, including when the drag reverses direction. */
export function createMouseSelection(store: SelectionStore) {
  let press: MousePress | null = null;
  let dragRelease: { id: string | number; timestamp: number } | null = null;
  function move(id: string | number, point: SelectionPoint | null, position: PointerPosition) {
    if (!press || press.id !== id) return;
    // Two visually distant bidi carets can share one logical text offset.
    press.dragged ||= outsideClickTolerance(press.origin, position);
    if (!point || samePoint(press.focus, point)) return;
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
    suppressesPress(id: string | number, position: PointerPosition) {
      if (press && press.id === id)
        return press.dragged || outsideClickTolerance(press.origin, position);
      return (
        dragRelease !== null &&
        dragRelease.id === id &&
        dragRelease.timestamp === position.timestamp
      );
    },
    down(id: string | number, anchor: SelectionPoint | null, position: PointerPosition) {
      if (store.getSnapshot().range) store.clear();
      dragRelease = null;
      const origin = { pageX: position.pageX, pageY: position.pageY };
      press = anchor ? { id, anchor, focus: anchor, origin, dragged: false } : null;
    },
    move,
    up(id: string | number, point: SelectionPoint | null, position: PointerPosition) {
      if (press?.id !== id) return;
      move(id, point, position);
      const { timestamp } = position;
      dragRelease = press.dragged && timestamp !== undefined ? { id, timestamp } : null;
      cancel();
    },
    cancel,
  };
}
