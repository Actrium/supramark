import { expect, test } from 'bun:test';
import { parseRealMarkdown } from '../support/real-parser';
import { linearizeDocument } from '../../linearize';
import { buildUnitIndex } from '../../resolve';
import { buildSegmentSpans, segmentTextFromSpans } from '../../native/segmentAdapter';
import { buildTextMetrics } from '../../metrics';
import { withInlineObjectMetrics } from '../../coordinator/inlineMetrics';
import { paragraphRangeAt } from '../../coordinator/hitTest';
import { createSelectionStore } from '../../coordinator/state';
import { serializeSelectionUnits } from '../../serialize';

test('hard breaks remain native offsets and clipboard newlines', async () => {
  // Use the canonical parser rather than a simplified handwritten paragraph AST.
  const plan = linearizeDocument(await parseRealMarkdown('hello  \nworld'));
  const index = buildUnitIndex(plan.units);
  expect(segmentTextFromSpans(index, buildSegmentSpans(plan.blocks[0], index))).toBe(
    'hello\nworld'
  );
});

test('measured inline attachments separate hit regions without altering clipboard offsets', async () => {
  // A wide formula must not distribute its width among the adjacent text characters.
  const plan = linearizeDocument(await parseRealMarkdown('left $x$ right'));
  const index = buildUnitIndex(plan.units);
  const binding = plan.blocks[0];
  const atom = plan.units.find(unit => unit.kind === 'atom')!;
  const raw = buildTextMetrics([{ text: 'left \ufffc right', x: 0, y: 0, width: 190, height: 20 }]);
  const metrics = withInlineObjectMetrics(
    raw,
    binding.unitIds,
    index,
    new Map([[atom.unitId, { x: 50, y: 0, w: 80, h: 20 }]])
  );
  const block = { ...binding, kind: 'text' as const, rect: { x: 0, y: 0, w: 190, h: 20 }, metrics };
  expect(metrics?.lines[0].charXs?.[6]).toBe(130);
  expect(paragraphRangeAt(block, { x: 100, y: 10 }, index)).toBeNull();
  const range = paragraphRangeAt(block, { x: 150, y: 10 }, index)!;
  const store = createSelectionStore(() => plan.units);
  store.beginAt(range.anchor);
  store.extendTo(range.focus);
  expect(serializeSelectionUnits(store.getSnapshot().units)?.trim()).toBe('right');
  expect(withInlineObjectMetrics(raw, binding.unitIds, index, new Map())).toBeUndefined();
});
