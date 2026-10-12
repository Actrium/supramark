import type { SelectionPoint, SelectionRange } from '../model';
import type { HandleEdge } from './handles';
import type { Point } from './hitTest';
import type { SelectionStore } from './state';

/**
 * The selection gesture, as a pure state machine.
 *
 * With the native command bridge gone, nothing on the platform side starts or
 * ends a selection any more: a long press used to travel
 * `UILongPressGestureRecognizer -> onTextLongPress -> store`, and the platform
 * owned every subsequent handle drag. All of that now happens here.
 *
 * Deliberately free of React and of timers. The React layer feeds it touch
 * events and calls `tick` from a `setTimeout`, which is what makes the
 * long-press threshold testable by advancing a number instead of waiting on a
 * clock.
 *
 * States:
 *
 * ```
 *  idle --touchStart--> pending --tick(threshold)--> extending --touchEnd--> idle
 *   |                     |  \                                                (committed)
 *   |                     |   `--move beyond tolerance--> idle (it was a scroll)
 *   |                     `--touchEnd--> idle (it was a tap: clears the selection)
 *   `--touchStart on a handle--> handle --touchEnd--> idle (committed)
 * ```
 */

export type GesturePhase = 'idle' | 'pending' | 'extending' | 'handle';

export interface SelectionGestureConfig {
  /** How long a press must be held before it selects. */
  longPressMs?: number;
  /**
   * Whether a held press selects a word at all (default: true). `false` keeps
   * only tap-to-clear and handle drags; the host starts selections through its
   * own entry points (a menu item, say), so a hold must never select — not via
   * the timer, not via the move path, and not via the touch-end fallback.
   */
  longPress?: boolean;
  /**
   * How far a finger may drift during that hold before the press is abandoned.
   * Small on purpose: anything larger and a slow scroll starts selecting text.
   */
  moveTolerance?: number;
}

export const DEFAULT_LONG_PRESS_MS = 400;
export const DEFAULT_MOVE_TOLERANCE = 10;

export interface SelectionGestureDeps {
  store: Pick<SelectionStore, 'beginAt' | 'extendTo' | 'commit' | 'clear' | 'getSnapshot'>;
  /** Root-space point -> document point; null when nothing is laid out yet. */
  pointAt(point: Point): SelectionPoint | null;
  /** Root-space point -> the word range under it; null when there is no text. */
  wordAt(point: Point): SelectionRange | null;
  /** Which handle a touch grabs, if any. */
  handleAt(point: Point): HandleEdge | null;
  /** The endpoint that must stay fixed while the visible handle moves. */
  fixedPointForHandle(edge: HandleEdge, range: SelectionRange): SelectionPoint;
  /** Notified whenever the phase changes, so React can re-render. */
  onPhaseChange?(phase: GesturePhase): void;
  config?: SelectionGestureConfig;
}

export interface SelectionGesture {
  phase(): GesturePhase;
  /**
   * True while the gesture owns the touch. The React layer returns this from
   * `onStartShouldSetResponder` / `onMoveShouldSetResponder` so an enclosing
   * `ScrollView` keeps scrolling until the moment we actually start selecting,
   * and stops competing with us afterwards.
   */
  isActive(): boolean;
  /** True when a press is being timed; the React layer arms its timer on this. */
  isPending(): boolean;
  touchStart(point: Point, timeMs: number): void;
  /** Start dragging a known handle edge from a dedicated handle responder. */
  handleStart(edge: HandleEdge, timeMs: number): void;
  touchMove(point: Point, timeMs: number): void;
  touchEnd(point: Point, timeMs: number): void;
  /** Advance the long-press timer. Safe to call at any time. */
  tick(timeMs: number): void;
  /** Abandon the gesture, committing anything already selected. */
  cancel(): void;
}

function distanceSquared(a: Point, b: Point): number {
  const dx = a.x - b.x;
  const dy = a.y - b.y;
  return dx * dx + dy * dy;
}

export function createSelectionGesture(deps: SelectionGestureDeps): SelectionGesture {
  const longPressMs = deps.config?.longPressMs ?? DEFAULT_LONG_PRESS_MS;
  const longPressEnabled = deps.config?.longPress !== false;
  const moveTolerance = deps.config?.moveTolerance ?? DEFAULT_MOVE_TOLERANCE;
  const toleranceSquared = moveTolerance * moveTolerance;

  let phase: GesturePhase = 'idle';
  let pressPoint: Point | null = null;
  let pressTime = 0;
  /**
   * While a handle is being dragged, the *other* edge of the selection stays
   * put. Captured once on grab rather than re-read per move: `store.beginAt`
   * rewrites the anchor on every frame, so re-deriving it from the live
   * snapshot would make the fixed edge chase the moving one.
   */
  let fixedPoint: SelectionPoint | null = null;

  const setPhase = (next: GesturePhase): void => {
    if (phase === next) return;
    phase = next;
    deps.onPhaseChange?.(next);
  };

  const reset = (): void => {
    pressPoint = null;
    fixedPoint = null;
    setPhase('idle');
  };

  /**
   * Fire the long press: select the word under the finger and commit it.
   * Returns whether a selection actually started.
   */
  const beginWordSelection = (): boolean => {
    const at = pressPoint;
    if (at === null) {
      reset();
      return false;
    }
    const word = deps.wordAt(at);
    if (word === null) {
      if (deps.store.getSnapshot().range !== null) deps.store.clear();
      reset();
      return false;
    }
    deps.store.beginAt(word.anchor);
    deps.store.extendTo(word.focus);
    // Commit immediately so the toolbar appears the moment the press lands,
    // rather than only after the finger lifts. A drag that continues from here
    // re-enters `selecting` through `extendTo` and hides it again.
    deps.store.commit();
    setPhase('extending');
    return true;
  };

  const beginHandleDrag = (edge: HandleEdge): void => {
    const range = deps.store.getSnapshot().range;
    // `anchor` / `focus` preserve gesture direction and may be reversed after
    // one handle crosses the other. Ask the document-aware dependency for the
    // endpoint opposite the VISUAL start/end handle instead of assuming anchor
    // is always the leading edge.
    fixedPoint = range === null ? null : deps.fixedPointForHandle(edge, range);
    if (fixedPoint === null) {
      reset();
      return;
    }
    pressPoint = null;
    setPhase('handle');
  };

  const tick = (timeMs: number): void => {
    if (!longPressEnabled) return;
    if (phase !== 'pending') return;
    if (timeMs - pressTime < longPressMs) return;
    beginWordSelection();
  };

  // Every member is a closure over the state above and uses no `this`, so the
  // gesture can be destructured or handed straight to a JSX prop.
  return {
    phase: () => phase,
    isActive: () => phase === 'extending' || phase === 'handle',
    isPending: () => phase === 'pending',

    handleStart(edge) {
      beginHandleDrag(edge);
    },

    touchStart(point, timeMs) {
      if (phase === 'extending' || phase === 'handle') return;
      const edge = deps.handleAt(point);
      if (edge !== null) {
        beginHandleDrag(edge);
        return;
      }
      pressPoint = point;
      pressTime = timeMs;
      setPhase('pending');
    },

    touchMove(point, timeMs) {
      if (phase === 'pending') {
        if (pressPoint !== null && distanceSquared(point, pressPoint) > toleranceSquared) {
          if (longPressEnabled && timeMs - pressTime >= longPressMs && beginWordSelection()) {
            pressPoint = null;
            const at = deps.pointAt(point);
            if (at !== null) deps.store.extendTo(at);
            return;
          }
          // The finger is travelling: this is a scroll, not a press. Give up
          // silently — we never claimed the responder, so the scroll continues.
          reset();
          return;
        }
        // Still holding still: the threshold may have elapsed between events.
        tick(timeMs);
        return;
      }
      if (phase === 'extending') {
        if (pressPoint !== null) {
          if (distanceSquared(point, pressPoint) <= toleranceSquared) return;
          pressPoint = null;
        }
        const at = deps.pointAt(point);
        if (at !== null) deps.store.extendTo(at);
        return;
      }
      if (phase === 'handle' && fixedPoint !== null) {
        const at = deps.pointAt(point);
        if (at === null) return;
        deps.store.beginAt(fixedPoint);
        deps.store.extendTo(at);
      }
    },

    touchEnd(point, timeMs) {
      if (phase === 'pending') {
        const held = timeMs - pressTime;
        // Held long enough for the threshold to have passed but the tick never
        // fired (a fast tap-and-hold on a busy frame): honour the press — unless
        // the long press is switched off, in which case a hold is not a tap.
        if (longPressEnabled && held >= longPressMs && beginWordSelection()) {
          deps.store.commit();
          reset();
          return;
        }
        if (!longPressEnabled && held >= longPressMs) {
          // Host-menu mode: the hold belonged to the host's long-press menu, and
          // the matching touchEnd was swallowed by the menu overlay. A touchEnd
          // arriving long after the press, once the menu closes, is that
          // swallowed release falling back in — it must not tap-clear the
          // selection the menu item just created.
          reset();
          return;
        }
        // An ordinary tap dismisses the selection. This is the whole of the
        // "tap outside" behaviour: with no native selection to fall out of
        // sync with, one code path clears both the state and the UI.
        deps.store.clear();
        reset();
        return;
      }
      if (phase === 'extending' || phase === 'handle') {
        deps.store.commit();
        reset();
      }
    },

    tick,

    cancel() {
      if (phase === 'extending' || phase === 'handle') deps.store.commit();
      reset();
    },
  };
}
