import type { SelectionPoint } from '../model';
import { offsetAtLocalPoint } from '../metrics';
import type { SelectionUnitIndex } from '../resolve';
import { buildSegmentSpans, segmentOffsetToPoint } from '../native/segmentAdapter';
import { visibleBlockRect, type LayoutRect, type RegisteredBlock } from './registry';

interface VisibleBlock {
  block: RegisteredBlock;
  rect: LayoutRect;
}

function visibleBlocks(blocks: readonly RegisteredBlock[]): VisibleBlock[] {
  const visible: VisibleBlock[] = [];
  for (const block of blocks) {
    const rect = visibleBlockRect(block);
    if (rect !== null) visible.push({ block, rect });
  }
  return visible;
}

/** A point in the `SelectionRoot`'s coordinate space. */
export interface Point {
  x: number;
  y: number;
}

/** True when `p` lies inside (inclusive) the rect. */
export function pointInRect(p: Point, r: LayoutRect): boolean {
  return p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h;
}

/** Vertical distance from `p` to the rect's y-band; 0 when inside the band. */
export function verticalGap(p: Point, r: LayoutRect): number {
  if (p.y >= r.y && p.y <= r.y + r.h) return 0;
  if (p.y < r.y) return r.y - p.y;
  return p.y - (r.y + r.h);
}

/** Horizontal distance from `p.x` to the rect's x-band; 0 when inside. */
function horizontalGap(p: Point, r: LayoutRect): number {
  if (p.x >= r.x && p.x <= r.x + r.w) return 0;
  if (p.x < r.x) return r.x - p.x;
  return p.x - (r.x + r.w);
}

/**
 * Pick the block a point belongs to. `blocks` must already be in document order
 * so ties resolve to the earlier (upper) block. Returns null only when no block
 * has a layout rect yet.
 */
export function chooseBlock(blocks: readonly RegisteredBlock[], p: Point): RegisteredBlock | null {
  const laid = visibleBlocks(blocks);
  if (laid.length === 0) return null;

  // (1) A block that directly contains the point wins (earliest in doc order).
  for (const entry of laid) if (pointInRect(p, entry.rect)) return entry.block;

  // (2) Blocks whose y-band contains the point: choose nearest by x, ties -> earliest.
  const band = laid.filter(entry => verticalGap(p, entry.rect) === 0);
  if (band.length > 0) {
    let best = band[0];
    let bestDist = horizontalGap(p, best.rect);
    for (let i = 1; i < band.length; i++) {
      const dist = horizontalGap(p, band[i].rect);
      if (dist < bestDist) {
        best = band[i];
        bestDist = dist;
      }
    }
    return best.block;
  }

  // (3) Otherwise the block with the smallest vertical gap, ties -> earlier.
  let best = laid[0];
  let bestGap = verticalGap(p, best.rect);
  for (let i = 1; i < laid.length; i++) {
    const gap = verticalGap(p, laid[i].rect);
    if (gap < bestGap) {
      best = laid[i];
      bestGap = gap;
    }
  }
  return best.block;
}

/**
 * Pick only a block whose measured rect directly contains the point. Long-press
 * selection uses this stricter hit-test so pressing unregistered UI, fixture
 * text, or whitespace does not select some unrelated nearest block.
 */
export function containingBlock(
  blocks: readonly RegisteredBlock[],
  p: Point
): RegisteredBlock | null {
  for (const block of blocks) {
    const rect = visibleBlockRect(block);
    if (rect !== null && pointInRect(p, rect)) return block;
  }
  return null;
}

/**
 * The point just before a block's first unit, or null when the block renders no
 * units at all (a registration whose `updateUnits` has not landed yet). Callers
 * return null rather than emitting `{unitId: undefined}`, which
 * `locateSelectionPoint` would clamp to the DOCUMENT start — a wildly wrong
 * answer dressed up as a valid one.
 */
function blockStartPoint(block: RegisteredBlock): SelectionPoint | null {
  const first = block.unitIds[0];
  if (first === undefined) return null;
  return { nodeId: block.nodeId, unitId: first, offset: 0 };
}

/**
 * The point just after a block's last unit, or null when it renders no units.
 *
 * `offset` is unit-relative and its meaning depends on the unit's kind:
 * `locateSelectionPoint` clamps a text unit's offset into `[0, text.length]`
 * but treats any positive offset on a zero-text unit (atom / boundary) as
 * "after". A hardcoded `offset: 1` therefore means "after" for an atom but
 * "one UTF-16 unit in" for text — so dragging past the bottom of a block
 * ending in `'Hello world'` used to select ONE character instead of eleven.
 */
function blockEndPoint(block: RegisteredBlock, index: SelectionUnitIndex): SelectionPoint | null {
  const last = block.unitIds[block.unitIds.length - 1];
  if (last === undefined) return null;
  const entryIndex = index.byUnitId.get(last);
  const textLength = entryIndex === undefined ? 0 : index.entries[entryIndex].textLength;
  return { nodeId: block.nodeId, unitId: last, offset: textLength > 0 ? textLength : 1 };
}

/**
 * Localize a point inside a chosen block to a `SelectionPoint`.
 *
 * With a line table (`block.metrics`) this is a real character hit-test: the
 * root-space point is moved into the block's text content box and resolved
 * through `offsetAtLocalPoint`, then mapped back onto the document unit stream.
 * A block that has not been measured yet — or one that is not text at all —
 * degrades to a coarse before/after by the point's position relative to the
 * rect's mid-lines, which is what the whole layer did before metrics existed.
 * Null when the block renders no units.
 */
export function localizePoint(
  block: RegisteredBlock,
  p: Point,
  index: SelectionUnitIndex
): SelectionPoint | null {
  const rect = block.rect as LayoutRect;
  if (block.metrics && block.metrics.lines.length > 0) {
    const origin = block.contentOffset ?? { x: 0, y: 0 };
    const offset = offsetAtLocalPoint(
      block.metrics,
      p.x - rect.x - origin.x,
      p.y - rect.y - origin.y
    );
    return segmentOffsetToPoint(buildSegmentSpans(block, index), offset);
  }
  const after =
    p.y > rect.y + rect.h / 2 || (verticalGap(p, rect) === 0 && p.x > rect.x + rect.w / 2);
  return after ? blockEndPoint(block, index) : blockStartPoint(block);
}

/**
 * Resolve a root-coordinate point to a document `SelectionPoint`, or null when
 * no block is laid out. Points before the first / after the last block collapse
 * to the document start / end; everything else localizes within its block.
 */
export function resolvePointToSelection(
  blocks: readonly RegisteredBlock[],
  p: Point,
  index: SelectionUnitIndex
): SelectionPoint | null {
  const block = chooseBlock(blocks, p);
  if (!block) return null;
  const rect = visibleBlockRect(block) as LayoutRect;
  const laid = visibleBlocks(blocks);

  // Before the first laid-out block -> document start.
  if (block === laid[0].block && p.y < rect.y) {
    return blockStartPoint(block);
  }
  // After the last laid-out block -> document end.
  if (block === laid[laid.length - 1].block && p.y > rect.y + rect.h) {
    return blockEndPoint(block, index);
  }
  return localizePoint(block, p, index);
}
