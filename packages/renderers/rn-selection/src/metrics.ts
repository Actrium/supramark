/**
 * Text metrics contract — the only thing the selection layer needs from a
 * rendered text block.
 *
 * Once the selection UI is drawn by us rather than by the platform (see
 * `SELECTION_PLAN.md`, "Interaction direction"), everything the UI has to
 * answer reduces to two questions about one laid-out text segment:
 *
 * - where in the text is this finger? (`offsetAtLocalPoint`)
 * - where on screen is this range?    (`rectsForRange`)
 *
 * Both are pure functions of a line table, so they are identical on iOS,
 * Android and — later — web. What differs per platform is only who fills the
 * table in:
 *
 * - React Native `<Text onTextLayout>` gives one entry per laid-out line
 *   (`{x, y, width, height, text}`) as public API on both platforms, Paper and
 *   Fabric alike. This is the default provider and needs no native code.
 * - A native provider can additionally fill `charXs` (per-character leading
 *   edges) from `NSLayoutManager` / `android.text.Layout`, which removes the
 *   within-line approximation documented below.
 * - On web, `Range.getClientRects()` fills the same shape.
 *
 * All coordinates are **segment-local**: measured from the origin of the text
 * content box the lines belong to. The coordinator translates them into
 * `SelectionRoot` space using the block's layout rect plus its content offset.
 * All offsets are **segment-local UTF-16 code units**, matching
 * `native/segmentAdapter.ts`.
 */

/** A box in whatever coordinate space its producer documents. */
export interface LocalRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** One laid-out line of a text segment. */
export interface TextLineMetrics {
  /** Segment-local UTF-16 offset of the line's first character. */
  start: number;
  /** Segment-local UTF-16 offset just past the line's last character. */
  end: number;
  /**
   * `end` minus any trailing line terminator. Hit-testing and highlight
   * geometry work over `[start, visibleEnd]`: a `'\n'` occupies an offset in
   * the text stream but no horizontal space on the line, so treating it as
   * ordinary text would let a tap past the end of a line land *after* the
   * break, i.e. on the next line.
   */
  visibleEnd: number;
  x: number;
  y: number;
  w: number;
  h: number;
  /**
   * Optional exact per-character geometry: `charXs[i]` is the segment-local x
   * of the leading edge of the character at `start + i`, and the final entry is
   * the trailing edge of the last visible character — so the array holds
   * `visibleEnd - start + 1` values. Absent for the `onTextLayout` provider,
   * which reports lines but not characters; see `xForOffset` for what happens
   * then.
   */
  charXs?: readonly number[];
}

/** The line table for one text segment. */
export interface SegmentTextMetrics {
  lines: readonly TextLineMetrics[];
  /** Total UTF-16 length the lines account for; `lines[last].end`, or 0. */
  textLength: number;
}

/**
 * A line as reported by React Native's `onTextLayout`. Only the fields the
 * metrics need are declared, so the RN type does not have to be imported here
 * (this module must stay loadable without React Native).
 */
export interface RawTextLine {
  text: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export const EMPTY_TEXT_METRICS: SegmentTextMetrics = { lines: [], textLength: 0 };

function clamp(value: number, min: number, max: number): number {
  if (value < min) return min;
  if (value > max) return max;
  return value;
}

/** Length of the line terminator ending `text`, or 0 when there is none. */
function terminatorLength(text: string): number {
  if (text.endsWith('\r\n')) return 2;
  const last = text.charCodeAt(text.length - 1);
  // \n, \r, LINE SEPARATOR, PARAGRAPH SEPARATOR — the terminators a text
  // engine may hand back at a line end.
  if (last === 0x0a || last === 0x0d || last === 0x2028 || last === 0x2029) return 1;
  return 0;
}

/**
 * Build a segment's line table from `onTextLayout` lines. Offsets accumulate
 * from each line's own text length, which is what makes the result comparable
 * with the unit stream: `textLength` should equal the summed length of the
 * units the block registered, and a mismatch means the block renders text the
 * selection model does not know about (`SelectableBlock` warns about exactly
 * this in `__DEV__`).
 */
export function buildTextMetrics(rawLines: readonly RawTextLine[]): SegmentTextMetrics {
  const lines: TextLineMetrics[] = [];
  let start = 0;
  for (const raw of rawLines) {
    const text = raw.text ?? '';
    const end = start + text.length;
    lines.push({
      start,
      end,
      visibleEnd: end - terminatorLength(text),
      x: raw.x,
      y: raw.y,
      w: raw.width,
      h: raw.height,
    });
    start = end;
  }
  return { lines, textLength: start };
}

/**
 * Segment-local x of a segment-local `offset` on `line`.
 *
 * With `charXs` this is a table lookup. Without it — the `onTextLayout` case —
 * the position is interpolated linearly across the line's advance width by
 * UTF-16 offset. That is exact for a monospaced run and an approximation for
 * everything else: proportional fonts, mixed CJK/Latin, and any offset inside
 * a surrogate pair drift by up to about half a character width. It is accurate
 * enough to place a highlight edge and a drag handle, and it is the reason the
 * `charXs` slot exists in the contract.
 */
export function xForOffset(line: TextLineMetrics, offset: number): number {
  const span = line.visibleEnd - line.start;
  if (span <= 0) return line.x;
  const local = clamp(offset, line.start, line.visibleEnd) - line.start;
  const charXs = line.charXs;
  if (charXs && charXs.length > span) return charXs[local];
  return line.x + (line.w * local) / span;
}

/**
 * Segment-local offset nearest to a segment-local x on `line`. Ties inside a
 * character resolve to the nearer edge, so tapping the left half of a glyph
 * puts the caret before it and the right half after it — the standard text
 * cursor rule.
 */
export function offsetAtLineX(line: TextLineMetrics, x: number): number {
  const span = line.visibleEnd - line.start;
  if (span <= 0) return line.start;
  if (x <= line.x) return line.start;
  if (x >= line.x + line.w) return line.visibleEnd;

  const charXs = line.charXs;
  if (charXs && charXs.length > span) {
    // Linear scan: a line holds tens of characters, so a binary search would
    // trade readability for nothing measurable.
    for (let i = 0; i < span; i++) {
      const left = charXs[i];
      const right = charXs[i + 1];
      if (x < right) return line.start + (x - left <= right - x ? i : i + 1);
    }
    return line.visibleEnd;
  }
  return line.start + Math.round(((x - line.x) / line.w) * span);
}

/**
 * Segment-local offset for the character under a point on `line`.
 *
 * Caret placement uses nearest edge (`offsetAtLineX`), but long-press word
 * selection needs the character the finger is actually over. With nearest-edge
 * rounding, pressing the right half of a CJK glyph picks the following glyph,
 * which reads as a systematic one-character skew.
 */
export function offsetInsideLineX(line: TextLineMetrics, x: number): number {
  const span = line.visibleEnd - line.start;
  if (span <= 0) return line.start;
  if (x <= line.x) return line.start;
  if (x >= line.x + line.w) return line.visibleEnd - 1;

  const charXs = line.charXs;
  if (charXs && charXs.length > span) {
    for (let i = 0; i < span; i++) {
      if (x < charXs[i + 1]) return line.start + i;
    }
    return line.visibleEnd - 1;
  }
  return line.start + clamp(Math.floor(((x - line.x) / line.w) * span), 0, span - 1);
}

/**
 * The line a segment-local y falls on. A y above the first line resolves to
 * it, and a y below the last line resolves to that one, so a drag that leaves
 * the block vertically still yields a sensible endpoint instead of nothing.
 * Returns null only for an empty line table.
 */
export function lineAtY(metrics: SegmentTextMetrics, y: number): TextLineMetrics | null {
  const { lines } = metrics;
  if (lines.length === 0) return null;
  for (const line of lines) {
    if (y < line.y + line.h) return line;
  }
  return lines[lines.length - 1];
}

/**
 * Map a segment-local point to a segment-local UTF-16 offset. This is the
 * hit-test the whole gesture layer is built on.
 */
export function offsetAtLocalPoint(metrics: SegmentTextMetrics, x: number, y: number): number {
  const line = lineAtY(metrics, y);
  if (line === null) return 0;
  return offsetAtLineX(line, x);
}

/**
 * Highlight rectangles for the segment-local range `[from, to)`, one per
 * intersected line, in line order.
 *
 * A line whose text continues into the selection past its own visible end —
 * either because the text wrapped or because a `'\n'` was selected — is
 * stretched to the full line width. Without that, a multi-line selection would
 * show a ragged right edge stopping at the last glyph of each line, which
 * reads as "the newline is not selected" rather than as one continuous
 * selection.
 */
export function rectsForRange(
  metrics: SegmentTextMetrics,
  from: number,
  to: number
): LocalRect[] {
  const start = Math.min(from, to);
  const end = Math.max(from, to);
  const rects: LocalRect[] = [];
  if (end <= start) return rects;

  for (const line of metrics.lines) {
    const lineStart = Math.max(start, line.start);
    const lineEnd = Math.min(end, line.end);
    if (lineEnd <= lineStart) continue;

    const x0 = xForOffset(line, lineStart);
    // The selection runs past this line's visible text: fill to the line end.
    const continues = end > line.visibleEnd;
    const x1 = continues ? line.x + line.w : xForOffset(line, lineEnd);
    if (x1 <= x0) continue;
    rects.push({ x: x0, y: line.y, w: x1 - x0, h: line.h });
  }
  return rects;
}
