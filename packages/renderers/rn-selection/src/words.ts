/**
 * Word boundaries for long-press selection.
 *
 * A long press does not select a character — every text selection UI on every
 * platform selects the *word* under the finger and then lets the user refine
 * from there. Once the selection UI is ours (see `SELECTION_PLAN.md`), that
 * rule is ours to implement too.
 *
 * Two implementations behind one contract, mirroring `text.ts`:
 * `Intl.Segmenter` with word granularity where the engine has it, and a
 * script-class scan where it does not — which on React Native is the normal
 * case, because Hermes ships no `Intl.Segmenter`.
 *
 * Character classes are expressed as numeric code points throughout: repository
 * source is gated ASCII-only, so the non-ASCII punctuation and space sets
 * cannot be written as string literals.
 */

import { snapToGraphemeBoundary } from './text';

export interface WordBounds {
  start: number;
  end: number;
}

/**
 * Inclusive code-point ranges treated as "one character is one word": CJK
 * radicals, Kana, the CJK unified ideograph blocks, CJK compatibility
 * ideographs, Hangul syllables, the supplementary ideograph planes, and the
 * symbol/emoji blocks.
 *
 * Emoji belong here for the same reason ideographs do — a long press next to
 * one should select the neighbouring word, not the word plus the emoji. A
 * multi-code-point emoji (ZWJ sequence, skin tone, flag) still selects whole,
 * because `wordBoundsAt` widens the result to grapheme-cluster boundaries.
 */
const SINGLE_CHAR_SCRIPTS: ReadonlyArray<readonly [number, number]> = [
  [0x2600, 0x27bf],
  [0x2e80, 0x2eff],
  [0x3040, 0x30ff],
  [0x3400, 0x4dbf],
  [0x4e00, 0x9fff],
  [0xf900, 0xfaff],
  [0xac00, 0xd7af],
  [0x1f000, 0x1faff],
  [0x20000, 0x2fa1f],
];

/**
 * Non-ASCII whitespace: NBSP, OGHAM SPACE MARK, the EN/EM QUAD..HAIR SPACE
 * family, NARROW NO-BREAK SPACE, MEDIUM MATHEMATICAL SPACE, IDEOGRAPHIC SPACE
 * and ZERO WIDTH NO-BREAK SPACE.
 */
const WIDE_SPACES: ReadonlySet<number> = new Set([
  0x00a0, 0x1680, 0x2000, 0x2001, 0x2002, 0x2003, 0x2004, 0x2005, 0x2006, 0x2007, 0x2008, 0x2009,
  0x200a, 0x202f, 0x205f, 0x3000, 0xfeff,
]);

const ASCII_SPACES = ' \t\n\r\f\v';

/** ASCII punctuation, everything in the standard printable ranges. */
const ASCII_PUNCTUATION = '!"#$%&\'()*+,-./:;<=>?@[\\]^`{|}~';

/**
 * Non-ASCII punctuation that would otherwise classify as a word character and
 * glue two clauses into a single "word": MIDDLE DOT, curly quotes, EM DASH,
 * HORIZONTAL ELLIPSIS, IDEOGRAPHIC COMMA and FULL STOP, the angle and corner
 * and lenticular brackets, and the fullwidth comma / full stop / exclamation /
 * question / colon / semicolon / parentheses.
 */
const WIDE_PUNCTUATION: ReadonlySet<number> = new Set([
  0x00b7, 0x2018, 0x2019, 0x201c, 0x201d, 0x2014, 0x2026, 0x3001, 0x3002, 0x3008, 0x3009, 0x300a,
  0x300b, 0x300c, 0x300d, 0x3010, 0x3011, 0xff01, 0xff08, 0xff09, 0xff0c, 0xff0e, 0xff1a, 0xff1b,
  0xff1f,
]);

type CharClass = 'word' | 'space' | 'punct' | 'single';

function classify(codePoint: number): CharClass {
  if (codePoint < 0x80) {
    const char = String.fromCharCode(codePoint);
    if (ASCII_SPACES.includes(char)) return 'space';
    return ASCII_PUNCTUATION.includes(char) ? 'punct' : 'word';
  }
  if (WIDE_SPACES.has(codePoint)) return 'space';
  if (WIDE_PUNCTUATION.has(codePoint)) return 'punct';
  for (const [low, high] of SINGLE_CHAR_SCRIPTS) {
    if (codePoint >= low && codePoint <= high) return 'single';
  }
  return 'word';
}

/** The code point ending at `index`, and where it starts. Null at the head. */
function codePointBefore(text: string, index: number): { code: number; start: number } | null {
  if (index <= 0) return null;
  const trailing = text.codePointAt(index - 1);
  if (trailing === undefined) return null;
  // A low surrogate at `index - 1` is the tail of a pair starting one earlier.
  if (trailing >= 0xdc00 && trailing <= 0xdfff && index >= 2) {
    const paired = text.codePointAt(index - 2);
    if (paired !== undefined && paired > 0xffff) return { code: paired, start: index - 2 };
  }
  return { code: trailing, start: index - 1 };
}

/**
 * Look `Intl.Segmenter` up on every call rather than caching an instance, for
 * the same reason `text.ts` does: environments (and this package's own tests)
 * install or remove the global at runtime, and the construction cost is
 * negligible next to the work it guards.
 */
function getWordSegmenter(): Intl.Segmenter | undefined {
  const ctor = (Intl as { Segmenter?: typeof Intl.Segmenter }).Segmenter;
  return typeof ctor === 'function' ? new ctor(undefined, { granularity: 'word' }) : undefined;
}

function boundsWithSegmenter(
  text: string,
  offset: number,
  segmenter: Intl.Segmenter
): WordBounds | null {
  let previous: WordBounds | null = null;
  for (const segment of segmenter.segment(text)) {
    const start = segment.index;
    const end = start + segment.segment.length;
    // An offset on a boundary belongs to the segment starting there; at the
    // very end of the text there is no such segment and the last one wins.
    if (offset < end) return { start, end };
    previous = { start, end };
  }
  return previous;
}

/**
 * Scan outward from `offset` over characters of the same class. Runs of word
 * characters and runs of whitespace each select as a unit; punctuation and
 * ideographic characters select one at a time — a segmenter-less engine has no
 * dictionary, so grabbing a whole ideographic run would swallow an entire
 * clause instead of the character under the finger.
 */
function boundsWithClassScan(text: string, offset: number): WordBounds {
  // Prefer the character *at* the offset; at the very end of the text, fall
  // back to the one before it so a press past the last word still selects it.
  let probeIndex = offset < text.length ? offset : Math.max(0, offset - 1);
  let probe = text.codePointAt(probeIndex);
  if (probe === undefined) return { start: offset, end: offset };
  // The offset can land on the tail of a surrogate pair. Classifying a lone
  // low surrogate would call it a word character and grow a run through half
  // an astral character, so step back onto the pair itself first.
  if (probe >= 0xdc00 && probe <= 0xdfff) {
    const paired = codePointBefore(text, probeIndex + 1);
    if (paired !== null) {
      probe = paired.code;
      probeIndex = paired.start;
    }
  }

  const cls = classify(probe);
  const probeSize = String.fromCodePoint(probe).length;
  if (cls === 'single' || cls === 'punct') {
    return { start: probeIndex, end: probeIndex + probeSize };
  }

  let start = probeIndex;
  for (;;) {
    const previous = codePointBefore(text, start);
    if (previous === null || classify(previous.code) !== cls) break;
    start = previous.start;
  }

  let end = probeIndex;
  while (end < text.length) {
    const code = text.codePointAt(end);
    if (code === undefined || classify(code) !== cls) break;
    end += String.fromCodePoint(code).length;
  }
  return { start, end };
}

/**
 * The word-ish range containing `offset`, as segment-local UTF-16 offsets.
 *
 * Boundaries are widened to whole grapheme clusters (start backward, end
 * forward) exactly like `resolve.ts` widens a partial slice, so a long press
 * on an emoji or a combining sequence can never produce half of one.
 */
export function wordBoundsAt(text: string, offset: number): WordBounds {
  if (text.length === 0) return { start: 0, end: 0 };
  const clamped = offset < 0 ? 0 : offset > text.length ? text.length : offset;

  const segmenter = getWordSegmenter();
  const raw = segmenter
    ? (boundsWithSegmenter(text, clamped, segmenter) ?? boundsWithClassScan(text, clamped))
    : boundsWithClassScan(text, clamped);

  return {
    start: snapToGraphemeBoundary(text, raw.start, 'backward'),
    end: snapToGraphemeBoundary(text, raw.end, 'forward'),
  };
}
