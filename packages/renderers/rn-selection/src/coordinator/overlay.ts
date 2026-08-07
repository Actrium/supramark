import type { LocalRect } from '../metrics';
import { rectsForRange } from '../metrics';
import type { SelectionRange, SelectionUnit } from '../model';
import { buildSegmentSpans, rangeToSegmentSelection } from '../native/segmentAdapter';
import type { SelectionUnitIndex } from '../resolve';
import { HANDLE_KNOB_RADIUS, HANDLE_TOUCH_RADIUS } from './handles';
import { intersectLayoutRects, type LayoutRect, type RegisteredBlock } from './registry';

export interface OverlayRect extends LayoutRect {
  /** True only when the real range start and its touch target fit in the clip. */
  startHandleVisible?: boolean;
  /** True only when the real range end and its touch target fit in the clip. */
  endHandleVisible?: boolean;
}

interface RectCandidate {
  raw: LayoutRect;
  visible: LayoutRect | null;
  block: RegisteredBlock;
}

/**
 * Selection highlight geometry.
 *
 * This module used to answer a much weaker question — "which whole blocks are
 * covered?" — because the platform drew the real highlight for single-block
 * selections and the coordinator only had to fill in the cross-block case with
 * flat block rectangles. It also had to *yield*: a `yieldNodeId` parameter told
 * it to skip the block the native bridge had taken over, so the two layers did
 * not paint on top of each other.
 *
 * Neither applies now. There is one highlight, we draw all of it, and it is
 * drawn per line of text from each block's own metrics.
 */

export interface SelectionRectsInput {
  /** Registered blocks, in document order. */
  blocks: readonly RegisteredBlock[];
  /** The live range; null when nothing is selected. */
  range: SelectionRange | null;
  /** Covered units from the store snapshot — what decides block coverage. */
  units: readonly SelectionUnit[];
  /** Unit index the range is resolved against. */
  index: SelectionUnitIndex;
}

/** Move a segment-local rect into `SelectionRoot` space. */
function translate(rect: LocalRect, block: RegisteredBlock): OverlayRect {
  const base = block.rect as LayoutRect;
  const origin = block.contentOffset ?? { x: 0, y: 0 };
  return {
    x: base.x + origin.x + rect.x,
    y: base.y + origin.y + rect.y,
    w: rect.w,
    h: rect.h,
  };
}

/** Clip a root-space highlight to its nested scroll viewport, when present. */
function visibleRect(rect: OverlayRect, block: RegisteredBlock): OverlayRect | null {
  if (block.clipRect === undefined) return rect;
  return intersectLayoutRects(rect, block.clipRect);
}

function handleFitsClip(rect: LayoutRect, block: RegisteredBlock, edge: 'start' | 'end'): boolean {
  const clip = block.clipRect;
  if (clip === undefined) return true;
  const x = edge === 'start' ? rect.x : rect.x + rect.w;
  const knobY =
    edge === 'start' ? rect.y - HANDLE_KNOB_RADIUS : rect.y + rect.h + HANDLE_KNOB_RADIUS;
  return (
    x - HANDLE_TOUCH_RADIUS >= clip.x &&
    x + HANDLE_TOUCH_RADIUS <= clip.x + clip.w &&
    knobY - HANDLE_TOUCH_RADIUS >= clip.y &&
    knobY + HANDLE_TOUCH_RADIUS <= clip.y + clip.h
  );
}

/**
 * Highlight rectangles for the current selection, in document order.
 *
 * Per covered block: project the document range onto the block's segment spans
 * (`rangeToSegmentSelection`, which already handles clamping a point that sits
 * before, after or between this block's units) and turn the resulting local
 * range into one rectangle per line of text.
 *
 * A block with no line table yet falls back to its whole layout rect, which is
 * exactly the old behaviour — so a block still waiting on its first
 * `onTextLayout`, or one that is an atom rather than text, highlights coarsely
 * instead of not at all.
 *
 * Unlike the old block-level version this does **not** merge vertically
 * adjacent rects: per-line rects from one block already tile, and merging
 * across blocks would square off the ragged first and last lines that make a
 * text selection readable.
 */
export function computeSelectionRects(input: SelectionRectsInput): OverlayRect[] {
  const { blocks, range, units, index } = input;
  const covered = new Set(units.map(u => u.unitId));
  const candidates: RectCandidate[] = [];

  const addCandidate = (raw: LayoutRect, block: RegisteredBlock) => {
    candidates.push({ raw, visible: visibleRect(raw, block), block });
  };

  for (const block of blocks) {
    if (!block.rect) continue;
    if (!block.unitIds.some(id => covered.has(id))) continue;

    const metrics = block.metrics;
    if (range !== null && metrics && metrics.lines.length > 0) {
      const spans = buildSegmentSpans(block, index);
      const segment = rangeToSegmentSelection(range, index, spans);
      if (segment !== null) {
        const lineRects = rectsForRange(metrics, segment.startUtf16, segment.endUtf16);
        if (lineRects.length > 0) {
          for (const rect of lineRects) {
            addCandidate(translate(rect, block), block);
          }
          continue;
        }
      }
    }
    // Copy so a consumer can never mutate a block's live registry rect.
    addCandidate({ ...block.rect }, block);
  }

  const visibleCandidates = candidates.filter(
    (candidate): candidate is RectCandidate & { visible: LayoutRect } => candidate.visible !== null
  );
  const rects: OverlayRect[] = visibleCandidates.map(candidate => ({ ...candidate.visible }));
  if (rects.length === 0) return rects;

  const firstVisible = visibleCandidates[0];
  const firstCandidate = candidates[0];
  if (
    firstVisible !== firstCandidate ||
    !handleFitsClip(firstVisible.raw, firstVisible.block, 'start')
  ) {
    rects[0].startHandleVisible = false;
  }

  const lastVisible = visibleCandidates[visibleCandidates.length - 1];
  const lastCandidate = candidates[candidates.length - 1];
  if (lastVisible !== lastCandidate || !handleFitsClip(lastVisible.raw, lastVisible.block, 'end')) {
    rects[rects.length - 1].endHandleVisible = false;
  }
  return rects;
}
