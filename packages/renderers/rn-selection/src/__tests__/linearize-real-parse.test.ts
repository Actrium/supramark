import { describe, expect, test } from 'bun:test';
import { linearizeForSelection } from '../linearize';
import { parseRealMarkdown } from './support/real-parser';
import { buildUnitIndex, detectDuplicateUnitIds, resolveSelectionRange } from '../resolve';
import { serializeSelectionUnits } from '../serialize';
import type { SelectionUnit } from '../model';

/**
 * Regression coverage against the REAL canonical parser.
 *
 * The canonical parser reports identical source positions for a paragraph and
 * its only text child, so position-based identities made the paragraph's break
 * unit and the text unit share one unitId — selecting `hel` resolved to `\n`.
 * Hand-built ASTs never reproduced it because they carry no positions, so these
 * cases parse real Markdown and assert id uniqueness end to end.
 */

const CASES: Array<[string, string]> = [
  ['plain paragraph', 'hello'],
  ['two paragraphs', 'one\n\ntwo'],
  ['repeated text', 'same\n\nsame'],
  ['inline mix', 'a **b** _c_ `d` [e](u)'],
  ['heading', '# Title'],
  ['tight list', '- a\n- b'],
  ['ordered and task list', '1. a\n2. b\n\n- [x] c'],
  ['loose list', '- a\n\n  second\n- b'],
  ['blockquote', '> q\n>\n> r'],
  ['code fence', '```ts\nconst x = 1;\n```'],
  ['table', '| a | b |\n| --- | --- |\n| 1 | 2 |'],
  ['footnote', 'a[^1]\n\n[^1]: note'],
  ['thematic break', 'a\n\n---\n\nb'],
  ['inline image', 'a ![alt](https://example.com/i.png) b'],
  ['inline and block math', 'a $x$ b\n\n$$\ny\n$$'],
  ['definition list', 'Term\n: Desc'],
];

function describeDuplicates(units: readonly SelectionUnit[], duplicates: ReturnType<typeof detectDuplicateUnitIds>): string {
  return duplicates
    .map(duplicate => {
      const first = units[duplicate.firstIndex];
      const dup = units[duplicate.duplicateIndex];
      return `${duplicate.unitId}: ${first.kind}(${JSON.stringify((first as { text?: string }).text ?? '')}) vs ${dup.kind}(${JSON.stringify((dup as { text?: string }).text ?? '')})`;
    })
    .join('; ');
}

describe('real parser output yields unique selection ids', () => {
  for (const [name, markdown] of CASES) {
    test(name, async () => {
      const units = linearizeForSelection(await parseRealMarkdown(markdown));
      expect(units.length).toBeGreaterThan(0);
      const duplicates = detectDuplicateUnitIds(units);
      expect(describeDuplicates(units, duplicates)).toBe('');
    });
  }

  test('expanded opaque container', async () => {
    const units = linearizeForSelection(await parseRealMarkdown(':::note\ninner text\n:::'));
    const duplicates = detectDuplicateUnitIds(units);
    expect(describeDuplicates(units, duplicates)).toBe('');
  });

  test('a paragraph and its only text child get different identities', async () => {
    const ast = await parseRealMarkdown('hello');
    const units = linearizeForSelection(ast);
    const textUnit = units.find(unit => unit.kind === 'text' && unit.text === 'hello');
    const breakUnit = units.find(unit => unit.kind === 'break');
    expect(textUnit).toBeDefined();
    expect(breakUnit).toBeDefined();
    expect(textUnit!.nodeId).not.toBe(breakUnit!.nodeId);
    expect(textUnit!.unitId).not.toBe(breakUnit!.unitId);
  });
});

describe('selecting into real parser output', () => {
  test('hello: offsets 0..3 resolve to and copy as "hel"', async () => {
    const ast = await parseRealMarkdown('hello');
    const units = linearizeForSelection(ast);
    const textUnit = units.find(unit => unit.kind === 'text' && unit.text === 'hello');
    expect(textUnit).toBeDefined();
    const index = buildUnitIndex(units);
    const resolved = resolveSelectionRange(
      units,
      {
        anchor: { nodeId: textUnit!.nodeId, offset: 0 },
        focus: { nodeId: textUnit!.nodeId, offset: 3 },
      },
      index
    );
    expect(serializeSelectionUnits(resolved, 'plainText')).toBe('hel');
  });

  test('inline mix: plain text drops markers, Markdown reconstructs them', async () => {
    const ast = await parseRealMarkdown('a **b** c');
    const units = linearizeForSelection(ast);
    const textUnits = units.filter(unit => unit.kind === 'text' && unit.text !== '');
    const first = textUnits[0];
    const last = textUnits[textUnits.length - 1];
    expect(first.text).toBe('a ');
    expect(last.text).toBe(' c');
    const resolved = resolveSelectionRange(units, {
      anchor: { nodeId: first.nodeId, unitId: first.unitId, offset: 0 },
      focus: { nodeId: last.nodeId, unitId: last.unitId, offset: last.text.length },
    });
    expect(serializeSelectionUnits(resolved, 'plainText')).toBe('a b c');
    expect(serializeSelectionUnits(resolved, 'markdown')).toBe('a **b** c');
  });

  test('two paragraphs: a cross-block range serializes with a break between', async () => {
    const ast = await parseRealMarkdown('one\n\ntwo');
    const units = linearizeForSelection(ast);
    const firstText = units.find(unit => unit.kind === 'text' && unit.text === 'one');
    const secondText = units.find(unit => unit.kind === 'text' && unit.text === 'two');
    expect(firstText).toBeDefined();
    expect(secondText).toBeDefined();
    const resolved = resolveSelectionRange(units, {
      anchor: { nodeId: firstText!.nodeId, offset: 0 },
      focus: { nodeId: secondText!.nodeId, offset: 3 },
    });
    expect(serializeSelectionUnits(resolved, 'plainText')).toBe('one\ntwo');
  });
});