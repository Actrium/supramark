import type { RawTextLine } from '../metrics';

export interface TextKitLine extends RawTextLine {
  // RN onTextLayout reports content-local rects; caret coordinates include Yoga insets.
  layoutX?: number;
  layoutY?: number;
}

export function matchesTextLayout(
  native: readonly TextKitLine[],
  rn: readonly RawTextLine[]
): boolean {
  const close = (a: number, b: number) => Math.abs(a - b) <= 0.01;
  return (
    native.length === rn.length &&
    native.every((line, i) => {
      const box = rn[i];
      return (
        line.text === box.text &&
        close(line.layoutX ?? line.x, box.x) &&
        close(line.layoutY ?? line.y, box.y) &&
        close(line.width, box.width) &&
        close(line.height, box.height)
      );
    })
  );
}
