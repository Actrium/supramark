import type { RawTextLine } from '../metrics';

/** Most platforms publish their line metrics directly on the Text layout event. */
export function useNativeLineMetrics(publish: (lines: readonly RawTextLine[]) => void) {
  return (_target: number, lines: readonly RawTextLine[]) => publish(lines);
}
