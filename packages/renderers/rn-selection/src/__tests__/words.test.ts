import { afterEach, describe, expect, test } from 'bun:test';
import { wordBoundsAt } from '../words';

const STAR = '\u{1F31F}'; // astral, 2 UTF-16 units
// Written as escapes because repository source is gated ASCII-only.
const CJK = String.fromCodePoint(0x4f60, 0x597d, 0x4e16, 0x754c); // four ideographs
const FULLWIDTH_COMMA = String.fromCodePoint(0xff0c);

/**
 * Bun runs on JavaScriptCore, which HAS `Intl.Segmenter` — the opposite of the
 * runtime that matters most here, since Hermes ships without it. Both paths
 * therefore need exercising, and the fallback is the one React Native actually
 * takes on device.
 */
function withoutSegmenter<T>(fn: () => T): T {
  const holder = Intl as { Segmenter?: unknown };
  const real = holder.Segmenter;
  delete holder.Segmenter;
  try {
    return fn();
  } finally {
    if (real !== undefined) holder.Segmenter = real;
  }
}

afterEach(() => {
  // Guard against a failing assertion leaving the global stripped.
  expect(typeof (Intl as { Segmenter?: unknown }).Segmenter).toBe('function');
});

describe('wordBoundsAt (Intl.Segmenter present)', () => {
  test('selects the word under the offset', () => {
    expect(wordBoundsAt('hello world', 2)).toEqual({ start: 0, end: 5 });
    expect(wordBoundsAt('hello world', 8)).toEqual({ start: 6, end: 11 });
  });

  test('an offset on a word boundary takes the following word', () => {
    expect(wordBoundsAt('hello world', 6)).toEqual({ start: 6, end: 11 });
  });

  test('an offset at the very end takes the last word', () => {
    expect(wordBoundsAt('hello world', 11)).toEqual({ start: 6, end: 11 });
  });

  test('empty text yields an empty range', () => {
    expect(wordBoundsAt('', 0)).toEqual({ start: 0, end: 0 });
  });

  test('an out-of-range offset is clamped rather than rejected', () => {
    expect(wordBoundsAt('hi', -5)).toEqual({ start: 0, end: 2 });
    expect(wordBoundsAt('hi', 99)).toEqual({ start: 0, end: 2 });
  });
});

describe('wordBoundsAt (segmenter-less fallback, the React Native case)', () => {
  test('selects a run of word characters', () => {
    withoutSegmenter(() => {
      expect(wordBoundsAt('hello world', 2)).toEqual({ start: 0, end: 5 });
      expect(wordBoundsAt('hello world', 8)).toEqual({ start: 6, end: 11 });
    });
  });

  test('selects a run of whitespace as one unit', () => {
    withoutSegmenter(() => {
      expect(wordBoundsAt('a   b', 2)).toEqual({ start: 1, end: 4 });
    });
  });

  test('punctuation selects one character at a time', () => {
    withoutSegmenter(() => {
      expect(wordBoundsAt('a, b', 1)).toEqual({ start: 1, end: 2 });
    });
  });

  test('an ideograph selects alone rather than swallowing the clause', () => {
    withoutSegmenter(() => {
      expect(wordBoundsAt(CJK, 1)).toEqual({ start: 1, end: 2 });
    });
  });

  test('fullwidth punctuation does not glue two clauses together', () => {
    withoutSegmenter(() => {
      const text = `ab${FULLWIDTH_COMMA}cd`;
      expect(wordBoundsAt(text, 0)).toEqual({ start: 0, end: 2 });
      expect(wordBoundsAt(text, 2)).toEqual({ start: 2, end: 3 });
      expect(wordBoundsAt(text, 3)).toEqual({ start: 3, end: 5 });
    });
  });

  test('an offset inside a surrogate pair never splits it', () => {
    withoutSegmenter(() => {
      // `${STAR}` occupies offsets 0..2; probing its low surrogate must widen
      // outward, not return a lone half.
      const bounds = wordBoundsAt(`${STAR}x`, 1);
      expect(bounds.start).toBe(0);
      expect(bounds.end).toBeGreaterThanOrEqual(2);
    });
  });

  test('scanning backward steps over a whole surrogate pair', () => {
    withoutSegmenter(() => {
      // A word run preceded by an astral character: the backward scan must not
      // stop half way into the pair.
      const text = `${STAR}abc`;
      expect(wordBoundsAt(text, 3)).toEqual({ start: 2, end: 5 });
    });
  });

  test('empty text yields an empty range', () => {
    withoutSegmenter(() => {
      expect(wordBoundsAt('', 0)).toEqual({ start: 0, end: 0 });
    });
  });
});
