import assert from 'node:assert/strict';
import test from 'node:test';
import { htmlToSemanticTree } from './html-semantics.mjs';

test('preserves whitespace runs inside inline code', () => {
  assert.deepEqual(htmlToSemanticTree('<code>a  b\tc</code>'), [
    {
      type: 'element',
      tag: 'code',
      children: [{ type: 'text', value: 'a  b\tc' }],
    },
  ]);
});

test('still collapses whitespace runs in ordinary inline text', () => {
  assert.deepEqual(htmlToSemanticTree('<span>a  b\tc</span>'), [
    {
      type: 'element',
      tag: 'span',
      children: [{ type: 'text', value: 'a b c' }],
    },
  ]);
});
