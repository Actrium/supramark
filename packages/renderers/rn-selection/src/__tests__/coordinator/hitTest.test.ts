import { describe, expect, test } from 'bun:test';
import type { SupramarkTextNode } from '@supramark/core';
import type { SelectionTextUnit, SelectionUnit } from '../../model';
import { buildTextMetrics } from '../../metrics';
import { buildUnitIndex } from '../../resolve';
import type { LayoutRect, RegisteredBlock } from '../../coordinator/registry';
import {
  chooseBlock,
  containingBlock,
  localizePoint,
  pointInRect,
  resolvePointToSelection,
  verticalGap,
} from '../../coordinator/hitTest';

const NODE = { type: 'text', value: '' } as SupramarkTextNode;
const tUnit = (unitId: string, nodeId: string, text: string): SelectionTextUnit => ({
  kind: 'text',
  unitId,
  nodeId,
  text,
  node: NODE,
});

// Three stacked text blocks (root coords): A y0..20, B y30..50, C y60..80, x 0..100.
const units: SelectionUnit[] = [
  tUnit('A#0', 'A', 'AAAAA'),
  tUnit('B#0', 'B', 'BBBBB'),
  tUnit('C#0', 'C', 'CCCCC'),
];
const index = buildUnitIndex(units);

const rectA: LayoutRect = { x: 0, y: 0, w: 100, h: 20 };
const rectB: LayoutRect = { x: 0, y: 30, w: 100, h: 20 };
const rectC: LayoutRect = { x: 0, y: 60, w: 100, h: 20 };

const blocks = (): RegisteredBlock[] => [
  { nodeId: 'A', unitIds: ['A#0'], kind: 'text', rect: rectA },
  { nodeId: 'B', unitIds: ['B#0'], kind: 'text', rect: rectB },
  { nodeId: 'C', unitIds: ['C#0'], kind: 'text', rect: rectC },
];

describe('hitTest primitives', () => {
  test('pointInRect is inclusive on the rect edges', () => {
    expect(pointInRect({ x: 0, y: 0 }, rectA)).toBe(true);
    expect(pointInRect({ x: 100, y: 20 }, rectA)).toBe(true);
    expect(pointInRect({ x: 101, y: 10 }, rectA)).toBe(false);
  });

  test('verticalGap is 0 inside the band and grows outside', () => {
    expect(verticalGap({ x: 0, y: 10 }, rectA)).toBe(0);
    expect(verticalGap({ x: 0, y: -5 }, rectA)).toBe(5);
    expect(verticalGap({ x: 0, y: 25 }, rectA)).toBe(5);
  });

  test('containingBlock requires a direct rect hit', () => {
    expect(containingBlock(blocks(), { x: 50, y: 10 })?.nodeId).toBe('A');
    expect(containingBlock(blocks(), { x: 50, y: 24 })).toBeNull();
    expect(
      containingBlock([{ nodeId: 'A', unitIds: ['A#0'], kind: 'text' }], { x: 10, y: 10 })
    ).toBeNull();
  });

  test('a nested viewport clips both visible hits and hidden blocks', () => {
    const partial = blocks()[0];
    partial.clipRect = { x: 0, y: 10, w: 100, h: 20 };
    expect(containingBlock([partial], { x: 50, y: 15 })?.nodeId).toBe('A');
    expect(containingBlock([partial], { x: 50, y: 5 })).toBeNull();

    const hidden = blocks()[1];
    hidden.clipRect = { x: 0, y: 100, w: 100, h: 20 };
    expect(containingBlock([hidden], { x: 50, y: 40 })).toBeNull();
    expect(chooseBlock([hidden], { x: 50, y: 40 })).toBeNull();
  });
});

describe('resolvePointToSelection', () => {
  test('point inside a block, left half -> before', () => {
    expect(resolvePointToSelection(blocks(), { x: 10, y: 10 }, index)).toEqual({
      nodeId: 'A',
      unitId: 'A#0',
      offset: 0,
    });
  });

  test('point inside a block, right/lower half -> after', () => {
    // "After" a TEXT unit is its full length, not 1: `offset` is unit-relative
    // and `locateSelectionPoint` clamps text offsets into [0, text.length], so
    // a hardcoded 1 would mean "one character in" and select a single letter.
    expect(resolvePointToSelection(blocks(), { x: 90, y: 15 }, index)).toEqual({
      nodeId: 'A',
      unitId: 'A#0',
      offset: 5,
    });
  });

  test('point in the gap between A and B picks the nearer block', () => {
    // Gap 20..30, midpoint 25. y=24 is nearer A -> A after; y=27 nearer B -> B before.
    expect(resolvePointToSelection(blocks(), { x: 50, y: 24 }, index)).toEqual({
      nodeId: 'A',
      unitId: 'A#0',
      offset: 5,
    });
    expect(resolvePointToSelection(blocks(), { x: 50, y: 27 }, index)).toEqual({
      nodeId: 'B',
      unitId: 'B#0',
      offset: 0,
    });
  });

  test('point before the first block clamps to document start', () => {
    expect(resolvePointToSelection(blocks(), { x: 50, y: -10 }, index)).toEqual({
      nodeId: 'A',
      unitId: 'A#0',
      offset: 0,
    });
  });

  test('point after the last block clamps to document end', () => {
    // The whole of C#0, not its first character.
    expect(resolvePointToSelection(blocks(), { x: 50, y: 200 }, index)).toEqual({
      nodeId: 'C',
      unitId: 'C#0',
      offset: 5,
    });
  });

  test('an atom block still uses offset 1 for "after"', () => {
    // Zero-text units carry no interior, so `locateSelectionPoint` reads any
    // positive offset as "after" — 1 is correct here and 0 would be "before".
    const atomUnits: SelectionUnit[] = [
      { kind: 'atom', unitId: 'D#0', nodeId: 'D', node: NODE } as SelectionUnit,
    ];
    const atomIndex = buildUnitIndex(atomUnits);
    const atomBlocks: RegisteredBlock[] = [
      { nodeId: 'D', unitIds: ['D#0'], kind: 'atom', rect: rectA },
    ];
    expect(resolvePointToSelection(atomBlocks, { x: 50, y: 200 }, atomIndex)).toEqual({
      nodeId: 'D',
      unitId: 'D#0',
      offset: 1,
    });
  });

  test('a block that renders no units resolves to null, not the document start', () => {
    // `block.unitIds[len - 1]` on an empty array yields undefined, and
    // `locateSelectionPoint` clamps `{unitId: undefined}` to unit 0 offset 0 —
    // silently answering "document start" for a block that has no answer.
    const empty: RegisteredBlock[] = [{ nodeId: 'E', unitIds: [], kind: 'text', rect: rectA }];
    expect(resolvePointToSelection(empty, { x: 50, y: 200 }, index)).toBeNull();
    expect(resolvePointToSelection(empty, { x: 50, y: -10 }, index)).toBeNull();
    expect(resolvePointToSelection(empty, { x: 90, y: 10 }, index)).toBeNull();
  });

  test('point beside a block within its vertical band localizes by x', () => {
    // x=150 is right of A but in A's y-band -> A wins the band, right half -> after.
    expect(resolvePointToSelection(blocks(), { x: 150, y: 10 }, index)).toEqual({
      nodeId: 'A',
      unitId: 'A#0',
      offset: 5,
    });
  });

  test('a measured block resolves the point to a character offset', () => {
    // One 100pt line holding 'AAAAA': each character occupies 20pt, so x=65
    // sits in the left half of the fourth character and snaps back to 3.
    const measured = blocks();
    measured[0].metrics = buildTextMetrics([{ text: 'AAAAA', x: 0, y: 0, width: 100, height: 20 }]);
    expect(resolvePointToSelection(measured, { x: 65, y: 10 }, index)).toEqual({
      nodeId: 'A',
      unitId: 'A#0',
      offset: 3,
    });
  });

  test('metrics are read relative to the block rect and its content offset', () => {
    // Block B starts at y=30 and pads its text by 4pt: the same local x/y as
    // the test above must resolve identically once both are subtracted.
    const measured = blocks();
    measured[1].metrics = buildTextMetrics([{ text: 'BBBBB', x: 0, y: 0, width: 100, height: 12 }]);
    measured[1].contentOffset = { x: 8, y: 4 };
    expect(resolvePointToSelection(measured, { x: 73, y: 40 }, index)).toEqual({
      nodeId: 'B',
      unitId: 'B#0',
      offset: 3,
    });
  });

  test('no laid-out blocks returns null', () => {
    const bare: RegisteredBlock[] = [
      { nodeId: 'A', unitIds: ['A#0'], kind: 'text' },
      { nodeId: 'B', unitIds: ['B#0'], kind: 'text' },
    ];
    expect(resolvePointToSelection(bare, { x: 10, y: 10 }, index)).toBeNull();
    expect(chooseBlock(bare, { x: 10, y: 10 })).toBeNull();
  });

  test('localizePoint reads a block metrics table directly', () => {
    const block: RegisteredBlock = {
      nodeId: 'A',
      unitIds: ['A#0'],
      kind: 'text',
      rect: rectA,
      metrics: buildTextMetrics([{ text: 'AAAAA', x: 0, y: 0, width: 100, height: 20 }]),
    };
    const point = localizePoint(block, { x: 85, y: 10 }, index);
    expect(point).not.toBeNull();
    expect(point?.unitId).toBe('A#0');
    expect(point?.offset).toBe(4);
  });

  test('an empty metrics table falls back to the coarse before/after rule', () => {
    // A block registered but not yet measured must not resolve every point to
    // offset 0: it degrades to the pre-metrics behaviour instead.
    const block: RegisteredBlock = {
      nodeId: 'A',
      unitIds: ['A#0'],
      kind: 'text',
      rect: rectA,
      metrics: buildTextMetrics([]),
    };
    expect(localizePoint(block, { x: 90, y: 15 }, index)?.offset).toBe(5);
    expect(localizePoint(block, { x: 10, y: 2 }, index)?.offset).toBe(0);
  });
});
