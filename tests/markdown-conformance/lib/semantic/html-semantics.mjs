import { parseFragment } from 'parse5';

const BLOCK_CONTAINERS = new Set([
  '#document-fragment',
  'blockquote',
  'body',
  'div',
  'li',
  'ol',
  'section',
  'table',
  'tbody',
  'thead',
  'tfoot',
  'tr',
  'ul',
]);

const TAG_TO_SEMANTIC_TYPE = {
  a: 'link',
  blockquote: 'blockquote',
  br: 'break',
  code: 'inline_code',
  em: 'emphasis',
  h1: 'heading',
  h2: 'heading',
  h3: 'heading',
  h4: 'heading',
  h5: 'heading',
  h6: 'heading',
  hr: 'thematic_break',
  img: 'image',
  li: 'list_item',
  ol: 'list',
  p: 'paragraph',
  pre: 'code',
  strong: 'strong',
  ul: 'list',
};

export function htmlToSemanticTree(html) {
  const fragment = parseFragment(html);
  return canonicalChildren(fragment, '#document-fragment');
}

export function collectSemanticTypes(html) {
  return collectSemanticTypesFromTree(htmlToSemanticTree(html));
}

export function collectSemanticTypesFromTree(tree) {
  const result = [];
  const seen = new Set();

  function add(value) {
    if (!seen.has(value)) {
      seen.add(value);
      result.push(value);
    }
  }

  function walk(node, parentTag) {
    if (Array.isArray(node)) {
      for (const child of node) walk(child, parentTag);
      return;
    }
    if (node.type === 'text') {
      if (parentTag !== 'pre' && parentTag !== 'code' && node.value.length > 0) add('text');
      return;
    }
    if (node.type === 'comment' || node.type === 'doctype') {
      add('raw');
      return;
    }
    if (node.type !== 'element') {
      add('raw');
      return;
    }

    if (!(node.tag === 'code' && parentTag === 'pre')) {
      add(TAG_TO_SEMANTIC_TYPE[node.tag] ?? 'raw');
    }
    for (const child of node.children ?? []) walk(child, node.tag);
  }

  walk(tree, '#document-fragment');
  return result;
}

export function findFirstDifference(expected, actual, path = '$') {
  if (typeof expected !== typeof actual) {
    return { path, expected, actual, reason: 'type' };
  }
  if (expected === null || actual === null || typeof expected !== 'object') {
    return expected === actual ? null : { path, expected, actual, reason: 'value' };
  }
  if (Array.isArray(expected) || Array.isArray(actual)) {
    if (!Array.isArray(expected) || !Array.isArray(actual)) {
      return { path, expected, actual, reason: 'array-type' };
    }
    if (expected.length !== actual.length) {
      return { path, expected, actual, reason: 'array-length' };
    }
    for (let index = 0; index < expected.length; index += 1) {
      const difference = findFirstDifference(expected[index], actual[index], `${path}[${index}]`);
      if (difference) return difference;
    }
    return null;
  }

  const expectedKeys = Object.keys(expected).sort();
  const actualKeys = Object.keys(actual).sort();
  if (JSON.stringify(expectedKeys) !== JSON.stringify(actualKeys)) {
    return { path, expected: expectedKeys, actual: actualKeys, reason: 'object-keys' };
  }
  for (const key of expectedKeys) {
    const difference = findFirstDifference(expected[key], actual[key], `${path}.${key}`);
    if (difference) return difference;
  }
  return null;
}

function canonicalChildren(node, parentTag) {
  const result = [];
  for (const child of node.childNodes ?? []) {
    const canonical = canonicalNode(child, parentTag);
    if (canonical === null) continue;
    if (Array.isArray(canonical)) result.push(...canonical);
    else result.push(canonical);
  }
  return mergeAdjacentText(result);
}

function canonicalNode(node, parentTag) {
  if (node.nodeName === '#text') {
    const value = normalizeText(node.value, parentTag);
    return value.length === 0 ? null : { type: 'text', value };
  }
  if (node.nodeName === '#comment') {
    return { type: 'comment', value: node.data };
  }
  if (node.nodeName === '#documentType') {
    return { type: 'doctype', name: node.name };
  }
  if (!node.tagName) {
    return canonicalChildren(node, node.nodeName);
  }

  const tag = node.tagName.toLowerCase();
  const attributes = Object.fromEntries(
    [...(node.attrs ?? [])]
      .map(attribute => [attribute.name.toLowerCase(), attribute.value])
      .sort(([left], [right]) => left.localeCompare(right))
  );
  const result = { type: 'element', tag };
  if (Object.keys(attributes).length > 0) result.attributes = attributes;
  const children = canonicalChildren(node, tag);
  if (children.length > 0) result.children = children;
  return result;
}

function normalizeText(value, parentTag) {
  const normalized = value.replace(/\r\n?/g, '\n');
  if (parentTag === 'pre' || parentTag === 'code') return normalized;
  if (BLOCK_CONTAINERS.has(parentTag) && /^\s+$/.test(normalized)) return '';
  return normalized.replace(/[\t\n\f\r ]+/g, ' ');
}

function mergeAdjacentText(nodes) {
  const result = [];
  for (const node of nodes) {
    const previous = result.at(-1);
    if (node.type === 'text' && previous?.type === 'text') previous.value += node.value;
    else result.push(node);
  }
  return result;
}

function isCommonMarkWrapper(tag, parentTag) {
  if (tag === 'p') return true;
  if (parentTag === 'pre' && tag === 'code') return true;
  return false;
}
