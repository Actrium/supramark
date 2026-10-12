import type { SelectionPoint, SelectionRange, SelectionUnit } from '../model';
import {
  buildUnitIndex,
  clampSelectionRange,
  locateSelectionPoint,
  resolveSelectionRange,
  type SelectionUnitIndex,
} from '../resolve';

export type SelectionPhase = 'idle' | 'selecting' | 'selected';

/**
 * Immutable snapshot of the selection. `units` are the covered units derived by
 * the M1 `resolveSelectionRange`; they are empty when idle or collapsed, so UI
 * must treat `phase` (not `units`) as the source of truth for "is selecting".
 */
export interface SelectionSnapshot {
  phase: SelectionPhase;
  range: SelectionRange | null;
  units: readonly SelectionUnit[];
}

export interface SelectionStore {
  // Property-style signatures: every member is a closure with no `this`, safe
  // to pass unbound (e.g. straight into `useSyncExternalStore`).
  getSnapshot: () => SelectionSnapshot;
  subscribe: (l: () => void) => () => void;
  beginAt: (point: SelectionPoint) => void;
  extendTo: (point: SelectionPoint) => void;
  commit: () => void;
  clear: () => void;
}

const EMPTY_IDLE: SelectionSnapshot = { phase: 'idle', range: null, units: [] };

/**
 * External-store selection state machine (`idle -> selecting -> selected`). This
 * module owns only transitions plus snapshot caching; covered-unit derivation is
 * delegated entirely to the M1 `resolveSelectionRange`.
 *
 * `getSnapshot` returns a cached object with a stable reference between actions,
 * so `useSyncExternalStore` never tears; a NEW object is minted only inside
 * `recompute`, right before listeners fire.
 */
export function createSelectionStore(getUnits: () => readonly SelectionUnit[]): SelectionStore {
  let anchor: SelectionPoint | null = null;
  let focus: SelectionPoint | null = null;
  let phase: SelectionPhase = 'idle';
  let snapshot: SelectionSnapshot = EMPTY_IDLE;
  const listeners = new Set<() => void>();

  function currentRange(): SelectionRange | null {
    if (phase === 'idle' || anchor === null || focus === null) return null;
    return { anchor, focus };
  }

  // Memoize the unit index by array identity. Every drag move calls
  // `recompute`, and `resolveSelectionRange` would otherwise rebuild a
  // document-wide index each time: measured at 20 000 units x 60 moves,
  // 942-1204 ms in Bun on desktop, i.e. ~16-20 ms per move before Hermes and
  // the bridge are involved. Keyed on the array reference rather than taking
  // the registry's index, because `SelectionRoot` refreshes `unitsRef` during
  // render while `registry.setUnits` runs in an effect — a gesture firing in
  // between would otherwise resolve against a stale index.
  let indexedUnits: readonly SelectionUnit[] | null = null;
  let unitIndex: SelectionUnitIndex | null = null;

  function indexFor(units: readonly SelectionUnit[]): SelectionUnitIndex {
    if (unitIndex === null || indexedUnits !== units) {
      indexedUnits = units;
      unitIndex = buildUnitIndex(units);
    }
    return unitIndex;
  }

  function recompute(): void {
    const range = currentRange();
    let units: SelectionUnit[] = [];
    if (range) {
      const current = getUnits();
      units = resolveSelectionRange(current, range, indexFor(current));
    }
    snapshot = { phase, range, units };
    for (const listener of listeners) listener();
  }

  return {
    getSnapshot() {
      return snapshot;
    },
    subscribe(l) {
      listeners.add(l);
      return () => {
        listeners.delete(l);
      };
    },
    beginAt(point) {
      // A press inside a hard boundary (an inline image, a disabled-feature
      // object) starts nothing: those are selection limits, not text.
      const current = getUnits();
      const located = locateSelectionPoint(indexFor(current), point);
      if (indexFor(current).regionOf[located.unitIndex] === -1) return;
      anchor = point;
      focus = point;
      phase = 'selecting';
      // Collapsed anchor === focus: resolveSelectionRange returns [] (M1).
      recompute();
    },
    extendTo(point) {
      // A drag before a begin is ignored: phase, not focus, gates selection.
      if (phase === 'idle' || anchor === null) return;
      // Clamp along the anchor so a drag across a hard boundary stops at its
      // edge. The stored range is what every layer derives from — geometry,
      // highlighting and copying — and clampSelectionRange is idempotent, so
      // re-clamping inside resolve keeps them in agreement either way.
      const current = getUnits();
      const clamped = clampSelectionRange(indexFor(current), { anchor, focus: point });
      anchor = clamped.anchor;
      focus = clamped.focus;
      phase = 'selecting';
      recompute();
    },
    commit() {
      if (phase === 'selecting' && anchor !== null && focus !== null) {
        phase = 'selected';
      }
      // Covered units are unchanged; a collapsed selection still commits.
      recompute();
    },
    clear() {
      anchor = null;
      focus = null;
      phase = 'idle';
      recompute();
    },
  };
}
