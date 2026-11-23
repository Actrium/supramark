import { expect, test } from 'bun:test';
import { matchesTextLayout } from '../coordinator/matchesTextLayout';

test('TextKit must match RN text and content-local line boxes before publishing carets', () => {
  const rn = { text: 'iiWW', x: 0, y: 10, width: 45, height: 18 };
  const native = { ...rn, x: 8, y: 14, layoutX: 0, layoutY: 10 };
  expect(matchesTextLayout([native], [rn])).toBe(true);
  for (const delta of [
    { width: 43 },
    { height: 14 },
    { layoutX: 2 },
    { layoutY: 8 },
    { text: 'iiW' },
  ]) {
    expect(matchesTextLayout([{ ...native, ...delta }], [rn])).toBe(false);
  }
  expect(matchesTextLayout([], [rn])).toBe(false);
});
