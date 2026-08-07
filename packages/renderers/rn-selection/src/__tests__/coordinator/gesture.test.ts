import { describe, expect, test } from 'bun:test';
import type { SelectionPoint, SelectionRange } from '../../model';
import {
  createSelectionGesture,
  DEFAULT_LONG_PRESS_MS,
  type SelectionGestureDeps,
} from '../../coordinator/gesture';
import type { HandleEdge } from '../../coordinator/handles';
import type { Point } from '../../coordinator/hitTest';
import type { SelectionSnapshot } from '../../coordinator/state';

const point = (offset: number): SelectionPoint => ({ nodeId: 'p', unitId: 'p#0', offset });

/**
 * A recording stand-in for the selection store. The gesture machine is judged
 * entirely by the sequence of store calls it makes, so the log is the assertion
 * surface.
 */
function fakeStore() {
  const calls: string[] = [];
  let range: SelectionRange | null = null;
  return {
    calls,
    setRange(next: SelectionRange | null) {
      range = next;
    },
    getSnapshot: (): SelectionSnapshot => ({ phase: 'selected', range, units: [] }),
    beginAt: (p: SelectionPoint) => {
      calls.push(`beginAt:${p.offset}`);
    },
    extendTo: (p: SelectionPoint) => {
      calls.push(`extendTo:${p.offset}`);
    },
    commit: () => {
      calls.push('commit');
    },
    clear: () => {
      calls.push('clear');
    },
  };
}

interface Harness {
  store: ReturnType<typeof fakeStore>;
  phases: string[];
  gesture: ReturnType<typeof createSelectionGesture>;
}

function harness(overrides: Partial<SelectionGestureDeps> = {}): Harness {
  const store = fakeStore();
  const phases: string[] = [];
  const gesture = createSelectionGesture({
    store,
    // A point's document offset is just its x, so assertions read directly.
    pointAt: (p: Point) => point(p.x),
    wordAt: (p: Point) => ({ anchor: point(p.x), focus: point(p.x + 5) }),
    handleAt: () => null,
    onPhaseChange: phase => phases.push(phase),
    ...overrides,
  });
  return { store, phases, gesture };
}

describe('long press', () => {
  test('a held press selects the word under the finger and commits', () => {
    const { store, gesture } = harness();
    gesture.touchStart({ x: 10, y: 0 }, 0);
    expect(gesture.phase()).toBe('pending');
    expect(store.calls).toEqual([]);

    gesture.tick(DEFAULT_LONG_PRESS_MS);
    // Committing immediately is what makes the toolbar appear on the press
    // rather than only on release.
    expect(store.calls).toEqual(['beginAt:10', 'extendTo:15', 'commit']);
    expect(gesture.phase()).toBe('extending');
  });

  test('a tick before the threshold does nothing', () => {
    const { store, gesture } = harness();
    gesture.touchStart({ x: 10, y: 0 }, 0);
    gesture.tick(DEFAULT_LONG_PRESS_MS - 1);
    expect(store.calls).toEqual([]);
    expect(gesture.phase()).toBe('pending');
  });

  test('a custom threshold is honoured', () => {
    const { store, gesture } = harness({ config: { longPressMs: 100 } });
    gesture.touchStart({ x: 10, y: 0 }, 0);
    gesture.tick(100);
    expect(store.calls).toEqual(['beginAt:10', 'extendTo:15', 'commit']);
  });

  test('a press over untextured space selects nothing and gives up', () => {
    const { store, gesture } = harness({ wordAt: () => null });
    gesture.touchStart({ x: 10, y: 0 }, 0);
    gesture.tick(DEFAULT_LONG_PRESS_MS);
    expect(store.calls).toEqual([]);
    expect(gesture.phase()).toBe('idle');
  });

  test('a blank long press clears an existing selection when the timer wins', () => {
    const { store, gesture } = harness({ wordAt: () => null });
    store.setRange({ anchor: point(0), focus: point(5) });
    gesture.touchStart({ x: 10, y: 0 }, 0);
    gesture.tick(DEFAULT_LONG_PRESS_MS);
    expect(store.calls).toEqual(['clear']);
    expect(gesture.phase()).toBe('idle');
  });

  test('moving beyond the tolerance abandons the press, so a scroll stays a scroll', () => {
    const { store, gesture } = harness();
    gesture.touchStart({ x: 10, y: 0 }, 0);
    gesture.touchMove({ x: 10, y: 40 }, 50);
    expect(gesture.phase()).toBe('idle');
    gesture.tick(DEFAULT_LONG_PRESS_MS);
    expect(store.calls).toEqual([]);
  });

  test('a small drift inside the tolerance keeps the press alive', () => {
    const { store, gesture } = harness();
    gesture.touchStart({ x: 10, y: 0 }, 0);
    gesture.touchMove({ x: 12, y: 2 }, 50);
    expect(gesture.phase()).toBe('pending');
    gesture.tick(DEFAULT_LONG_PRESS_MS);
    expect(store.calls).toEqual(['beginAt:10', 'extendTo:15', 'commit']);
  });

  test('a stationary move after the press lands does not collapse the selected word', () => {
    const { store, gesture } = harness();
    gesture.touchStart({ x: 10, y: 0 }, 0);
    gesture.tick(DEFAULT_LONG_PRESS_MS);
    store.calls.length = 0;

    gesture.touchMove({ x: 12, y: 2 }, DEFAULT_LONG_PRESS_MS + 20);

    expect(store.calls).toEqual([]);
    expect(gesture.phase()).toBe('extending');
  });

  test('a drag after the press lands extends only after leaving the hold slop', () => {
    const { store, gesture } = harness();
    gesture.touchStart({ x: 10, y: 0 }, 0);
    gesture.tick(DEFAULT_LONG_PRESS_MS);
    store.calls.length = 0;

    gesture.touchMove({ x: 25, y: 0 }, DEFAULT_LONG_PRESS_MS + 20);

    expect(store.calls).toEqual(['extendTo:25']);
  });

  test('a move past the threshold fires the press without waiting for a tick', () => {
    const { store, gesture } = harness();
    gesture.touchStart({ x: 10, y: 0 }, 0);
    gesture.touchMove({ x: 10, y: 0 }, DEFAULT_LONG_PRESS_MS);
    expect(store.calls).toEqual(['beginAt:10', 'extendTo:15', 'commit']);
  });

  test('a move after a delayed threshold starts selection instead of discarding it', () => {
    const { store, gesture } = harness();
    gesture.touchStart({ x: 10, y: 0 }, 0);

    gesture.touchMove({ x: 25, y: 0 }, DEFAULT_LONG_PRESS_MS + 20);

    expect(store.calls).toEqual(['beginAt:10', 'extendTo:15', 'commit', 'extendTo:25']);
    expect(gesture.phase()).toBe('extending');
  });
});

describe('tap', () => {
  test('a short tap clears the selection', () => {
    // The whole of "tap outside to dismiss": one path, one state.
    const { store, gesture } = harness();
    gesture.touchStart({ x: 10, y: 0 }, 0);
    gesture.touchEnd({ x: 10, y: 0 }, 50);
    expect(store.calls).toEqual(['clear']);
    expect(gesture.phase()).toBe('idle');
  });

  test('a release after the threshold selects instead of clearing', () => {
    // The timer can be starved on a busy frame; the release must still honour
    // a press the user genuinely held.
    const { store, gesture } = harness();
    gesture.touchStart({ x: 10, y: 0 }, 0);
    gesture.touchEnd({ x: 10, y: 0 }, DEFAULT_LONG_PRESS_MS + 10);
    expect(store.calls).toEqual(['beginAt:10', 'extendTo:15', 'commit', 'commit']);
    expect(store.calls).not.toContain('clear');
  });
});

describe('drag to extend', () => {
  test('a drag after the press moves the focus and commits on release', () => {
    const { store, gesture } = harness();
    gesture.touchStart({ x: 10, y: 0 }, 0);
    gesture.tick(DEFAULT_LONG_PRESS_MS);
    store.calls.length = 0;

    gesture.touchMove({ x: 40, y: 0 }, 500);
    gesture.touchMove({ x: 60, y: 0 }, 520);
    gesture.touchEnd({ x: 60, y: 0 }, 540);

    expect(store.calls).toEqual(['extendTo:40', 'extendTo:60', 'commit']);
    expect(gesture.phase()).toBe('idle');
  });

  test('the gesture owns the touch only once it is selecting', () => {
    // This is what lets an enclosing ScrollView scroll until the press lands.
    const { gesture } = harness();
    gesture.touchStart({ x: 10, y: 0 }, 0);
    expect(gesture.isActive()).toBe(false);
    gesture.tick(DEFAULT_LONG_PRESS_MS);
    expect(gesture.isActive()).toBe(true);
  });

  test('a drag with no resolvable point is ignored rather than collapsing', () => {
    const { store, gesture } = harness({ pointAt: () => null });
    gesture.touchStart({ x: 10, y: 0 }, 0);
    gesture.tick(DEFAULT_LONG_PRESS_MS);
    store.calls.length = 0;
    gesture.touchMove({ x: 40, y: 0 }, 500);
    expect(store.calls).toEqual([]);
  });
});

describe('handle drag', () => {
  const existing: SelectionRange = { anchor: point(4), focus: point(9) };

  function withHandle(edge: HandleEdge) {
    const h = harness({ handleAt: () => edge });
    h.store.setRange(existing);
    return h;
  }

  test('grabbing the end handle pins the anchor and moves the focus', () => {
    const { store, gesture } = withHandle('end');
    gesture.touchStart({ x: 100, y: 0 }, 0);
    expect(gesture.phase()).toBe('handle');
    expect(gesture.isActive()).toBe(true);

    gesture.touchMove({ x: 30, y: 0 }, 20);
    expect(store.calls).toEqual(['beginAt:4', 'extendTo:30']);
  });

  test('a dedicated handle responder can start a drag without root hit-testing', () => {
    const { store, gesture } = harness({ handleAt: () => null });
    store.setRange(existing);
    gesture.handleStart('end', 0);
    gesture.touchMove({ x: 30, y: 0 }, 20);
    expect(store.calls).toEqual(['beginAt:4', 'extendTo:30']);
  });

  test('a duplicate root touchStart during a handle drag does not reset to pending', () => {
    const { store, gesture } = harness({ handleAt: () => null });
    store.setRange(existing);
    gesture.handleStart('end', 0);

    gesture.touchStart({ x: 100, y: 0 }, 5);
    gesture.touchMove({ x: 30, y: 0 }, 20);

    expect(gesture.phase()).toBe('handle');
    expect(store.calls).toEqual(['beginAt:4', 'extendTo:30']);
  });

  test('grabbing the start handle pins the FOCUS instead, so either edge can lead', () => {
    const { store, gesture } = withHandle('start');
    gesture.touchStart({ x: 0, y: 0 }, 0);
    gesture.touchMove({ x: 2, y: 0 }, 20);
    expect(store.calls).toEqual(['beginAt:9', 'extendTo:2']);
  });

  test('the fixed edge does not chase the moving one across frames', () => {
    // `beginAt` rewrites the anchor every frame, so re-reading the live range
    // per move would drag both edges together.
    const { store, gesture } = withHandle('end');
    gesture.touchStart({ x: 100, y: 0 }, 0);
    gesture.touchMove({ x: 30, y: 0 }, 20);
    gesture.touchMove({ x: 50, y: 0 }, 40);
    expect(store.calls).toEqual(['beginAt:4', 'extendTo:30', 'beginAt:4', 'extendTo:50']);
  });

  test('releasing commits', () => {
    const { store, gesture } = withHandle('end');
    gesture.touchStart({ x: 100, y: 0 }, 0);
    gesture.touchMove({ x: 30, y: 0 }, 20);
    store.calls.length = 0;
    gesture.touchEnd({ x: 30, y: 0 }, 40);
    expect(store.calls).toEqual(['commit']);
    expect(gesture.phase()).toBe('idle');
  });

  test('a handle hit with no live range falls back to idle', () => {
    const h = harness({ handleAt: () => 'end' });
    h.store.setRange(null);
    h.gesture.touchStart({ x: 100, y: 0 }, 0);
    expect(h.gesture.phase()).toBe('idle');
    expect(h.store.calls).toEqual([]);
  });
});

describe('cancel', () => {
  test('cancelling mid-drag keeps what was selected', () => {
    const { store, gesture } = harness();
    gesture.touchStart({ x: 10, y: 0 }, 0);
    gesture.tick(DEFAULT_LONG_PRESS_MS);
    store.calls.length = 0;
    gesture.cancel();
    expect(store.calls).toEqual(['commit']);
    expect(gesture.phase()).toBe('idle');
  });

  test('cancelling a pending press touches nothing', () => {
    const { store, gesture } = harness();
    gesture.touchStart({ x: 10, y: 0 }, 0);
    gesture.cancel();
    expect(store.calls).toEqual([]);
    expect(gesture.phase()).toBe('idle');
  });
});

describe('phase reporting', () => {
  test('every transition is announced once', () => {
    const { gesture, phases } = harness();
    gesture.touchStart({ x: 10, y: 0 }, 0);
    gesture.tick(DEFAULT_LONG_PRESS_MS);
    gesture.touchMove({ x: 20, y: 0 }, 500);
    gesture.touchEnd({ x: 20, y: 0 }, 520);
    expect(phases).toEqual(['pending', 'extending', 'idle']);
  });

  test('isPending tracks the timed window the React layer arms its timer on', () => {
    const { gesture } = harness();
    expect(gesture.isPending()).toBe(false);
    gesture.touchStart({ x: 10, y: 0 }, 0);
    expect(gesture.isPending()).toBe(true);
    gesture.tick(DEFAULT_LONG_PRESS_MS);
    expect(gesture.isPending()).toBe(false);
  });
});
