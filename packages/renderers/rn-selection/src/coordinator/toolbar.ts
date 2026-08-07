import type { LocalRect } from '../metrics';
import type { SelectionSerializeFormat } from '../serialize';

/**
 * The selection action bar — the thing that pops up once the user has selected
 * something.
 *
 * This used to be the platform's edit menu: pushing a range into the native
 * text view made UIKit present a `UIEditMenuInteraction` (and Android an
 * `ActionMode`), and our only influence was appending items to a list we did
 * not own. Item order, grouping, icons, styling, and above all the *lifecycle*
 * — when it appears and when it dismisses — belonged to the platform, and the
 * two platforms disagreed on all of it.
 *
 * Drawing it ourselves makes the bar an ordinary component: this module holds
 * the item model and the placement arithmetic, both pure, and
 * `SelectionToolbar.tsx` renders them. Hosts that want something else entirely
 * pass `renderToolbar` to `SelectionRoot` and use this placement as-is.
 */

export interface SelectionToolbarItem {
  /** Stable id, reported back on the action callback. */
  id: string;
  /** Label shown in the bar. Hosts localize; this package never does. */
  title: string;
  /**
   * Serialization format the action copies. Omit for actions that carry no
   * payload (the host handles them from the id alone).
   */
  format?: SelectionSerializeFormat;
}

/**
 * Default bar: the two actions the model can always satisfy. Deliberately
 * short — a default that guesses at product intent is worse than one the host
 * has to opt out of.
 */
export const DEFAULT_TOOLBAR_ITEMS: readonly SelectionToolbarItem[] = [
  { id: 'copy', title: 'Copy', format: 'plainText' },
  { id: 'copy-markdown', title: 'Copy as Markdown', format: 'markdown' },
];

export interface Size {
  w: number;
  h: number;
}

export type ToolbarSide = 'above' | 'below';

export interface ToolbarPlacement {
  /** Top-left of the bar, in `SelectionRoot` space. */
  x: number;
  y: number;
  side: ToolbarSide;
  /**
   * Where the bar's arrow should point, as an x offset from the bar's own left
   * edge. Kept inside the bar's rounded corners so the arrow never hangs off.
   */
  arrowX: number;
}

export interface ToolbarPlacementOptions {
  /** Gap between the selection and the bar. */
  gap?: number;
  /** Minimum distance from the viewport edges. */
  margin?: number;
  /** Half-width of the arrow, used to keep it off the bar's corners. */
  arrowInset?: number;
  /**
   * Extra root-space rectangles the bar should not cover, such as the two
   * handle touch targets. The selection rects remain the arrow anchor; these
   * only move the bar farther away on the chosen vertical side.
   */
  avoidRects?: readonly LocalRect[];
}

export const TOOLBAR_GAP = 8;
export const TOOLBAR_MARGIN = 8;
export const TOOLBAR_ARROW_INSET = 12;

function clamp(value: number, min: number, max: number): number {
  if (min > max) return min;
  if (value < min) return min;
  if (value > max) return max;
  return value;
}

function overlapArea(a: LocalRect, b: LocalRect): number {
  const xOverlap = Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x);
  const yOverlap = Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
  return xOverlap > 0 && yOverlap > 0 ? xOverlap * yOverlap : 0;
}

/** Bounding box of every rect, or null when there are none. */
export function unionRect(rects: readonly LocalRect[]): LocalRect | null {
  if (rects.length === 0) return null;
  let left = Infinity;
  let top = Infinity;
  let right = -Infinity;
  let bottom = -Infinity;
  for (const rect of rects) {
    if (rect.x < left) left = rect.x;
    if (rect.y < top) top = rect.y;
    if (rect.x + rect.w > right) right = rect.x + rect.w;
    if (rect.y + rect.h > bottom) bottom = rect.y + rect.h;
  }
  return { x: left, y: top, w: right - left, h: bottom - top };
}

/**
 * Place the bar against a selection.
 *
 * Above the selection is preferred — that is where both platforms put it, and
 * it keeps the bar clear of the hand holding the phone. It flips below when
 * there is no room above, and if neither side fits (a selection taller than the
 * viewport) it clamps into the viewport rather than disappearing off-screen.
 * Horizontally the bar centres on the selection and is clamped to the margins,
 * with the arrow tracking the selection's centre independently so it keeps
 * pointing at the text even once the bar itself has been pushed sideways.
 *
 * `viewport` is the visible area in the same coordinate space as `rects`.
 * Returns null when there is nothing to anchor to.
 */
export function computeToolbarPlacement(
  rects: readonly LocalRect[],
  size: Size,
  viewport: Size,
  options: ToolbarPlacementOptions = {}
): ToolbarPlacement | null {
  const anchor = unionRect(rects);
  if (anchor === null) return null;

  const gap = options.gap ?? TOOLBAR_GAP;
  const margin = options.margin ?? TOOLBAR_MARGIN;
  const arrowInset = options.arrowInset ?? TOOLBAR_ARROW_INSET;
  const avoidRects = options.avoidRects ?? [];
  const obstacleTop = avoidRects.reduce((top, rect) => Math.min(top, rect.y), anchor.y);
  const obstacleBottom = avoidRects.reduce(
    (bottom, rect) => Math.max(bottom, rect.y + rect.h),
    anchor.y + anchor.h
  );
  const centre = anchor.x + anchor.w / 2;
  const x = clamp(centre - size.w / 2, margin, Math.max(margin, viewport.w - margin - size.w));

  const above = obstacleTop - gap - size.h;
  const below = obstacleBottom + gap;
  let side: ToolbarSide;
  let y: number;
  if (above >= margin) {
    side = 'above';
    y = above;
  } else if (below + size.h <= viewport.h - margin) {
    side = 'below';
    y = below;
  } else {
    // Neither side fits: clamp inside, then choose the side that covers the
    // least handle touch target area. Ties keep the platform-like above bias.
    const minY = margin;
    const maxY = Math.max(margin, viewport.h - margin - size.h);
    const aboveY = clamp(above, minY, maxY);
    const belowY = clamp(below, minY, maxY);
    const overlapScore = (candidateY: number) =>
      avoidRects.reduce(
        (score, rect) => score + overlapArea({ x, y: candidateY, w: size.w, h: size.h }, rect),
        0
      );
    if (overlapScore(belowY) < overlapScore(aboveY)) {
      side = 'below';
      y = belowY;
    } else {
      side = 'above';
      y = aboveY;
    }
  }

  const arrowX = clamp(centre - x, arrowInset, Math.max(arrowInset, size.w - arrowInset));

  return { x, y, side, arrowX };
}
