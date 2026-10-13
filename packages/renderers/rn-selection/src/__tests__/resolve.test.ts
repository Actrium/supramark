import { describe, expect, test } from 'bun:test';
import type {
  SupramarkHeadingNode,
  SupramarkNode,
  SupramarkParagraphNode,
  SupramarkTextNode,
} from '@supramark/core';
import { linearizeForSelection } from '../linearize';
import type {
  SelectionAtomUnit,
  SelectionBoundaryUnit,
  SelectionBreakUnit,
  SelectionRange,
  SelectionSourceRange,
  SelectionTextUnit,
  SelectionUnit,
} from '../model';
import {
  buildUnitIndex,
  clampSelectionRange,
  detectDuplicateUnitIds,
  locateSelectionPoint,
  orderSelectionRange,
  resolveSelectionRange,
} from '../resolve';
import { serializeSelectionUnits } from '../serialize';

// A throwaway AST node — resolve.ts copies `node` but never inspects it.
const NODE = { type: 'text', value: '' } as SupramarkTextNode;

const tUnit = (
  unitId: string,
  nodeId: string,
  text: string,
  sourceRange?: SelectionSourceRange
): SelectionTextUnit => ({ kind: 'text', unitId, nodeId, text, node: NODE, sourceRange });
const brk = (unitId: string, nodeId: string): SelectionBreakUnit => ({
  kind: 'break',
  unitId,
  nodeId,
  text: '\n',
  reason: 'block',
  node: NODE,
});
const bound = (unitId: string, nodeId: string): SelectionBoundaryUnit => ({
  kind: 'boundary',
  unitId,
  nodeId,
  node: NODE,
  reason: 'custom',
});

const text = (value: string): SupramarkTextNode => ({ type: 'text', value }) as SupramarkTextNode;
const paragraph = (...children: SupramarkNode[]): SupramarkParagraphNode =>
  ({ type: 'paragraph', children }) as SupramarkParagraphNode;
const heading = (
  depth: 1 | 2 | 3 | 4 | 5 | 6,
  ...children: SupramarkNode[]
): SupramarkHeadingNode => ({ type: 'heading', depth, children }) as SupramarkHeadingNode;

describe('resolveSelectionRange', () => {
  test('orders visual endpoints without discarding gesture direction', () => {
    const units: SelectionUnit[] = [tUnit('u#0', 'u', 'HelloWorld')];
    const range = {
      anchor: { nodeId: 'u', unitId: 'u#0', offset: 8 },
      focus: { nodeId: 'u', unitId: 'u#0', offset: 2 },
    };
    expect(orderSelectionRange(buildUnitIndex(units), range)).toEqual({
      start: range.focus,
      end: range.anchor,
    });
  });

  test('selecting the whole stream serializes identically to the full stream', () => {
    const units = linearizeForSelection([heading(1, text('Hello')), paragraph(text('world'))]);
    const first = units[0];
    const last = units[units.length - 1];
    const resolved = resolveSelectionRange(units, {
      anchor: { nodeId: first.nodeId, unitId: first.unitId, offset: 0 },
      focus: { nodeId: last.nodeId, unitId: last.unitId, offset: 99 },
    });
    expect(serializeSelectionUnits(resolved, 'plainText')).toBe(
      serializeSelectionUnits(units, 'plainText')
    );
    expect(serializeSelectionUnits(resolved, 'markdown')).toBe(
      serializeSelectionUnits(units, 'markdown')
    );
  });

  test('a collapsed range resolves to nothing', () => {
    const units: SelectionUnit[] = [tUnit('u#0', 'u', 'HelloWorld')];
    const resolved = resolveSelectionRange(units, {
      anchor: { nodeId: 'u', unitId: 'u#0', offset: 4 },
      focus: { nodeId: 'u', unitId: 'u#0', offset: 4 },
    });
    expect(resolved).toEqual([]);
  });

  test('an intra-unit range slices the text and shifts the source range', () => {
    const units: SelectionUnit[] = [
      tUnit('u#0', 'u', 'HelloWorld', { startUtf16: 100, endUtf16: 110 }),
    ];
    const resolved = resolveSelectionRange(units, {
      anchor: { nodeId: 'u', unitId: 'u#0', offset: 2 },
      focus: { nodeId: 'u', unitId: 'u#0', offset: 7 },
    });
    expect(resolved).toHaveLength(1);
    const only = resolved[0] as SelectionTextUnit;
    expect(only.text).toBe('lloWo');
    expect(only.sourceRange?.startUtf16).toBe(102);
    expect(only.sourceRange?.endUtf16).toBe(107);
  });

  test('a reversed range (anchor after focus) normalizes to the forward result', () => {
    const units: SelectionUnit[] = [
      tUnit('u#0', 'u', 'HelloWorld', { startUtf16: 0, endUtf16: 10 }),
    ];
    const forward = resolveSelectionRange(units, {
      anchor: { nodeId: 'u', unitId: 'u#0', offset: 2 },
      focus: { nodeId: 'u', unitId: 'u#0', offset: 7 },
    });
    const reversed = resolveSelectionRange(units, {
      anchor: { nodeId: 'u', unitId: 'u#0', offset: 7 },
      focus: { nodeId: 'u', unitId: 'u#0', offset: 2 },
    });
    expect(reversed).toEqual(forward);
  });

  test('a multi-unit range half-cuts the ends and keeps the middle verbatim', () => {
    const units: SelectionUnit[] = [
      tUnit('a#0', 'a', 'HELLO'),
      tUnit('m#0', 'm', 'MIDDLE'),
      tUnit('z#0', 'z', 'WORLD'),
    ];
    const resolved = resolveSelectionRange(units, {
      anchor: { nodeId: 'a', unitId: 'a#0', offset: 2 },
      focus: { nodeId: 'z', unitId: 'z#0', offset: 3 },
    });
    expect(resolved).toHaveLength(3);
    expect((resolved[0] as SelectionTextUnit).text).toBe('LLO');
    expect((resolved[1] as SelectionTextUnit).text).toBe('MIDDLE');
    expect((resolved[2] as SelectionTextUnit).text).toBe('WOR');
    expect(serializeSelectionUnits(resolved, 'plainText')).toBe('LLOMIDDLEWOR');
  });

  test('hard boundaries clamp the range instead of being crossed', () => {
    // nodeId "b0"/"b2" hold a single zero-text boundary each, so the nodeId walk
    // encodes before (offset 0) / after (offset > 0) as intraOffset 0 / 1.
    const units: SelectionUnit[] = [
      bound('b0#0', 'b0'),
      tUnit('t1#0', 't1', 'MID'),
      bound('b2#0', 'b2'),
    ];

    // Anchored on the head boundary: snaps to the nearest region on the focus
    // side, so the selection never includes the boundary itself.
    const fromHead = resolveSelectionRange(units, {
      anchor: { nodeId: 'b0', offset: 0 },
      focus: { nodeId: 't1', unitId: 't1#0', offset: 3 },
    });
    expect(fromHead.every(u => u.kind !== 'boundary')).toBe(true);
    expect(serializeSelectionUnits(fromHead, 'plainText')).toBe('MID');

    // Dragging past the tail boundary clamps the focus to the region's edge.
    const toTail = resolveSelectionRange(units, {
      anchor: { nodeId: 't1', unitId: 't1#0', offset: 0 },
      focus: { nodeId: 'b2', offset: 1 },
    });
    expect(toTail.every(u => u.kind !== 'boundary')).toBe(true);
    expect(serializeSelectionUnits(toTail, 'plainText')).toBe('MID');

    // Reversed (dragging left into the head boundary) clamps identically.
    const reversed = resolveSelectionRange(units, {
      anchor: { nodeId: 't1', unitId: 't1#0', offset: 3 },
      focus: { nodeId: 'b0', offset: 1 },
    });
    expect(reversed.every(u => u.kind !== 'boundary')).toBe(true);
    expect(serializeSelectionUnits(reversed, 'plainText')).toBe('MID');

    // Both endpoints on boundaries: nothing selectable is in reach.
    const bothBlocked = resolveSelectionRange(units, {
      anchor: { nodeId: 'b0', offset: 1 },
      focus: { nodeId: 'b2', offset: 1 },
    });
    expect(bothBlocked).toHaveLength(0);
  });
});

describe('resolveSelectionRange widens grapheme-unsafe offsets (regression)', () => {
  test('an offset splitting an astral emoji surrogate pair is widened to include it whole', () => {
    const units: SelectionUnit[] = [tUnit('u#0', 'u', 'a\u{1F600}b')]; // 'a', surrogate pair [1,3), 'b'; length 4
    // anchor/focus both land inside the surrogate pair (offset 2): a collapsed
    // selection there still resolves to nothing, so pick a range that starts
    // inside the pair and ends after it, forcing the start to widen backward.
    const resolved = resolveSelectionRange(units, {
      anchor: { nodeId: 'u', unitId: 'u#0', offset: 2 },
      focus: { nodeId: 'u', unitId: 'u#0', offset: 4 },
    });
    expect(resolved).toHaveLength(1);
    expect((resolved[0] as SelectionTextUnit).text).toBe('\u{1F600}b');
  });

  test('an offset splitting a combining-mark cluster is widened forward at the tail', () => {
    // 'c','a','f' each length 1, then a decomposed 2-unit 'e' + U+0301 (combining
    // acute) cluster; length 5. Focus at offset 4 lands between the base 'e' and
    // its combining mark, so the tail must widen forward to include the mark.
    const units: SelectionUnit[] = [tUnit('u#0', 'u', `caf${'é'}`)];
    const resolved = resolveSelectionRange(units, {
      anchor: { nodeId: 'u', unitId: 'u#0', offset: 0 },
      focus: { nodeId: 'u', unitId: 'u#0', offset: 4 },
    });
    expect(resolved).toHaveLength(1);
    expect((resolved[0] as SelectionTextUnit).text).toBe(`caf${'é'}`);
  });

  test('an offset already on a grapheme boundary is unaffected (no-op)', () => {
    const units: SelectionUnit[] = [tUnit('u#0', 'u', 'HelloWorld')];
    const resolved = resolveSelectionRange(units, {
      anchor: { nodeId: 'u', unitId: 'u#0', offset: 2 },
      focus: { nodeId: 'u', unitId: 'u#0', offset: 7 },
    });
    expect(resolved).toHaveLength(1);
    expect((resolved[0] as SelectionTextUnit).text).toBe('lloWo');
  });
});

describe('splitTextUnit payload preservation (regression)', () => {
  const codeUnit: SelectionTextUnit = {
    kind: 'text',
    unitId: 'c#0',
    nodeId: 'c',
    text: 'foo',
    node: NODE,
    payload: { plainText: 'foo', markdown: '`foo`', source: '`foo`' },
  };

  test('a fully covered payload unit keeps its whole-unit markdown/source', () => {
    const resolved = resolveSelectionRange([codeUnit], {
      anchor: { nodeId: 'c', unitId: 'c#0', offset: 0 },
      focus: { nodeId: 'c', unitId: 'c#0', offset: 3 },
    });
    expect(serializeSelectionUnits(resolved, 'markdown')).toBe('`foo`');
    expect(serializeSelectionUnits(resolved, 'source')).toBe('`foo`');
    expect(serializeSelectionUnits(resolved, 'plainText')).toBe('foo');
  });

  test('a partial slice drops the whole-unit syntax instead of leaking it', () => {
    const resolved = resolveSelectionRange([codeUnit], {
      anchor: { nodeId: 'c', unitId: 'c#0', offset: 1 },
      focus: { nodeId: 'c', unitId: 'c#0', offset: 3 },
    });
    expect(serializeSelectionUnits(resolved, 'markdown')).toBe('oo');
  });
});

describe('zero-text units addressed by unitId (regression)', () => {
  test('a boundary is a hard limit: selecting it via {unitId} resolves to nothing', () => {
    // Boundaries are selection limits, not selectable content (see
    // clampSelectionRange): addressing one directly must not resurrect the
    // "fully covered" span the old bridge-era rule produced.
    const units: SelectionUnit[] = [bound('b#0', 'b')];
    const resolved = resolveSelectionRange(units, {
      anchor: { nodeId: 'b', unitId: 'b#0', offset: 0 },
      focus: { nodeId: 'b', unitId: 'b#0', offset: 1 },
    });
    expect(resolved).toHaveLength(0);
  });
});

describe('locateSelectionPoint', () => {
  test('a unitId hit wins over the nodeId walk', () => {
    const units: SelectionUnit[] = [tUnit('shared#0', 'shared', 'AAAA'), brk('shared#1', 'shared')];
    const index = buildUnitIndex(units);

    const byUnit = locateSelectionPoint(index, { nodeId: 'shared', unitId: 'shared#1', offset: 0 });
    expect(byUnit.unitIndex).toBe(1); // the trailing break, targeted directly

    const byNode = locateSelectionPoint(index, { nodeId: 'shared', offset: 2 });
    expect(byNode.unitIndex).toBe(0); // walks to the content unit
    expect(byNode.intraOffset).toBe(2);
  });

  test('offsets past a node clamp to the end of its last content unit', () => {
    const units: SelectionUnit[] = [tUnit('shared#0', 'shared', 'AAAA'), brk('shared#1', 'shared')];
    const index = buildUnitIndex(units);
    const located = locateSelectionPoint(index, { nodeId: 'shared', offset: 999 });
    // The break (length 1) is the last content unit reachable by the walk.
    expect(located.unitIndex).toBe(1);
    expect(located.intraOffset).toBe(1);
  });
});

describe('duplicate unit ids', () => {
  test('detectDuplicateUnitIds reports every collision with both indexes', () => {
    const units: SelectionUnit[] = [
      tUnit('dup#0', 'a', 'AA'),
      tUnit('dup#0', 'b', 'BB'),
      tUnit('ok#0', 'c', 'CC'),
    ];
    expect(detectDuplicateUnitIds(units)).toEqual([
      { unitId: 'dup#0', firstIndex: 0, duplicateIndex: 1 },
    ]);
  });

  test('buildUnitIndex keeps the first holder instead of silently overwriting', () => {
    const units: SelectionUnit[] = [tUnit('dup#0', 'a', 'AA'), tUnit('dup#0', 'b', 'BB')];
    const index = buildUnitIndex(units);
    expect(index.byUnitId.get('dup#0')).toBe(0);
    // Both units stay in the stream, in document order.
    expect(index.entries.map(entry => entry.unit.unitId)).toEqual(['dup#0', 'dup#0']);
  });
});

describe('clampSelectionRange idempotency', () => {
  const atom = (unitId: string, nodeId: string): SelectionAtomUnit => ({
    kind: 'atom',
    unitId,
    nodeId,
    label: 'image',
    node: NODE,
    payload: {},
  });

  // Two selectable regions ("LEFTTAIL", "RIGHT") split by two hard boundaries.
  const units: SelectionUnit[] = [
    tUnit('a#0', 'a', 'LEFT'),
    tUnit('a#1', 'a', 'TAIL'),
    atom('img#0', 'img'),
    tUnit('b#0', 'b', 'RIGHT'),
    bound('end#0', 'end'),
  ];
  const index = buildUnitIndex(units);

  const cases: Array<[string, SelectionRange]> = [
    [
      'same region, one unit',
      {
        anchor: { nodeId: 'a', unitId: 'a#0', offset: 1 },
        focus: { nodeId: 'a', unitId: 'a#0', offset: 4 },
      },
    ],
    [
      'same region, two units',
      {
        anchor: { nodeId: 'a', unitId: 'a#0', offset: 2 },
        focus: { nodeId: 'a', unitId: 'a#1', offset: 2 },
      },
    ],
    [
      'focus crosses the atom into the next region',
      {
        anchor: { nodeId: 'a', unitId: 'a#0', offset: 1 },
        focus: { nodeId: 'b', unitId: 'b#0', offset: 2 },
      },
    ],
    [
      'focus offset past the stream end',
      {
        anchor: { nodeId: 'a', unitId: 'a#0', offset: 1 },
        focus: { nodeId: 'b', unitId: 'b#0', offset: 999 },
      },
    ],
    [
      'anchor on the atom, focus forward',
      {
        anchor: { nodeId: 'img', unitId: 'img#0', offset: 0 },
        focus: { nodeId: 'b', unitId: 'b#0', offset: 2 },
      },
    ],
    [
      'anchor on the atom, focus backward',
      {
        anchor: { nodeId: 'img', unitId: 'img#0', offset: 1 },
        focus: { nodeId: 'a', unitId: 'a#0', offset: 0 },
      },
    ],
    [
      'focus on the trailing boundary',
      {
        anchor: { nodeId: 'a', unitId: 'a#0', offset: 1 },
        focus: { nodeId: 'end', unitId: 'end#0', offset: 0 },
      },
    ],
    [
      'both endpoints on boundaries',
      {
        anchor: { nodeId: 'img', unitId: 'img#0', offset: 0 },
        focus: { nodeId: 'end', unitId: 'end#0', offset: 1 },
      },
    ],
  ];

  for (const [name, range] of cases) {
    test(`clamping twice equals clamping once: ${name}`, () => {
      const once = clampSelectionRange(index, range);
      expect(clampSelectionRange(index, once)).toEqual(once);
    });
  }

  test('the clamp actually moves a cross-boundary focus (not a no-op)', () => {
    const once = clampSelectionRange(index, {
      anchor: { nodeId: 'a', unitId: 'a#0', offset: 1 },
      focus: { nodeId: 'b', unitId: 'b#0', offset: 2 },
    });
    // The focus clamps to the anchor region's trailing edge, so the second
    // clamp sees one region and returns its input verbatim.
    expect(once).toEqual({
      anchor: { nodeId: 'a', unitId: 'a#0', offset: 1 },
      focus: { nodeId: 'a', unitId: 'a#1', offset: 4 },
    });
  });
});

describe('empty unit stream', () => {
  test('clamp keeps the input range and locate resolves to the document start', () => {
    const index = buildUnitIndex([]);
    const range: SelectionRange = {
      anchor: { nodeId: 'gone', offset: 3 },
      focus: { nodeId: 'also-gone', unitId: 'nope#0', offset: 1 },
    };
    expect(clampSelectionRange(index, range)).toEqual(range);
    expect(locateSelectionPoint(index, range.anchor)).toEqual({ unitIndex: 0, intraOffset: 0 });
    expect(locateSelectionPoint(index, { nodeId: 'x', offset: 5 })).toEqual({
      unitIndex: 0,
      intraOffset: 0,
    });
  });
});
