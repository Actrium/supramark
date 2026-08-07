import type { LocalRect } from '../metrics';

/**
 * Drag-handle geometry for a self-drawn selection.
 *
 * The platform used to supply these: pushing a range into the native text view
 * made UIKit / Android draw their own grab handles. Now that the selection UI
 * is ours end to end (see `SELECTION_PLAN.md`), the handles are two views we
 * position from the selection's own highlight rectangles — which means they
 * look and behave the same on both platforms, and the same code will serve web.
 *
 * All coordinates are in `SelectionRoot` space, like the rects they derive
 * from. This module is pure geometry: it neither draws nor tracks gestures.
 */

/** Radius of the round knob drawn at a handle's outer end. */
export const HANDLE_KNOB_RADIUS = 6;

/** Touch slop around a knob centre, in points. Generous by design: a 6pt knob
 * is far below the ~44pt minimum comfortable touch target, so the grabbable
 * area has to be much larger than the drawn one. */
export const HANDLE_TOUCH_RADIUS = 22;

export type HandleEdge = 'start' | 'end';

export interface HandleGeometry {
  edge: HandleEdge;
  /** The selection edge itself: a zero-width caret line. */
  x: number;
  y: number;
  h: number;
  /** Centre of the round knob, placed clear of the text line. */
  knobX: number;
  knobY: number;
  /** False when the real range edge is outside a nested viewport clip. */
  visible: boolean;
}

export interface SelectionHandles {
  start: HandleGeometry;
  end: HandleGeometry;
}

/**
 * Derive both handles from the selection's highlight rectangles, which
 * `computeSelectionRects` returns in document order. The start handle sits at
 * the leading edge of the first rect with its knob *above* the line, the end
 * handle at the trailing edge of the last rect with its knob *below* — the
 * convention on both iOS and Android, and the reason the two knobs never
 * collide on a single-line selection.
 *
 * Returns null when there is nothing to hold on to.
 */
interface HandleSourceRect extends LocalRect {
  startHandleVisible?: boolean;
  endHandleVisible?: boolean;
}

export function computeHandles(rects: readonly HandleSourceRect[]): SelectionHandles | null {
  if (rects.length === 0) return null;
  const first = rects[0];
  const last = rects[rects.length - 1];
  return {
    start: {
      edge: 'start',
      x: first.x,
      y: first.y,
      h: first.h,
      knobX: first.x,
      knobY: first.y - HANDLE_KNOB_RADIUS,
      visible: first.startHandleVisible !== false,
    },
    end: {
      edge: 'end',
      x: last.x + last.w,
      y: last.y,
      h: last.h,
      knobX: last.x + last.w,
      knobY: last.y + last.h + HANDLE_KNOB_RADIUS,
      visible: last.endHandleVisible !== false,
    },
  };
}

/**
 * Which handle, if any, a touch grabs. Distances are compared squared to keep
 * the hot path free of `Math.sqrt`; a tie goes to `end`, because a collapsed or
 * very short selection is nearly always refined by dragging its trailing edge.
 */
export function hitTestHandle(
  point: { x: number; y: number },
  handles: SelectionHandles | null,
  radius: number = HANDLE_TOUCH_RADIUS
): HandleEdge | null {
  if (handles === null) return null;
  const limit = radius * radius;
  const distanceTo = (handle: HandleGeometry): number => {
    const dx = point.x - handle.knobX;
    const dy = point.y - handle.knobY;
    return dx * dx + dy * dy;
  };
  const toStart = handles.start.visible ? distanceTo(handles.start) : Infinity;
  const toEnd = handles.end.visible ? distanceTo(handles.end) : Infinity;
  if (toEnd <= limit && toEnd <= toStart) return 'end';
  if (toStart <= limit) return 'start';
  return null;
}
