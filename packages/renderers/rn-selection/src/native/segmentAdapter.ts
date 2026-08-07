import type { SelectionNodeId, SelectionPoint, SelectionRange } from '../model';
import { locateSelectionPoint, type SelectionUnitIndex } from '../resolve';
import { snapToGraphemeBoundary } from '../text';
import type { RegisteredBlock } from '../coordinator/registry';

/**
 * Pure mapping between a rendered text segment's local UTF-16 offsets and
 * Supramark's document-wide `SelectionPoint`s.
 *
 * "Segment" no longer means a native text view with its own selection state —
 * the native command bridge is gone (see `SELECTION_PLAN.md`). It means one
 * block's laid-out text: the string the block rendered, whose offsets index
 * both its line metrics (`metrics.ts`) and the units it registered. This
 * mapping is what lets a finger position become a document range and a document
 * range become a highlight.
 *
 * Every function here is pure and free of React Native imports, so the module
 * loads under `bun test` unchanged.
 */

/** Segment-local UTF-16 `[start, end)` span contributed by one unit. */
export interface SegmentSpan {
  unitId: SelectionNodeId;
  nodeId: SelectionNodeId;
  start: number;
  end: number;
}

function clamp(value: number, min: number, max: number): number {
  if (value < min) return min;
  if (value > max) return max;
  return value;
}

/**
 * Build the segment-local span table for a block's units, in registration
 * order. Only units with `textLength > 0` (text + inline break) participate
 * in the native view's visible plain text; empty-text syntax units are
 * skipped. Unknown unit ids (not present in `index`) are skipped too.
 */
export function buildSegmentSpans(
  block: Pick<RegisteredBlock, 'unitIds'>,
  index: SelectionUnitIndex
): SegmentSpan[] {
  const spans: SegmentSpan[] = [];
  let start = 0;
  for (const unitId of block.unitIds) {
    const unitIndex = index.byUnitId.get(unitId);
    if (unitIndex === undefined) continue;
    const entry = index.entries[unitIndex];
    if (entry.textLength <= 0) continue;
    const end = start + entry.textLength;
    spans.push({ unitId, nodeId: entry.unit.nodeId, start, end });
    start = end;
  }
  return spans;
}

/**
 * Map a segment-local UTF-16 offset to a `SelectionPoint`. `localOffset` is
 * clamped into `[0, total]`. An offset sitting exactly on a shared boundary
 * between two spans resolves to the later span's start (offset 0), except
 * the final end, which resolves to the last span's end — the exact inverse
 * of `pointToSegmentOffset`.
 */
export function segmentOffsetToPoint(
  spans: readonly SegmentSpan[],
  localOffset: number
): SelectionPoint {
  if (spans.length === 0) return { nodeId: '', offset: 0 };
  const last = spans[spans.length - 1];
  const offset = clamp(localOffset, 0, last.end);
  for (const span of spans) {
    if (offset < span.end) {
      return { nodeId: span.nodeId, unitId: span.unitId, offset: offset - span.start };
    }
  }
  return { nodeId: last.nodeId, unitId: last.unitId, offset: last.end - last.start };
}

/**
 * Inverse of `segmentOffsetToPoint`: map a `SelectionPoint` back to a
 * segment-local UTF-16 offset. A direct `unitId` match wins; otherwise the
 * first span sharing `nodeId` is used. Returns `0` when nothing matches.
 */
export function pointToSegmentOffset(spans: readonly SegmentSpan[], point: SelectionPoint): number {
  let span = point.unitId !== undefined ? spans.find(s => s.unitId === point.unitId) : undefined;
  if (!span) span = spans.find(s => s.nodeId === point.nodeId);
  if (!span) return 0;
  return span.start + clamp(point.offset, 0, span.end - span.start);
}

/** Turn a segment-local `[start, end)` pair into a document range. */
export function segmentSelectionToRange(
  spans: readonly SegmentSpan[],
  startUtf16: number,
  endUtf16: number
): SelectionRange {
  return {
    anchor: segmentOffsetToPoint(spans, startUtf16),
    focus: segmentOffsetToPoint(spans, endUtf16),
  };
}

/**
 * Map a document `SelectionRange` into this segment's local `[start, end)`
 * UTF-16 pair, ordered ascending, ready to hand to `rectsForRange` so the
 * highlight can be drawn over exactly this block's share of the selection.
 *
 * Endpoints are resolved with `locateSelectionPoint` on the full document
 * index — the same resolution `resolveSelectionRange` uses — and then
 * projected onto the segment's spans: a point before the segment clamps to
 * `0`, a point after it (e.g. on the block's trailing break unit, which the
 * document stream owns but the segment does not render) clamps to the segment
 * end, and a zero-text syntax unit inside the block lands on the next span's
 * start. The naive per-span `pointToSegmentOffset` fallback cannot do this:
 * without the document index it maps an out-of-segment point that shares the
 * block's `nodeId` into the FIRST span, collapsing an end-of-block selection
 * to the block's head.
 *
 * Returns `null` when the segment renders no text or the projected range
 * collapses.
 */
export function rangeToSegmentSelection(
  range: SelectionRange,
  index: SelectionUnitIndex,
  spans: readonly SegmentSpan[]
): { startUtf16: number; endUtf16: number } | null {
  if (spans.length === 0) return null;

  const spanByUnitIndex = new Map<number, SegmentSpan>();
  let firstUnitIndex = Number.MAX_SAFE_INTEGER;
  let lastUnitIndex = -1;
  for (const span of spans) {
    const unitIndex = index.byUnitId.get(span.unitId);
    if (unitIndex === undefined) continue;
    spanByUnitIndex.set(unitIndex, span);
    if (unitIndex < firstUnitIndex) firstUnitIndex = unitIndex;
    if (unitIndex > lastUnitIndex) lastUnitIndex = unitIndex;
  }
  if (lastUnitIndex < 0) return null;
  const segmentEnd = spans[spans.length - 1].end;

  const project = (located: { unitIndex: number; intraOffset: number }): number => {
    const span = spanByUnitIndex.get(located.unitIndex);
    if (span) return span.start + clamp(located.intraOffset, 0, span.end - span.start);
    if (located.unitIndex < firstUnitIndex) return 0;
    if (located.unitIndex > lastUnitIndex) return segmentEnd;
    // A zero-text unit interleaved between this segment's spans: it occupies no
    // stream text, so both its before and after positions coincide with the
    // next rendered span's start.
    for (const [unitIndex, candidate] of spanByUnitIndex) {
      if (unitIndex > located.unitIndex) return candidate.start;
    }
    return segmentEnd;
  };

  const a = project(locateSelectionPoint(index, range.anchor));
  const f = project(locateSelectionPoint(index, range.focus));
  // Snap to the same grapheme-cluster boundaries the serializer uses. Without
  // this the highlight and the clipboard disagree inside a cluster: `project`
  // is built on `locateSelectionPoint`, which does not snap, while
  // `resolveSelectionRange` -> `splitTextUnit` widens outward — so the rect
  // would stop mid-emoji while the copied text contained the whole thing.
  // Widening (start backward, end forward) matches the serializer's direction
  // exactly, so both consumers land on one range.
  const segmentText = segmentTextFromSpans(index, spans);
  const startUtf16 = snapToGraphemeBoundary(segmentText, Math.min(a, f), 'backward');
  const endUtf16 = snapToGraphemeBoundary(segmentText, Math.max(a, f), 'forward');
  return startUtf16 === endUtf16 ? null : { startUtf16, endUtf16 };
}

/**
 * Reassemble the segment's visible plain text from its span table, so
 * `rangeToSegmentSelection` can snap against the same string the block laid
 * out. Spans are contiguous and ordered by construction
 * (`buildSegmentSpans`), and every span's unit is a text/break unit — those
 * are the only kinds with a non-zero `textLength`.
 */
export function segmentTextFromSpans(index: SelectionUnitIndex, spans: readonly SegmentSpan[]): string {
  let text = '';
  for (const span of spans) {
    const unitIndex = index.byUnitId.get(span.unitId);
    if (unitIndex === undefined) continue;
    const unit = index.entries[unitIndex].unit;
    if (unit.kind === 'text' || unit.kind === 'break') text += unit.text;
  }
  return text;
}
