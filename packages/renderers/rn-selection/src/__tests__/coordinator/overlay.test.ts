import { describe, expect, test } from 'bun:test';
import type { SupramarkTextNode } from '@supramark/core';
import { buildTextMetrics } from '../../metrics';
import type {
  SelectionBreakUnit,
  SelectionRange,
  SelectionTextUnit,
  SelectionUnit,
} from '../../model';
import { buildUnitIndex, resolveSelectionRange } from '../../resolve';
import type { LayoutRect, RegisteredBlock } from '../../coordinator/registry';
import { computeSelectionRects } from '../../coordinator/overlay';

// overlay copies `node` but never inspects it.
const NODE = { type: 'text', value: '' } as SupramarkTextNode;
const tUnit = (unitId: string, nodeId: string, text: string): SelectionTextUnit => ({
  kind: 'text',
  unitId,
  nodeId,
  text,
  node: NODE,
});
const brk = (unitId: string, nodeId: string): SelectionBreakUnit => ({
  kind: 'break',
  unitId,
  nodeId,
  text: '\n',
  reason: 'block',
  node: NODE,
});

const block = (
  nodeId: string,
  unitIds: string[],
  rect: LayoutRect | undefined
): RegisteredBlock => ({ nodeId, unitIds, kind: 'text', rect });

/** Resolve a range against a unit stream the way the store does. */
function rectsFor(
  blocks: readonly RegisteredBlock[],
  units: readonly SelectionUnit[],
  range: SelectionRange | null
) {
  const index = buildUnitIndex(units);
  const covered = range === null ? [] : resolveSelectionRange(units, range, index);
  return computeSelectionRects({ blocks, range, units: covered, index });
}

describe('computeSelectionRects block fallback', () => {
  // A block with no metrics yet cannot place a rect inside its own text, so it
  // highlights whole — the behaviour the whole layer had before metrics.
  const units: SelectionUnit[] = [tUnit('a#0', 'a', 'hello'), brk('a#1', 'a')];
  const range: SelectionRange = {
    anchor: { nodeId: 'a', unitId: 'a#0', offset: 0 },
    focus: { nodeId: 'a', unitId: 'a#0', offset: 5 },
  };

  test('empty selection yields no rects', () => {
    const blocks = [block('a', ['a#0'], { x: 0, y: 0, w: 10, h: 20 })];
    expect(rectsFor(blocks, units, null)).toEqual([]);
  });

  test('a covered unmeasured block yields its whole rect', () => {
    const rect = { x: 0, y: 0, w: 10, h: 20 };
    expect(rectsFor([block('a', ['a#0'], rect)], units, range)).toEqual([rect]);
  });

  test('a covered block without a layout rect is skipped', () => {
    expect(rectsFor([block('a', ['a#0'], undefined)], units, range)).toEqual([]);
  });

  test('the fallback copies the rect rather than aliasing the registry', () => {
    const rect = { x: 0, y: 0, w: 10, h: 20 };
    const [out] = rectsFor([block('a', ['a#0'], rect)], units, range);
    expect(out).not.toBe(rect);
    expect(out).toEqual(rect);
  });

  test('a covered break unit not owned by any block adds no phantom rect', () => {
    const rect = { x: 0, y: 0, w: 30, h: 20 };
    const wide: SelectionRange = {
      anchor: { nodeId: 'a', unitId: 'a#0', offset: 0 },
      focus: { nodeId: 'a', unitId: 'a#1', offset: 1 },
    };
    // a#1 (the block break) belongs to no block.unitIds -> one rect, not two.
    expect(rectsFor([block('a', ['a#0'], rect)], units, wide)).toEqual([rect]);
  });
});

describe('computeSelectionRects text precision', () => {
  // One block, one 100pt line of 'hello world' (11 chars => ~9.09pt each).
  const units: SelectionUnit[] = [tUnit('p#0', 'p', 'hello world'), brk('p#1', 'p')];
  const measured = (): RegisteredBlock => ({
    nodeId: 'p',
    unitIds: ['p#0'],
    kind: 'text',
    rect: { x: 10, y: 100, w: 200, h: 20 },
    metrics: buildTextMetrics([{ text: 'hello world', x: 0, y: 0, width: 110, height: 20 }]),
  });

  test('a partial selection highlights only the selected characters', () => {
    const rects = rectsFor([measured()], units, {
      anchor: { nodeId: 'p', unitId: 'p#0', offset: 0 },
      focus: { nodeId: 'p', unitId: 'p#0', offset: 5 },
    });
    // 'hello' = 5 of 11 characters => 50pt wide, at the block's origin.
    expect(rects).toEqual([{ x: 10, y: 100, w: 50, h: 20 }]);
  });

  test('a selection in the middle of the line starts where the text does', () => {
    const rects = rectsFor([measured()], units, {
      anchor: { nodeId: 'p', unitId: 'p#0', offset: 6 },
      focus: { nodeId: 'p', unitId: 'p#0', offset: 11 },
    });
    expect(rects).toEqual([{ x: 70, y: 100, w: 50, h: 20 }]);
  });

  test('rects are offset by the block content box', () => {
    const block = measured();
    block.contentOffset = { x: 8, y: 4 };
    const rects = rectsFor([block], units, {
      anchor: { nodeId: 'p', unitId: 'p#0', offset: 0 },
      focus: { nodeId: 'p', unitId: 'p#0', offset: 5 },
    });
    expect(rects).toEqual([{ x: 18, y: 104, w: 50, h: 20 }]);
  });

  test('a multi-line block yields one rect per line', () => {
    const block: RegisteredBlock = {
      nodeId: 'p',
      unitIds: ['p#0'],
      kind: 'text',
      rect: { x: 0, y: 0, w: 60, h: 40 },
      metrics: buildTextMetrics([
        { text: 'hello ', x: 0, y: 0, width: 60, height: 20 },
        { text: 'world', x: 0, y: 20, width: 50, height: 20 },
      ]),
    };
    const rects = rectsFor([block], units, {
      anchor: { nodeId: 'p', unitId: 'p#0', offset: 3 },
      focus: { nodeId: 'p', unitId: 'p#0', offset: 8 },
    });
    // First line: from character 3 to its end, stretched to the full line
    // width because the selection continues onto the next line.
    expect(rects).toEqual([
      { x: 30, y: 0, w: 30, h: 20 },
      { x: 0, y: 20, w: 20, h: 20 },
    ]);
  });

  test('an unselected line contributes no rect', () => {
    const block: RegisteredBlock = {
      nodeId: 'p',
      unitIds: ['p#0'],
      kind: 'text',
      rect: { x: 0, y: 0, w: 60, h: 40 },
      metrics: buildTextMetrics([
        { text: 'hello ', x: 0, y: 0, width: 60, height: 20 },
        { text: 'world', x: 0, y: 20, width: 50, height: 20 },
      ]),
    };
    const rects = rectsFor([block], units, {
      anchor: { nodeId: 'p', unitId: 'p#0', offset: 6 },
      focus: { nodeId: 'p', unitId: 'p#0', offset: 11 },
    });
    expect(rects).toEqual([{ x: 0, y: 20, w: 50, h: 20 }]);
  });

  test('clips a partially visible line to its nested scroll viewport', () => {
    const block = measured();
    block.clipRect = { x: 0, y: 110, w: 200, h: 30 };
    const rects = rectsFor([block], units, {
      anchor: { nodeId: 'p', unitId: 'p#0', offset: 0 },
      focus: { nodeId: 'p', unitId: 'p#0', offset: 5 },
    });
    expect(rects).toEqual([
      {
        x: 10,
        y: 110,
        w: 50,
        h: 10,
        startHandleVisible: false,
        endHandleVisible: false,
      },
    ]);
  });

  test('does not paint a selected block outside its nested viewport', () => {
    const block = measured();
    block.clipRect = { x: 0, y: 200, w: 200, h: 30 };
    const rects = rectsFor([block], units, {
      anchor: { nodeId: 'p', unitId: 'p#0', offset: 0 },
      focus: { nodeId: 'p', unitId: 'p#0', offset: 5 },
    });
    expect(rects).toEqual([]);
  });
});

describe('computeSelectionRects across blocks', () => {
  const units: SelectionUnit[] = [
    tUnit('a#0', 'a', 'aaaa'),
    brk('a#1', 'a'),
    tUnit('b#0', 'b', 'bbbb'),
    brk('b#1', 'b'),
  ];
  const range: SelectionRange = {
    anchor: { nodeId: 'a', unitId: 'a#0', offset: 2 },
    focus: { nodeId: 'b', unitId: 'b#0', offset: 2 },
  };

  const blockA = (): RegisteredBlock => ({
    nodeId: 'a',
    unitIds: ['a#0'],
    kind: 'text',
    rect: { x: 0, y: 0, w: 40, h: 20 },
    metrics: buildTextMetrics([{ text: 'aaaa', x: 0, y: 0, width: 40, height: 20 }]),
  });
  const blockB = (): RegisteredBlock => ({
    nodeId: 'b',
    unitIds: ['b#0'],
    kind: 'text',
    rect: { x: 0, y: 20, w: 40, h: 20 },
    metrics: buildTextMetrics([{ text: 'bbbb', x: 0, y: 0, width: 40, height: 20 }]),
  });

  test('each block contributes its own share of the range', () => {
    // The first block runs from the anchor to its end (stretched, because the
    // selection continues), the second from its start to the focus.
    expect(rectsFor([blockA(), blockB()], units, range)).toEqual([
      { x: 20, y: 0, w: 20, h: 20 },
      { x: 0, y: 20, w: 20, h: 20 },
    ]);
  });

  test('adjacent rects are not merged', () => {
    // The old block-level overlay merged vertically contiguous rects into one
    // union box. Per-line rects must stay separate: merging would square off
    // the ragged first and last lines that make a text selection readable.
    const rects = rectsFor([blockA(), blockB()], units, range);
    expect(rects.length).toBe(2);
  });

  test('an uncovered block in between contributes nothing', () => {
    const middle: RegisteredBlock = {
      nodeId: 'z',
      unitIds: ['z#0'],
      kind: 'text',
      rect: { x: 0, y: 10, w: 40, h: 5 },
    };
    const rects = rectsFor([blockA(), middle, blockB()], units, range);
    expect(rects).toEqual([
      { x: 20, y: 0, w: 20, h: 20 },
      { x: 0, y: 20, w: 20, h: 20 },
    ]);
  });

  test('a measured and an unmeasured block mix without either disappearing', () => {
    const bare = blockB();
    delete bare.metrics;
    expect(rectsFor([blockA(), bare], units, range)).toEqual([
      { x: 20, y: 0, w: 20, h: 20 },
      { x: 0, y: 20, w: 40, h: 20 },
    ]);
  });
});
