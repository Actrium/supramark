import { describe, expect, test } from 'bun:test';
import type { SupramarkTextNode } from '@supramark/core';
import type { SelectionRange, SelectionTextUnit } from '../../model';
import { buildCopyRequest } from '../../coordinator/actions';

const NODE = { type: 'text', value: '' } as SupramarkTextNode;
const unit = (unitId: string, text: string, markdown?: string): SelectionTextUnit => ({
  kind: 'text',
  unitId,
  nodeId: 'p',
  text,
  node: NODE,
  ...(markdown ? { payload: { markdown } } : {}),
});

const range: SelectionRange = {
  anchor: { nodeId: 'p', unitId: 'p#0', offset: 0 },
  focus: { nodeId: 'p', unitId: 'p#1', offset: 5 },
};
const units = [unit('p#0', 'Hello '), unit('p#1', 'world', '**world**')];

describe('buildCopyRequest', () => {
  test('serializes in the item format and always carries plain text', () => {
    const request = buildCopyRequest({ id: 'copy-markdown', title: 'x', format: 'markdown' }, units, range);
    expect(request).toEqual({
      id: 'copy-markdown',
      format: 'markdown',
      payload: 'Hello **world**',
      text: 'Hello world',
      range,
    });
  });

  test('an item with no format still yields the selected text', () => {
    // Host actions such as "Ask AI" or "Quote" get the text without having to
    // re-serialize the selection themselves.
    const request = buildCopyRequest({ id: 'ask', title: 'Ask' }, units, range);
    expect(request.format).toBe('plainText');
    expect(request.text).toBe('Hello world');
  });

  test('an empty selection yields an empty string rather than undefined text', () => {
    const request = buildCopyRequest({ id: 'copy', title: 'Copy' }, [], range);
    expect(request.text).toBe('');
  });

  test('the range is passed through untouched', () => {
    expect(buildCopyRequest({ id: 'copy', title: 'Copy' }, units, range).range).toBe(range);
  });
});
