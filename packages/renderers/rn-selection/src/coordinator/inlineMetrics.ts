import type { SegmentTextMetrics } from '../metrics';
import type { SelectionUnitIndex } from '../resolve';
import { buildSegmentSpans } from '../native/segmentAdapter';
import type { LayoutRect } from './registry';

/** Anchor text interpolation to measured native attachments instead of averaging their width into text. */
export function withInlineObjectMetrics(
  metrics: SegmentTextMetrics,
  unitIds: readonly string[],
  index: SelectionUnitIndex,
  objects: ReadonlyMap<string, LayoutRect>
): SegmentTextMetrics | undefined {
  // The display projection includes one native placeholder for each object.
  const spans = buildSegmentSpans({ unitIds }, index);
  const atoms = spans.filter(
    span => index.entries[index.byUnitId.get(span.unitId)!].unit.kind === 'atom'
  );
  // Plain paragraphs keep their original line provider and avoid allocating character tables.
  if (atoms.length === 0) return metrics;
  // Wait for actual attachment layout before publishing geometry for either side.
  if (atoms.some(atom => !objects.has(atom.unitId))) return undefined;
  return {
    ...metrics,
    lines: metrics.lines.map(line => {
      // The line endpoints and measured object edges bound each plain-text run.
      const anchors = [
        { offset: line.start, x: line.x },
        { offset: line.visibleEnd, x: line.x + line.w },
      ];
      for (const atom of atoms) {
        // Only attachments belonging to this native line contribute horizontal anchors.
        if (atom.start < line.start || atom.end > line.visibleEnd) continue;
        const rect = objects.get(atom.unitId)!;
        anchors.push({ offset: atom.start, x: rect.x }, { offset: atom.end, x: rect.x + rect.w });
      }
      anchors.sort((a, b) => a.offset - b.offset);
      // Character geometry remains approximate within text runs, but object widths are exact.
      const charXs: number[] = [];
      for (let i = 1; i < anchors.length; i += 1) {
        const from = anchors[i - 1];
        const to = anchors[i];
        for (let offset = from.offset; offset <= to.offset; offset += 1) {
          charXs[offset - line.start] =
            from.offset === to.offset
              ? to.x
              : from.x + ((to.x - from.x) * (offset - from.offset)) / (to.offset - from.offset);
        }
      }
      return { ...line, charXs };
    }),
  };
}
