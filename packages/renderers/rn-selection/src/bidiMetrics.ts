import type { LocalRect } from './metrics';

export interface VisualCluster {
  start: number;
  end: number;
  left: number;
  right: number;
  rtl: boolean;
}

/** Offsets stay logical. Hit-testing walks grapheme boxes in visual order. */
export function visualClusters(
  offsets: readonly number[],
  starts: readonly number[],
  ends?: readonly number[]
): VisualCluster[] {
  const clusters: VisualCluster[] = [];
  for (let i = 1; i < offsets.length; i++) {
    const start = offsets[i - 1];
    const end = offsets[i];
    const a = starts[start];
    const b = ends ? ends[end - 1] : starts[end];
    if (a !== b)
      clusters.push({ start, end, left: Math.min(a, b), right: Math.max(a, b), rtl: b < a });
  }
  return clusters.sort((a, b) => a.left - b.left);
}

export function hitVisualCluster(
  clusters: readonly VisualCluster[],
  x: number,
  caret: boolean
): number {
  const box = clusters.find(cluster => x < cluster.right) ?? clusters[clusters.length - 1];
  if (!caret) return box.start;
  const midpoint = (box.left + box.right) / 2;
  return (box.rtl ? x >= midpoint : x <= midpoint) ? box.start : box.end;
}

export function visualRangeRects(
  clusters: readonly VisualCluster[],
  start: number,
  end: number,
  y: number,
  h: number,
  continuation?: { x: number; rtl: boolean }
): LocalRect[] {
  const rects: LocalRect[] = [];
  function append(left: number, right: number) {
    if (right <= left) return;
    const previous = rects[rects.length - 1];
    if (previous && left <= previous.x + previous.w + 0.001) {
      previous.w = Math.max(previous.w, right - previous.x);
    } else {
      rects.push({ x: left, y, w: right - left, h });
    }
  }
  if (continuation?.rtl) append(continuation.x, clusters[0].left);
  for (const box of clusters) {
    if (box.end > start && box.start < end) append(box.left, box.right);
  }
  if (continuation && !continuation.rtl)
    append(clusters[clusters.length - 1].right, continuation.x);
  return rects;
}
