import { describe, expect, test } from 'bun:test';
import {
  computeHandles,
  hitTestHandle,
  HANDLE_KNOB_RADIUS,
  type SelectionHandles,
} from '../../coordinator/handles';

const single = [{ x: 10, y: 100, w: 60, h: 20 }];
const multi = [
  { x: 30, y: 100, w: 70, h: 20 },
  { x: 0, y: 120, w: 100, h: 20 },
  { x: 0, y: 140, w: 40, h: 20 },
];

describe('computeHandles', () => {
  test('no rects means no handles', () => {
    expect(computeHandles([])).toBeNull();
  });

  test('a single rect anchors both handles to its two edges', () => {
    const handles = computeHandles(single) as SelectionHandles;
    expect(handles.start).toMatchObject({ edge: 'start', x: 10, y: 100, h: 20 });
    expect(handles.end).toMatchObject({ edge: 'end', x: 70, y: 100, h: 20 });
  });

  test('the knobs sit clear of the text, above and below', () => {
    // Both on one line: if either knob sat on the line itself they would
    // overlap the glyphs, and on a short selection each other.
    const handles = computeHandles(single) as SelectionHandles;
    expect(handles.start.knobY).toBe(100 - HANDLE_KNOB_RADIUS);
    expect(handles.end.knobY).toBe(120 + HANDLE_KNOB_RADIUS);
  });

  test('a multi-line selection uses the first and last rect', () => {
    const handles = computeHandles(multi) as SelectionHandles;
    expect(handles.start).toMatchObject({ x: 30, y: 100 });
    expect(handles.end).toMatchObject({ x: 40, y: 140 });
  });

  test('keeps clipped range edges out of drawing and hit-testing', () => {
    const handles = computeHandles([
      { x: 10, y: 100, w: 60, h: 20, startHandleVisible: false },
    ]) as SelectionHandles;
    expect(handles.start.visible).toBe(false);
    expect(handles.end.visible).toBe(true);
    expect(hitTestHandle({ x: 10, y: 94 }, handles)).toBeNull();
    expect(hitTestHandle({ x: 70, y: 126 }, handles)).toBe('end');
  });
});

describe('hitTestHandle', () => {
  const handles = computeHandles(single) as SelectionHandles;

  test('null handles are never hit', () => {
    expect(hitTestHandle({ x: 10, y: 100 }, null)).toBeNull();
  });

  test('a touch on a knob grabs that handle', () => {
    expect(hitTestHandle({ x: 10, y: 94 }, handles)).toBe('start');
    expect(hitTestHandle({ x: 70, y: 126 }, handles)).toBe('end');
  });

  test('a touch grabs from well outside the drawn knob', () => {
    // A 6pt knob is far below a comfortable touch target, so the grabbable
    // area is deliberately much larger than the drawn one.
    expect(hitTestHandle({ x: 10, y: 80 }, handles)).toBe('start');
  });

  test('a touch beyond the slop grabs nothing', () => {
    expect(hitTestHandle({ x: 10, y: 40 }, handles)).toBeNull();
    expect(hitTestHandle({ x: 300, y: 110 }, handles)).toBeNull();
  });

  test('the nearer handle wins when both are in range', () => {
    // A very short selection puts the two knobs within one touch radius.
    const tiny = computeHandles([{ x: 50, y: 100, w: 2, h: 10 }]) as SelectionHandles;
    expect(hitTestHandle({ x: 50, y: 90 }, tiny)).toBe('start');
    expect(hitTestHandle({ x: 52, y: 120 }, tiny)).toBe('end');
  });

  test('an explicit radius narrows the target', () => {
    expect(hitTestHandle({ x: 10, y: 80 }, handles, 4)).toBeNull();
  });
});
