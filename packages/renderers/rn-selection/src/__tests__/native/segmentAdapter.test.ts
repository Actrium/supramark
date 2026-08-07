import { describe, expect, test } from 'bun:test';
import type { SupramarkTextNode } from '@supramark/core';
import type { SelectionTextUnit, SelectionUnit } from '../../model';
import { buildUnitIndex, resolveSelectionRange } from '../../resolve';
import { serializeSelectionUnits } from '../../serialize';
import {
  buildSegmentSpans,
  pointToSegmentOffset,
  rangeToSegmentSelection,
  segmentOffsetToPoint,
  segmentSelectionToRange,
  segmentTextFromSpans,
} from '../../native/segmentAdapter';

// A throwaway AST node — segmentAdapter copies `node` but never inspects it.
const NODE = { type: 'text', value: '' } as SupramarkTextNode;

const tUnit = (unitId: string, nodeId: string, text: string): SelectionTextUnit => ({
  kind: 'text',
  unitId,
  nodeId,
  text,
  node: NODE,
});

describe('buildSegmentSpans', () => {
  test('skips empty-text units and accumulates offsets', () => {
    const units: SelectionUnit[] = [
      tUnit('h#0', 'h', ''),
      tUnit('h#1', 'h', 'Hello'),
      tUnit('h#2', 'h', 'World'),
    ];
    const index = buildUnitIndex(units);
    const block = { unitIds: ['h#0', 'h#1', 'h#2'] };
    const spans = buildSegmentSpans(block, index);

    expect(spans).toEqual([
      { unitId: 'h#1', nodeId: 'h', start: 0, end: 5 },
      { unitId: 'h#2', nodeId: 'h', start: 5, end: 10 },
    ]);
  });

  test('skips unit ids missing from the index', () => {
    const units: SelectionUnit[] = [tUnit('h#1', 'h', 'Hello')];
    const index = buildUnitIndex(units);
    const spans = buildSegmentSpans({ unitIds: ['missing#0', 'h#1'] }, index);
    expect(spans).toEqual([{ unitId: 'h#1', nodeId: 'h', start: 0, end: 5 }]);
  });
});

describe('segmentOffsetToPoint / pointToSegmentOffset', () => {
  const units: SelectionUnit[] = [
    tUnit('h#0', 'h', ''),
    tUnit('h#1', 'h', 'Hello'),
    tUnit('h#2', 'h', 'World'),
  ];
  const index = buildUnitIndex(units);
  const spans = buildSegmentSpans({ unitIds: ['h#0', 'h#1', 'h#2'] }, index);

  test('maps interior offsets to the containing span', () => {
    expect(segmentOffsetToPoint(spans, 3)).toEqual({ nodeId: 'h', unitId: 'h#1', offset: 3 });
    expect(segmentOffsetToPoint(spans, 7)).toEqual({ nodeId: 'h', unitId: 'h#2', offset: 2 });
  });

  test('a shared boundary offset resolves to the later span start', () => {
    expect(segmentOffsetToPoint(spans, 5)).toEqual({ nodeId: 'h', unitId: 'h#2', offset: 0 });
  });

  test('the final end resolves to the last span end', () => {
    expect(segmentOffsetToPoint(spans, 10)).toEqual({ nodeId: 'h', unitId: 'h#2', offset: 5 });
  });

  test('offsets clamp into range', () => {
    expect(segmentOffsetToPoint(spans, 99)).toEqual({ nodeId: 'h', unitId: 'h#2', offset: 5 });
    expect(segmentOffsetToPoint(spans, -1)).toEqual({ nodeId: 'h', unitId: 'h#1', offset: 0 });
  });

  test('pointToSegmentOffset is the exact inverse over the whole range', () => {
    const total = spans[spans.length - 1].end;
    for (let o = 0; o <= total; o++) {
      const point = segmentOffsetToPoint(spans, o);
      expect(pointToSegmentOffset(spans, point)).toBe(o);
    }
  });

  test('pointToSegmentOffset falls back to matching by nodeId when unitId is absent', () => {
    expect(pointToSegmentOffset(spans, { nodeId: 'h', offset: 2 })).toBe(2);
  });

  test('pointToSegmentOffset returns 0 when nothing matches', () => {
    expect(pointToSegmentOffset(spans, { nodeId: 'other', offset: 2 })).toBe(0);
  });
});

describe('segment range translation', () => {
  const units: SelectionUnit[] = [tUnit('h#1', 'h', 'Hello'), tUnit('h#2', 'h', 'World')];
  const index = buildUnitIndex(units);
  const spans = buildSegmentSpans({ unitIds: ['h#1', 'h#2'] }, index);

  test('segmentSelectionToRange builds anchor/focus from segment-local offsets', () => {
    // This is the path a long press takes now: the gesture layer resolves a
    // word to a local [start, end) and hands it straight to the document model.
    const range = segmentSelectionToRange(spans, 2, 7);
    expect(range.anchor).toEqual({ nodeId: 'h', unitId: 'h#1', offset: 2 });
    expect(range.focus).toEqual({ nodeId: 'h', unitId: 'h#2', offset: 2 });
  });

  test('segmentSelectionToRange round-trips through rangeToSegmentSelection', () => {
    const range = segmentSelectionToRange(spans, 2, 7);
    expect(rangeToSegmentSelection(range, index, spans)).toEqual({ startUtf16: 2, endUtf16: 7 });
  });

  test('segmentTextFromSpans reassembles what the block laid out', () => {
    // The word-boundary scan runs against this string, so it has to be exactly
    // the text whose offsets the spans describe.
    expect(segmentTextFromSpans(index, spans)).toBe('HelloWorld');
  });

  test('rangeToSegmentSelection orders the result ascending regardless of direction', () => {
    const forward = rangeToSegmentSelection(
      {
        anchor: { nodeId: 'h', unitId: 'h#1', offset: 2 },
        focus: { nodeId: 'h', unitId: 'h#2', offset: 2 },
      },
      index,
      spans
    );
    const reversed = rangeToSegmentSelection(
      {
        anchor: { nodeId: 'h', unitId: 'h#2', offset: 2 },
        focus: { nodeId: 'h', unitId: 'h#1', offset: 2 },
      },
      index,
      spans
    );
    expect(forward).toEqual({ startUtf16: 2, endUtf16: 7 });
    expect(reversed).toEqual({ startUtf16: 2, endUtf16: 7 });
  });
});

describe('rangeToSegmentSelection document-index projection', () => {
  const brk = (unitId: string, nodeId: string): SelectionUnit => ({
    kind: 'break',
    unitId,
    nodeId,
    text: '\n',
    reason: 'block',
    node: NODE,
  });
  // Three-paragraph stream; the segment under test renders p1's visible units.
  const units: SelectionUnit[] = [
    tUnit('p0#0', 'p0', 'Prev'),
    brk('p0#1', 'p0'),
    tUnit('p1#0', 'p1', 'Hello '),
    tUnit('p1#1', 'p1', 'world'),
    brk('p1#2', 'p1'),
    tUnit('p2#0', 'p2', 'Next'),
  ];
  const index = buildUnitIndex(units);
  const spans = buildSegmentSpans({ unitIds: ['p1#0', 'p1#1'] }, index);

  test('a focus on the trailing break clamps to the segment end, not the head', () => {
    // The break shares nodeId 'p1' but is not rendered by the segment; the old
    // per-span nodeId fallback collapsed this to the FIRST span.
    const seg = rangeToSegmentSelection(
      {
        anchor: { nodeId: 'p1', unitId: 'p1#0', offset: 0 },
        focus: { nodeId: 'p1', unitId: 'p1#2', offset: 1 },
      },
      index,
      spans
    );
    expect(seg).toEqual({ startUtf16: 0, endUtf16: 11 });
  });

  test('an anchor before the segment clamps to 0', () => {
    const seg = rangeToSegmentSelection(
      {
        anchor: { nodeId: 'p0', unitId: 'p0#0', offset: 2 },
        focus: { nodeId: 'p1', unitId: 'p1#1', offset: 3 },
      },
      index,
      spans
    );
    expect(seg).toEqual({ startUtf16: 0, endUtf16: 9 });
  });

  test('a zero-text unit interleaved between spans projects to the next span start', () => {
    const mixed: SelectionUnit[] = [
      tUnit('m#0', 'm', 'ab'),
      tUnit('m#1', 'm', ''),
      tUnit('m#2', 'm', 'cd'),
    ];
    const mIndex = buildUnitIndex(mixed);
    const mSpans = buildSegmentSpans({ unitIds: ['m#0', 'm#1', 'm#2'] }, mIndex);
    const seg = rangeToSegmentSelection(
      {
        anchor: { nodeId: 'm', unitId: 'm#1', offset: 1 },
        focus: { nodeId: 'm', unitId: 'm#2', offset: 2 },
      },
      mIndex,
      mSpans
    );
    expect(seg).toEqual({ startUtf16: 2, endUtf16: 4 });
  });

  test('a collapsed projection returns null', () => {
    const seg = rangeToSegmentSelection(
      {
        anchor: { nodeId: 'p1', unitId: 'p1#0', offset: 2 },
        focus: { nodeId: 'p1', unitId: 'p1#0', offset: 2 },
      },
      index,
      spans
    );
    expect(seg).toBeNull();
  });

  test('a segment with no spans returns null', () => {
    const seg = rangeToSegmentSelection(
      {
        anchor: { nodeId: 'p1', unitId: 'p1#0', offset: 0 },
        focus: { nodeId: 'p1', unitId: 'p1#1', offset: 5 },
      },
      index,
      []
    );
    expect(seg).toBeNull();
  });
});

// The resolve path and the highlight path must agree on where a selection may
// be cut. `resolveSelectionRange` -> `splitTextUnit` grapheme-snaps outward,
// while the projection is built from `locateSelectionPoint`, which does not.
// Before the snap was added to `rangeToSegmentSelection`, a focus landing
// inside a surrogate pair produced a highlight one code unit shorter than the
// text the clipboard received: the user saw one thing highlighted and pasted
// another.
describe('rangeToSegmentSelection grapheme agreement with the serializer', () => {
  const STAR = '\u{1F31F}'; // astral, 2 UTF-16 units

  const units: SelectionUnit[] = [tUnit('p1#0', 'p1', 'Hi '), tUnit('p1#1', 'p1', `${STAR}b`)];
  const index = buildUnitIndex(units);
  const spans = buildSegmentSpans({ unitIds: ['p1#0', 'p1#1'] }, index);
  const segmentText = `Hi ${STAR}b`;

  test('a focus inside a surrogate pair widens to the same range the copy uses', () => {
    const range = {
      anchor: { nodeId: 'p1', unitId: 'p1#0', offset: 0 },
      // Offset 1 inside 'STAR + b' is the seam between the high and low
      // surrogate of STAR.
      focus: { nodeId: 'p1', unitId: 'p1#1', offset: 1 },
    };

    const seg = rangeToSegmentSelection(range, index, spans);
    expect(seg).not.toBeNull();
    const nativeSlice = segmentText.slice(seg!.startUtf16, seg!.endUtf16);
    const copied = serializeSelectionUnits(resolveSelectionRange(units, range), 'plainText');

    expect(nativeSlice).toBe(copied as string);
    expect(nativeSlice).toBe(`Hi ${STAR}`);
    // And nothing was cut mid-pair: a lone surrogate would fail this.
    expect([...nativeSlice].length).toBe(4);
  });

  test('every offset in the segment keeps native and clipboard in step', () => {
    const anchor = { nodeId: 'p1', unitId: 'p1#0', offset: 0 };
    const focusUnitLength = `${STAR}b`.length;
    for (let offset = 1; offset <= focusUnitLength; offset += 1) {
      const range = { anchor, focus: { nodeId: 'p1', unitId: 'p1#1', offset } };
      const seg = rangeToSegmentSelection(range, index, spans);
      if (seg === null) continue;
      const copied = serializeSelectionUnits(
        resolveSelectionRange(units, range),
        'plainText'
      ) as string;
      expect(segmentText.slice(seg.startUtf16, seg.endUtf16)).toBe(copied);
    }
  });
});
