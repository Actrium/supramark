// cjk-allow-file: the CJK strings below are data, not prose — they assert that
// the fence slice reads UTF-16 offsets, which is where a CJK-heavy document
// differs from a byte-offset one.
import { describe, expect, it } from 'bun:test';
import type { SupramarkCodeNode, SupramarkNode } from '../src/ast';
import { parse } from '../src/plugin';
import { CODE_ACTION_ICONS, CODE_ACTION_ICON_VIEWBOX } from '../src/code-action-icons';
import {
  codeBlockContent,
  fencedCode,
  fencedCodeFromSource,
  rebuildFencedCode,
} from '../src/code-block';

function codeNode(
  value: string,
  extra?: Partial<Pick<SupramarkCodeNode, 'lang' | 'meta' | 'position'>>
): SupramarkCodeNode {
  return { type: 'code', value, ...extra };
}

/** A `position` spanning `[start, end)` of the UTF-16 source. */
function span(start: number, end: number): SupramarkCodeNode['position'] {
  return {
    start: { line: 1, column: 1, utf16_offset: start },
    end: { line: 1, column: 1, utf16_offset: end },
  };
}

/** Every code node in a tree, in document order. */
function collectCodeNodes(node: SupramarkNode, out: SupramarkCodeNode[] = []): SupramarkCodeNode[] {
  if (node.type === 'code') {
    out.push(node);
  }
  const children = (node as { children?: SupramarkNode[] }).children;
  if (Array.isArray(children)) {
    for (const child of children) {
      collectCodeNodes(child, out);
    }
  }
  return out;
}

describe('rebuildFencedCode', () => {
  it('does not add a blank line when the value already ends in a newline', () => {
    // The parser keeps the line ending in `value`, so appending another would
    // invent a blank line the author never wrote.
    expect(rebuildFencedCode(codeNode('const x = 1\n', { lang: 'ts' }))).toBe(
      '```ts\nconst x = 1\n```'
    );
  });

  it('terminates the body when the value has no trailing newline', () => {
    expect(rebuildFencedCode(codeNode('const x = 1', { lang: 'ts' }))).toBe(
      '```ts\nconst x = 1\n```'
    );
  });

  it('renders an empty block as an empty fence', () => {
    expect(rebuildFencedCode(codeNode('', { lang: 'ts' }))).toBe('```ts\n```');
  });

  it('carries the info-string meta after the language', () => {
    expect(rebuildFencedCode(codeNode('a\n', { lang: 'ts', meta: 'title="x"' }))).toBe(
      '```ts title="x"\na\n```'
    );
  });

  it('omits the info string entirely when there is no language', () => {
    expect(rebuildFencedCode(codeNode('a\n'))).toBe('```\na\n```');
  });

  it('documents its own limit: a rebuilt nested fence re-parses as a different document', async () => {
    // The canonical rebuild cannot express content that itself contains ```.
    // Pinning the failure (rather than describing it in prose) is what keeps
    // the caveat in the doc comment honest.
    const source = '````md\n```ts\nconst x = 1\n```\n````\n';
    const rebuild = rebuildFencedCode(codeNode('```ts\nconst x = 1\n```\n', { lang: 'md' }));

    // The author's text is one code block…
    const fromSource = await parse(source);
    expect(fromSource.children).toHaveLength(1);
    expect(fromSource.children[0]).toMatchObject({ type: 'code', lang: 'md' });

    // …while the rebuild closes at the nested ``` and leaves a stray fence.
    const fromRebuild = await parse(rebuild);
    expect(fromRebuild.children.length).toBeGreaterThan(1);
  });
});

// The unit tests below drive `position` by hand to reach the guard's branches.
// They deliberately sit apart from the parse-driven suite that follows: this is
// exactly the shape of test that cannot notice a wrong `position` from the real
// parser, so it must not be the only coverage.
describe('fencedCodeFromSource guards', () => {
  it('accepts a slice that is this node’s own fence', () => {
    const source = '```ts title="a"\nconst x = 1;\n```\n';
    const node = codeNode('const x = 1;\n', {
      lang: 'ts',
      meta: 'title="a"',
      position: span(0, source.trimEnd().length),
    });
    expect(fencedCodeFromSource(node, source)).toBe('```ts title="a"\nconst x = 1;\n```');
  });

  it('returns null without a position', () => {
    expect(fencedCodeFromSource(codeNode('a\n', { lang: 'ts' }), '```ts\na\n```\n')).toBeNull();
  });

  it('returns null for an offset pair that cannot bound a block', () => {
    const node = codeNode('a\n', { lang: 'ts', position: span(5, 5) });
    expect(fencedCodeFromSource(node, '```ts\na\n```\n')).toBeNull();
  });

  it('returns null when the range runs past the end of the source', () => {
    const source = '```ts\na\n```\n';
    const node = codeNode('a\n', { lang: 'ts', position: span(0, source.length + 10) });
    expect(fencedCodeFromSource(node, source)).toBeNull();
  });

  it('returns null when the slice does not contain the node value', () => {
    // A host that passes a markdown string unrelated to its AST must not be
    // handed a plausible-looking piece of the wrong document.
    const node = codeNode('const x = 1\n', { lang: 'ts', position: span(0, 14) });
    expect(fencedCodeFromSource(node, 'totally different document\n')).toBeNull();
  });

  it('returns null when the slice is a fence of a different language', () => {
    // The container case, reproduced by hand: the offsets point at a js block
    // while the node says ts.
    const source = '```js\nconst x = 1\n```\n';
    const node = codeNode('const x = 1\n', { lang: 'ts', position: span(0, 21) });
    expect(fencedCodeFromSource(node, source)).toBeNull();
  });

  it('returns null when the slice has no closing fence', () => {
    const source = '```ts\nconst x = 1\n';
    const node = codeNode('const x = 1\n', { lang: 'ts', position: span(0, source.length) });
    expect(fencedCodeFromSource(node, source)).toBeNull();
  });

  it('returns null when the closing line carries an info string', () => {
    const source = '```ts\nconst x = 1\n```oops\n';
    const node = codeNode('const x = 1\n', {
      lang: 'ts',
      position: span(0, source.trimEnd().length),
    });
    expect(fencedCodeFromSource(node, source)).toBeNull();
  });

  it('returns null when the closing fence is shorter than the opening one', () => {
    const source = '````ts\nconst x = 1\n```\n';
    const node = codeNode('const x = 1\n', {
      lang: 'ts',
      position: span(0, source.trimEnd().length),
    });
    expect(fencedCodeFromSource(node, source)).toBeNull();
  });

  it('normalises info-string whitespace before comparing', () => {
    // The author may space the fence info freely; the parser collapses it.
    const source = '```   ts   title="a"   \nconst x = 1\n```\n';
    const node = codeNode('const x = 1\n', {
      lang: 'ts',
      meta: 'title="a"',
      position: span(0, source.trimEnd().length),
    });
    expect(fencedCodeFromSource(node, source)).toBe('```   ts   title="a"   \nconst x = 1\n```');
  });
});

// The suite that matters: every case below runs the real Rust parser, so a
// change in what `position` means fails here instead of silently handing a host
// the wrong text.
describe('fencedCode against a real parse', () => {
  it("returns the author's fence verbatim for a top-level block", async () => {
    const source = 'intro\n\n```ts title="a"\nconst x = 1;\n```\n\noutro\n';
    const [node] = collectCodeNodes(await parse(source));
    expect(fencedCode(node, source)).toBe('```ts title="a"\nconst x = 1;\n```');
  });

  it('keeps tilde markers and the info-string spacing the author wrote', async () => {
    const source = '~~~  js  extra  \nlet a = 1;\n~~~\n';
    const [node] = collectCodeNodes(await parse(source));
    expect(fencedCode(node, source)).toBe('~~~  js  extra  \nlet a = 1;\n~~~');
  });

  it('keeps an indented fence as the author wrote it', async () => {
    // The position starts at the markers, so the slice drops the indent the
    // parser already stripped from `value`.
    const source = 'lead in\n\n  ```ts\n  const x = 1;\n  ```\n';
    const [node] = collectCodeNodes(await parse(source));
    expect(fencedCode(node, source)).toBe('```ts\n  const x = 1;\n  ```');
  });

  it('slices an indented fence whose body spans several lines', async () => {
    // CommonMark strips the opener's indent from every body line, so the body
    // comparison must put it back; otherwise only single-line indented fences
    // would ever slice.
    const source = 'lead in\n\n  ```ts\n  a\n  b\n  ```\n';
    const [node] = collectCodeNodes(await parse(source));
    expect(fencedCode(node, source)).toBe('```ts\n  a\n  b\n  ```');
  });

  it('preserves a four-backtick fence wrapping content that contains ```', async () => {
    const source = '````md\n```ts\nconst x = 1\n```\n````\n';
    const [node] = collectCodeNodes(await parse(source));
    expect(fencedCode(node, source)).toBe('````md\n```ts\nconst x = 1\n```\n````');
  });

  it('reads UTF-16 offsets, so a CJK prefix does not shift the slice', async () => {
    const source = '中文标题\n\n```js\nlet a = 1;\n```\n';
    const [node] = collectCodeNodes(await parse(source));
    expect(fencedCode(node, source)).toBe('```js\nlet a = 1;\n```');
  });

  it('never hands a host another block when the block sits inside a ::: container', async () => {
    // expandOpaqueContainers re-parses a container body on its own and grafts
    // the subtree back without rebasing `position`, so an inner block reports
    // offsets relative to the body. Slicing the document at those offsets lands
    // on the earlier js block — the guard must refuse it and rebuild instead.
    const source = '```js\nconst x = 1\n```\n\n:::note\n```ts\nconst x = 1\n```\n:::\n';
    const nodes = collectCodeNodes(await parse(source));
    expect(nodes).toHaveLength(2);
    expect(nodes[1].lang).toBe('ts');
    expect(fencedCode(nodes[1], source)).toBe('```ts\nconst x = 1\n```');
  });

  it('refuses a same-language window whose length matches by construction', async () => {
    // The window onto the first block is exactly as long as the second block's
    // fence: a four-backtick opener over `AAA\n` against a three-backtick opener
    // over `AAA\nX\n`. The extra body line pays for the shorter marker, so a
    // substring check accepts the wrong window — equality does not.
    const source = '```ts\nAAA\nX\n```\n\n:::note\n````ts\nAAA\n````\n:::\n';
    const nodes = collectCodeNodes(await parse(source));
    expect(nodes).toHaveLength(2);
    expect(nodes[1].value).toBe('AAA\n');
    expect(fencedCode(nodes[1], source)).toBe('```ts\nAAA\n```');
  });

  it('closes an unterminated fence that cannot be sliced', async () => {
    // The position of a still-open fence stops before the line ending its value
    // carries, so streaming input takes the rebuild path and the host gets a
    // closed fence rather than a fragment.
    const source = 'intro\n\n```ts\nconst x = 1;\ncons';
    const [node] = collectCodeNodes(await parse(source));
    expect(fencedCode(node, source)).toBe('```ts\nconst x = 1;\ncons\n```');
  });
});

describe('fencedCode', () => {
  it('falls back to the canonical fence with no source', () => {
    expect(fencedCode(codeNode('a\n', { lang: 'ts' }))).toBe('```ts\na\n```');
  });

  it('falls back to the canonical fence when the slice cannot be trusted', () => {
    const source = 'unrelated\n';
    const node = codeNode('a\n', { lang: 'ts', position: span(0, 9) });
    expect(fencedCode(node, source)).toBe('```ts\na\n```');
  });
});

describe('codeBlockContent', () => {
  it('carries the code, the fence and the node', () => {
    const node = codeNode('a\n', { lang: 'ts' });
    const content = codeBlockContent(node, '```ts\na\n```\n');

    expect(content.code).toBe('a\n');
    expect(content.fenced).toBe('```ts\na\n```');
    expect(content.lang).toBe('ts');
    expect(content.node).toBe(node);
  });

  it('omits absent optional fields rather than setting them to undefined', () => {
    const content = codeBlockContent(codeNode('a\n'));
    expect('lang' in content).toBe(false);
    expect('meta' in content).toBe(false);
    // Survives a round trip, so a host can forward it over the wire.
    expect(JSON.parse(JSON.stringify(content))).toEqual(content);
  });
});

describe('CODE_ACTION_ICONS', () => {
  it('draws every action in one shared coordinate system', () => {
    expect(CODE_ACTION_ICON_VIEWBOX).toBe('0 0 24 24');
  });

  it('provides a non-empty path for each action, copy state included', () => {
    expect(Object.keys(CODE_ACTION_ICONS).sort()).toEqual(['copy', 'fullscreen']);
    for (const icon of Object.values(CODE_ACTION_ICONS)) {
      expect(icon.path.startsWith('M')).toBe(true);
      expect(['outline', 'solid']).toContain(icon.paint);
    }
  });
});
