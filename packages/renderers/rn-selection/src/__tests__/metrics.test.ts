import { describe, expect, test } from 'bun:test';
import {
  buildTextMetrics,
  lineAtY,
  offsetInsideLineX,
  offsetAtLineX,
  offsetAtLocalPoint,
  rectsForRange,
  xForOffset,
  type SegmentTextMetrics,
} from '../metrics';

/** Two 20pt lines: 'hello ' over 60pt, 'world' over 50pt. */
const wrapped = (): SegmentTextMetrics =>
  buildTextMetrics([
    { text: 'hello ', x: 0, y: 0, width: 60, height: 20 },
    { text: 'world', x: 0, y: 20, width: 50, height: 20 },
  ]);

describe('buildTextMetrics', () => {
  test('accumulates offsets across lines', () => {
    const metrics = wrapped();
    expect(metrics.textLength).toBe(11);
    expect(metrics.lines.map(l => [l.start, l.end, l.visibleEnd])).toEqual([
      [0, 6, 6],
      [6, 11, 11],
    ]);
  });

  test('an explicit newline is excluded from the visible span', () => {
    // The terminator occupies an offset in the text stream but no width on the
    // line, so hit-testing past the end of the line must not land after it.
    const metrics = buildTextMetrics([
      { text: 'a\n', x: 0, y: 0, width: 10, height: 20 },
      { text: 'b', x: 0, y: 20, width: 10, height: 20 },
    ]);
    expect(metrics.lines[0]).toMatchObject({ start: 0, end: 2, visibleEnd: 1 });
    expect(metrics.textLength).toBe(3);
  });

  test('a CRLF terminator counts as two units', () => {
    const metrics = buildTextMetrics([{ text: 'a\r\n', x: 0, y: 0, width: 10, height: 20 }]);
    expect(metrics.lines[0]).toMatchObject({ end: 3, visibleEnd: 1 });
  });

  test('no lines yields an empty table', () => {
    expect(buildTextMetrics([])).toEqual({ lines: [], textLength: 0 });
  });
});

describe('xForOffset / offsetAtLineX without per-character data', () => {
  const line = wrapped().lines[1]; // 'world', 5 chars over 50pt from x=0

  test('interpolates evenly across the line', () => {
    expect(xForOffset(line, 6)).toBe(0);
    expect(xForOffset(line, 8)).toBe(20);
    expect(xForOffset(line, 11)).toBe(50);
  });

  test('clamps an out-of-range offset to the line ends', () => {
    expect(xForOffset(line, 0)).toBe(0);
    expect(xForOffset(line, 99)).toBe(50);
  });

  test('a point resolves to the nearer character edge', () => {
    expect(offsetAtLineX(line, 24)).toBe(8); // left half of char 2
    expect(offsetAtLineX(line, 26)).toBe(9); // right half of char 2
  });

  test('a point can resolve to the character it sits inside', () => {
    expect(offsetInsideLineX(line, 24)).toBe(8);
    expect(offsetInsideLineX(line, 26)).toBe(8);
    expect(offsetInsideLineX(line, 500)).toBe(10);
  });

  test('a point outside the line clamps to its ends', () => {
    expect(offsetAtLineX(line, -50)).toBe(6);
    expect(offsetAtLineX(line, 500)).toBe(11);
  });

  test('offset and x invert each other on character boundaries', () => {
    for (let offset = 6; offset <= 11; offset++) {
      expect(offsetAtLineX(line, xForOffset(line, offset))).toBe(offset);
    }
  });
});

describe('xForOffset / offsetAtLineX with per-character data', () => {
  // Proportional text: 'iWi' where W is much wider than i. Interpolation would
  // put every boundary at a third of the line; the table puts them where the
  // glyphs actually end.
  const metrics = buildTextMetrics([{ text: 'iWi', x: 0, y: 0, width: 60, height: 20 }]);
  const line = { ...metrics.lines[0], charXs: [0, 10, 50, 60] };

  test('uses the table rather than interpolating', () => {
    expect(xForOffset(line, 1)).toBe(10);
    expect(xForOffset(line, 2)).toBe(50);
  });

  test('hit-testing splits each character at its own midpoint', () => {
    expect(offsetAtLineX(line, 9)).toBe(1); // right half of the narrow 'i'
    expect(offsetAtLineX(line, 25)).toBe(1); // left half of the wide 'W'
    expect(offsetAtLineX(line, 45)).toBe(2); // right half of the wide 'W'
  });

  test('inside-character hit-testing uses glyph bounds from the table', () => {
    expect(offsetInsideLineX(line, 9)).toBe(0);
    expect(offsetInsideLineX(line, 25)).toBe(1);
    expect(offsetInsideLineX(line, 45)).toBe(1);
    expect(offsetInsideLineX(line, 55)).toBe(2);
  });

  test('a table of the wrong length is ignored rather than misread', () => {
    const stale = { ...line, charXs: [0, 10] };
    expect(xForOffset(stale, 2)).toBeCloseTo(40, 5);
  });
});

describe('lineAtY / offsetAtLocalPoint', () => {
  const metrics = wrapped();

  test('picks the line whose band contains the point', () => {
    expect(lineAtY(metrics, 5)?.start).toBe(0);
    expect(lineAtY(metrics, 25)?.start).toBe(6);
  });

  test('a point above the first line resolves to it', () => {
    expect(lineAtY(metrics, -100)?.start).toBe(0);
    expect(offsetAtLocalPoint(metrics, 0, -100)).toBe(0);
  });

  test('a point below the last line resolves to it, so a drag off the block still ends somewhere', () => {
    expect(lineAtY(metrics, 999)?.start).toBe(6);
    expect(offsetAtLocalPoint(metrics, 999, 999)).toBe(11);
  });

  test('an empty table resolves everything to 0', () => {
    const empty = buildTextMetrics([]);
    expect(lineAtY(empty, 10)).toBeNull();
    expect(offsetAtLocalPoint(empty, 10, 10)).toBe(0);
  });
});

describe('rectsForRange', () => {
  const metrics = wrapped();

  test('a range inside one line yields one rect', () => {
    expect(rectsForRange(metrics, 0, 3)).toEqual([{ x: 0, y: 0, w: 30, h: 20 }]);
  });

  test('a collapsed or inverted range yields nothing', () => {
    expect(rectsForRange(metrics, 4, 4)).toEqual([]);
  });

  test('a reversed range is normalized', () => {
    expect(rectsForRange(metrics, 3, 0)).toEqual([{ x: 0, y: 0, w: 30, h: 20 }]);
  });

  test('a range spanning a wrap stretches the first line to its full width', () => {
    // Without the stretch the highlight would stop at the last glyph of line
    // one and read as "the line break is not selected".
    expect(rectsForRange(metrics, 3, 8)).toEqual([
      { x: 30, y: 0, w: 30, h: 20 },
      { x: 0, y: 20, w: 20, h: 20 },
    ]);
  });

  test('lines outside the range contribute nothing', () => {
    expect(rectsForRange(metrics, 7, 9)).toEqual([{ x: 10, y: 20, w: 20, h: 20 }]);
  });

  test('a selection ending on an explicit newline still fills the line', () => {
    const explicit = buildTextMetrics([
      { text: 'ab\n', x: 0, y: 0, width: 20, height: 20 },
      { text: 'cd', x: 0, y: 20, width: 20, height: 20 },
    ]);
    // Offsets 0..3 cover 'ab' plus the newline: the rect runs to the line end.
    expect(rectsForRange(explicit, 0, 3)).toEqual([{ x: 0, y: 0, w: 20, h: 20 }]);
  });
});
