import React from 'react';
import { afterEach, describe, expect, test } from 'bun:test';
import { Window } from 'happy-dom';
import { createRoot, type Root } from 'react-dom/client';
import type { SupramarkRootNode } from '@supramark/core';
import { Supramark } from '../src/Supramark';

type TestAct = (callback: () => void | Promise<void>) => Promise<void>;
const act = (React as typeof React & { act: TestAct }).act;
const browser = new Window();
Object.assign(globalThis, {
  window: browser,
  document: browser.document,
  navigator: browser.navigator,
  HTMLElement: browser.HTMLElement,
  Node: browser.Node,
});
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT =
  true;

let root: Root | null = null;
type TestContainer = ReturnType<typeof browser.document.createElement>;

afterEach(async () => {
  if (root) {
    await act(async () => root?.unmount());
    root = null;
  }
  browser.document.body.replaceChildren();
});

function createContainer(): TestContainer {
  const container = browser.document.createElement('div');
  browser.document.body.appendChild(container);
  root = createRoot(container as unknown as HTMLDivElement);
  return container;
}

async function renderAst(
  ast: SupramarkRootNode,
  options?: { allowDangerousHtml?: boolean; gfmTagfilter?: boolean; flattenNestedStrong?: boolean }
): Promise<TestContainer> {
  const container = createContainer();
  const opts = options
    ? {
        options: {
          ...(options.allowDangerousHtml ? { allowDangerousHtml: true } : {}),
          ...(options.gfmTagfilter ? { gfmTagfilter: true } : {}),
          ...(options.flattenNestedStrong ? { flattenNestedStrong: true } : {}),
        },
      }
    : undefined;
  await act(async () => {
    root?.render(<Supramark markdown="" ast={ast} config={opts} />);
  });
  return container;
}

function makeRoot(children: SupramarkRootNode['children']): SupramarkRootNode {
  return { type: 'root', ast_version: 2, diagnostics: [], children };
}

function paragraph(text: string) {
  return { type: 'paragraph', children: [{ type: 'text', value: text }] } as const;
}

function text(value: string) {
  return { type: 'text', value } as const;
}

function heading(depth: number, content: string) {
  return { type: 'heading', depth, children: [text(content)] } as const;
}

function unorderedList(children: SupramarkRootNode['children']) {
  return { type: 'list', ordered: false, children } as const;
}

function listItem(children: SupramarkRootNode['children']) {
  return { type: 'list_item', children } as const;
}

function raw(value: string, block = true) {
  return { type: 'raw', format: 'html', value, block } as const;
}

function tableCell(content: string, header: boolean, align?: 'left' | 'right' | 'center') {
  return {
    type: 'table_cell',
    header,
    ...(align ? { align } : {}),
    children: [text(content)],
  } as const;
}

function tableRow(cells: ReturnType<typeof tableCell>[]) {
  return { type: 'table_row', children: cells } as const;
}

function table(rows: ReturnType<typeof tableRow>[], align?: ('left' | 'right' | 'center')[]) {
  return { type: 'table', align: align ?? [], children: rows } as const;
}

describe('CommonMark block rendering', () => {
  test('renders a blockquote with its paragraph children', async () => {
    const ast = makeRoot([{ type: 'blockquote', children: [paragraph('quote')] }]);
    const container = await renderAst(ast);
    expect(container.innerHTML).toContain('<blockquote>');
    expect(container.innerHTML).toContain('quote');
    expect(container.innerHTML).toContain('</blockquote>');
  });

  test('renders a thematic break as <hr />', async () => {
    const ast = makeRoot([{ type: 'thematic_break' }]);
    const container = await renderAst(ast);
    expect(container.innerHTML).toMatch(/<hr\s*\/?>/i);
  });

  test('renders ordered list start attribute when start is not 1', async () => {
    const ast = makeRoot([
      {
        type: 'list',
        ordered: true,
        start: 3,
        children: [{ type: 'list_item', children: [paragraph('a')] }],
      },
    ]);
    const container = await renderAst(ast);
    expect(container.innerHTML).toMatch(/<ol[^>]*\sstart="3"[^>]*>/i);
  });

  test('omits start attribute on ordered list when start is 1', async () => {
    const ast = makeRoot([
      {
        type: 'list',
        ordered: true,
        start: 1,
        children: [{ type: 'list_item', children: [paragraph('a')] }],
      },
    ]);
    const container = await renderAst(ast);
    const olMatch = container.innerHTML.match(/<ol[^>]*>/i);
    expect(olMatch).not.toBeNull();
    expect(olMatch![0]).not.toMatch(/\sstart=/i);
  });

  test('emits language-xxx class on fenced code with a lang', async () => {
    const ast = makeRoot([{ type: 'code', lang: 'ruby', value: 'x = 1\n' }]);
    const container = await renderAst(ast);
    expect(container.innerHTML).toContain('class="language-ruby"');
  });

  test('omits language class on fenced code without a lang', async () => {
    const ast = makeRoot([{ type: 'code', value: 'plain\n' }]);
    const container = await renderAst(ast);
    const codeMatch = container.innerHTML.match(/<code[^>]*>/i);
    expect(codeMatch).not.toBeNull();
    expect(codeMatch![0]).not.toMatch(/language-/i);
  });
});

describe('cmark-gfm nested-strong flattening (opt-in)', () => {
  // cmark-gfm 0.29's HTML renderer suppresses the <strong> wrapper when a
  // strong node's parent is itself strong (html.c, CMARK_NODE_STRONG),
  // collapsing the nested `<strong><strong>foo</strong></strong>` that the
  // delimiter run algorithm produces into a single flat `<strong>foo</strong>`.
  // CommonMark 0.31 does NOT do this — its spec keeps the nesting — so the
  // flattening is opt-in via `options.flattenNestedStrong` (the cmark-gfm
  // conformance harness enables it; the default stays nested). Only STRONG is
  // suppressed; nested <em> always stays nested. See issue #144.

  function strong(children: SupramarkRootNode['children']) {
    return { type: 'strong', children } as const;
  }
  function emph(children: SupramarkRootNode['children']) {
    return { type: 'emphasis', children } as const;
  }

  test('default keeps nested strong-strong (CommonMark 0.31 behavior)', async () => {
    // `****foo****` -> <p><strong><strong>foo</strong></strong></p> by default
    const ast = makeRoot([
      { type: 'paragraph', children: [strong([strong([text('foo')])])] },
    ]);
    const container = await renderAst(ast);
    expect(container.innerHTML).toContain('<strong><strong>foo</strong></strong>');
  });

  test('flattens nested strong-strong to a single <strong> when opted in', async () => {
    // `****foo****` -> <p><strong>foo</strong></p>
    const ast = makeRoot([
      { type: 'paragraph', children: [strong([strong([text('foo')])])] },
    ]);
    const container = await renderAst(ast, { flattenNestedStrong: true });
    expect(container.innerHTML).toContain('<strong>foo</strong>');
    expect(container.innerHTML).not.toContain('<strong><strong>');
  });

  test('flattens strong child of strong but keeps intervening text (opted in)', async () => {
    // `__foo, __bar__, baz__` -> <p><strong>foo, bar, baz</strong></p>
    const ast = makeRoot([
      {
        type: 'paragraph',
        children: [
          strong([text('foo, '), strong([text('bar')]), text(', baz')]),
        ],
      },
    ]);
    const container = await renderAst(ast, { flattenNestedStrong: true });
    expect(container.innerHTML).toContain('<strong>foo, bar, baz</strong>');
    expect(container.innerHTML).not.toContain('<strong><strong>');
  });

  test('preserves nested emphasis (only strong is suppressed, opted in)', async () => {
    // `*(*foo*)*` -> <p><em>(<em>foo</em>)</em></p>
    const ast = makeRoot([
      {
        type: 'paragraph',
        children: [
          emph([text('('), emph([text('foo')]), text(')')]),
        ],
      },
    ]);
    const container = await renderAst(ast, { flattenNestedStrong: true });
    expect(container.innerHTML).toContain('<em>(<em>foo</em>)</em>');
  });

  test('keeps inner strong whose parent is emphasis, not strong (opted in)', async () => {
    // `**_**b**_**` -> <p><strong><em><strong>b</strong></em></strong></p>
    const ast = makeRoot([
      {
        type: 'paragraph',
        children: [strong([emph([strong([text('b')])])])],
      },
    ]);
    const container = await renderAst(ast, { flattenNestedStrong: true });
    expect(container.innerHTML).toContain('<strong><em><strong>b</strong></em></strong>');
  });

  test('flattens strong-strong inside a raw-HTML paragraph (serialize path, opted in)', async () => {
    // `****foo**** <b>x</b>` -> the paragraph contains raw HTML, so it goes
    // through serializeInlineNode; the inner strong must still suppress.
    const ast = makeRoot([
      {
        type: 'paragraph',
        children: [strong([strong([text('foo')])]), raw('<b>x</b>', false), text(' ')],
      },
    ]);
    const container = await renderAst(ast, { allowDangerousHtml: true, flattenNestedStrong: true });
    expect(container.innerHTML).toContain('<strong>foo</strong>');
    expect(container.innerHTML).not.toContain('<strong><strong>');
  });
});

describe('Raw HTML active-formatting-element reconstruction across blocks', () => {
  // cmark-gfm spec-0652: a paragraph with unclosed inline formatting tags
  // (`<strong> … <em>`) followed by a raw block. cmark emits the raw tokens
  // verbatim, and the browser's tree-construction reconstructs the open
  // strong/em across `</p>` into the following block. The renderer folds the
  // paragraph and following serializable raw siblings into one RawHtml
  // fragment so a single parse owns the reconstruction (verified end-to-end by
  // the real-Chromium conformance gate). In a non-reconstructing DOM
  // (happy-dom) the observable guard is that the paragraph-level path no
  // longer leaves a trailing reconstructed `<strong><em>…</em></strong>`
  // artifact between the paragraph and the following block.
  function inlineRaw(value: string) {
    return { type: 'raw', format: 'html', value, block: false } as const;
  }
  function blockRaw(value: string) {
    return { type: 'raw', format: 'html', value, block: true } as const;
  }

  test('folds the paragraph and following raw block into one fragment', async () => {
    const ast = makeRoot([
      {
        type: 'paragraph',
        children: [
          inlineRaw('<strong>'),
          text(' '),
          inlineRaw('<title>'),
          text(' '),
          inlineRaw('<style>'),
          text(' '),
          inlineRaw('<em>'),
        ],
      },
      blockRaw('<blockquote>\n  &lt;xmp> is disallowed.\n</blockquote>\n'),
    ]);
    const container = await renderAst(ast, { allowDangerousHtml: true, gfmTagfilter: true });
    const html = container.innerHTML;
    // The paragraph-level reconstruction artifact (a trailing empty
    // <strong><em>…</em></strong> between </p> and the blockquote) is gone.
    expect(html).not.toContain('<strong><em>');
    // The blockquote is still rendered as a sibling block.
    expect(html).toContain('<blockquote>');
  });
});

describe('CommonMark hard line breaks', () => {
  test('emits a newline after <br /> between text in a paragraph', async () => {
    // commonmark-0.31.2-0633: `foo  \nbaz` -> <p>foo<br />\nbaz</p>
    const ast = makeRoot([
      {
        type: 'paragraph',
        children: [text('foo'), { type: 'break' }, text('baz')],
      },
    ]);
    const container = await renderAst(ast);
    expect(container.innerHTML).toMatch(/foo<br\s*\/?>\nbaz/);
  });

  test('emits a newline after <br /> inside emphasis', async () => {
    // commonmark-0.31.2-0638: `*foo  \nbar*` -> <p><em>foo<br />\nbar</em></p>
    const ast = makeRoot([
      {
        type: 'paragraph',
        children: [
          {
            type: 'emphasis',
            children: [text('foo'), { type: 'break' }, text('bar')],
          },
        ],
      },
    ]);
    const container = await renderAst(ast);
    expect(container.innerHTML).toMatch(/foo<br\s*\/?>\nbar/);
  });
});

describe('CommonMark list item block/inline boundaries', () => {
  test('separates inline text from a nested list with a newline', async () => {
    // commonmark-0.31.2-0323: `- a\n  - b` -> <li>a\n<ul>...
    const ast = makeRoot([
      unorderedList([
        listItem([text('a'), unorderedList([listItem([text('b')])])]),
      ]),
    ]);
    const container = await renderAst(ast);
    expect(container.innerHTML).toMatch(/<li>a\n<ul/);
  });

  test('separates a nested block from following inline text with a newline', async () => {
    // commonmark-0.31.2-0300 (second item): `<h2>Bar</h2>\nbaz`
    const ast = makeRoot([
      unorderedList([listItem([heading(2, 'Bar'), text('baz')])]),
    ]);
    const container = await renderAst(ast);
    expect(container.innerHTML).toMatch(/<h2[^>]*>Bar<\/h2>\nbaz/);
  });

  test('does not insert a newline between adjacent inline nodes in a tight item', async () => {
    const ast = makeRoot([
      unorderedList([
        listItem([
          text('a'),
          { type: 'emphasis', children: [text('b')] },
        ]),
      ]),
    ]);
    const container = await renderAst(ast);
    expect(container.innerHTML).toMatch(/<li>a<em>b<\/em><\/li>/);
  });

  test('does not insert a newline in a tight item with only inline text', async () => {
    const ast = makeRoot([unorderedList([listItem([text('foo')])])]);
    const container = await renderAst(ast);
    expect(container.innerHTML).toMatch(/<li>foo<\/li>/);
  });
});

describe('CommonMark raw HTML', () => {
  test('renders a balanced block raw element via a same-named host', async () => {
    // commonmark-0.31.2-0185: `<div>\nbar\n</div>` stays as <div>\nbar\n</div>
    const ast = makeRoot([raw('<div>\nbar\n</div>')]);
    const container = await renderAst(ast, { allowDangerousHtml: true });
    expect(container.innerHTML).toContain('<div>');
    expect(container.innerHTML).toContain('bar');
    expect(container.innerHTML).toContain('</div>');
    // no extra wrapper element around the raw div
    expect(container.firstChild?.nodeName).toBe('DIV');
  });

  test('preserves the literal inner text of a raw block (no markdown processing)', async () => {
    // commonmark-0.31.2-0189: `<div>\n*Emphasized* text.\n</div>` — `*x*` stays literal
    const ast = makeRoot([raw('<div>\n*Emphasized* text.\n</div>')]);
    const container = await renderAst(ast, { allowDangerousHtml: true });
    expect(container.innerHTML).toContain('*Emphasized*');
    expect(container.innerHTML).not.toContain('<em>');
  });

  test('renders a self-closing inline custom element in a paragraph', async () => {
    // commonmark-0.31.2-0617: `Foo <responsive-image src="foo.jpg" />`
    const ast = makeRoot([
      {
        type: 'paragraph',
        children: [text('Foo '), raw('<responsive-image src="foo.jpg" />', false)],
      },
    ]);
    const container = await renderAst(ast, { allowDangerousHtml: true });
    expect(container.innerHTML).toContain('Foo');
    expect(container.innerHTML).toMatch(/<responsive-image[^>]*src="foo.jpg"/);
  });

  test('renders a raw <textarea> with its literal content', async () => {
    // commonmark-0.31.2-0171: textarea content is raw text, not markdown
    const ast = makeRoot([raw('<textarea>\n\n*foo*\n\n</textarea>')]);
    const container = await renderAst(ast, { allowDangerousHtml: true });
    expect(container.innerHTML).toMatch(/<textarea[^>]*>/);
    expect(container.innerHTML).toContain('*foo*');
    expect(container.innerHTML).toContain('</textarea>');
  });

  test('renders a bare open-tag raw fragment as an empty same-named host', async () => {
    // commonmark-0.31.2-0152 splits `<DIV CLASS="foo">` and `</DIV>` into two
    // raw nodes. A bare open-tag fragment with no close tag and no following
    // sibling renders as a same-named host carrying the attributes; the HTML
    // parser auto-closes it. (With a matching close-tag sibling, mergeRawNodes
    // wraps the intervening children — covered by the conformance suite.)
    const ast = makeRoot([raw('<DIV CLASS="foo">')]);
    const container = await renderAst(ast, { allowDangerousHtml: true });
    expect(container.innerHTML).toMatch(/<div[^>]*>/i);
    expect(container.innerHTML).toContain('foo');
  });

  test('renders a comment raw node via DOM injection', async () => {
    // `<!-- foo -->` is a raw fragment React cannot emit as an element, so
    // the renderer parses the value through a <template> and splices the
    // resulting comment node in place.
    const ast = makeRoot([raw('<!-- foo -->')]);
    const container = await renderAst(ast, { allowDangerousHtml: true });
    expect(container.innerHTML).toContain('<!-- foo -->');
  });

  test('drops raw HTML when allowDangerousHtml is not enabled (default off)', async () => {
    // Raw HTML is opt-in. Without the flag, raw nodes are dropped so an
    // upgrade never silently enables script execution from untrusted
    // markdown — matching the pre-raw-HTML behaviour. The raw `<img onerror>`
    // must not reach the DOM; surrounding content still renders.
    const ast = makeRoot([
      raw('<div><img src="x" onerror="alert(1)"></div>'),
      paragraph('after'),
    ]);
    const container = await renderAst(ast);
    expect(container.innerHTML).not.toContain('onerror');
    expect(container.innerHTML).not.toContain('<img');
    expect(container.innerHTML).toContain('after');
  });

  test('drops inline raw HTML inside a paragraph when the flag is off', async () => {
    const ast = makeRoot([
      {
        type: 'paragraph',
        children: [text('hello '), raw('<img src="z" onerror="alert(1)">', false), text(' world')],
      },
    ]);
    const container = await renderAst(ast);
    expect(container.innerHTML).not.toContain('onerror');
    expect(container.innerHTML).not.toContain('<img');
    expect(container.innerHTML).toContain('hello');
    expect(container.innerHTML).toContain('world');
  });

  test('does not duplicate raw output across re-renders', async () => {
    // RawHtml's useLayoutEffect cleanup must remove the nodes it spliced in
    // and restore the placeholder's slot, otherwise every re-render appends
    // another copy — unbounded growth for streaming markdown.
    const ast = makeRoot([
      {
        type: 'paragraph',
        children: [text('a '), raw('<span>x</span>', false)],
      },
    ]);
    const container = await renderAst(ast, { allowDangerousHtml: true });
    const firstHtml = container.innerHTML.replace(/<span style="display: ?none[^>]*><\/span>/, '');
    const spanCount = (firstHtml.match(/<span>x<\/span>/g) ?? []).length;
    expect(spanCount).toBe(1);

    // Re-render the same container with a different raw value.
    const ast2 = makeRoot([
      {
        type: 'paragraph',
        children: [text('b '), raw('<span>y</span>', false)],
      },
    ]);
    await act(async () => {
      root?.render(
        <Supramark
          markdown=""
          ast={ast2}
          config={{ options: { allowDangerousHtml: true } }}
        />
      );
    });
    const secondHtml = container.innerHTML.replace(/<span style="display: ?none[^>]*><\/span>/, '');
    expect((secondHtml.match(/<span>x<\/span>/g) ?? []).length).toBe(0);
    expect((secondHtml.match(/<span>y<\/span>/g) ?? []).length).toBe(1);
  });
});

// GFM tables: cmark-gfm wraps the header row in <thead>, the rest in <tbody>,
// and emits the obsolete `align` attribute (not an inline style) on aligned
// cells. See issue #144 (cmark-gfm conformance).
describe('GFM table rendering', () => {
  test('splits the header row into <thead> and body rows into <tbody>', async () => {
    const ast = makeRoot([
      table([
        tableRow([tableCell('foo', true), tableCell('bar', true)]),
        tableRow([tableCell('baz', false), tableCell('bim', false)]),
      ]),
    ]);
    const container = await renderAst(ast);
    const html = container.innerHTML;
    expect(html).toContain('<thead>');
    expect(html).toContain('</thead>');
    expect(html).toContain('<tbody>');
    expect(html).toContain('</tbody>');
    expect(html).toMatch(
      /<thead>\s*<tr[^>]*>\s*<th[^>]*>foo<\/th>\s*<th[^>]*>bar<\/th>\s*<\/tr>\s*<\/thead>/
    );
    expect(html).toMatch(
      /<tbody>\s*<tr[^>]*>\s*<td[^>]*>baz<\/td>\s*<td[^>]*>bim<\/td>\s*<\/tr>\s*<\/tbody>/
    );
  });

  test('emits align attribute (not inline style) on aligned cells', async () => {
    const ast = makeRoot([
      table(
        [
          tableRow([tableCell('aaa', true, 'left'), tableCell('ccc', true, 'center')]),
          tableRow([tableCell('fff', false, 'left'), tableCell('hhh', false, 'center')]),
        ],
        ['left', 'center']
      ),
    ]);
    const container = await renderAst(ast);
    const html = container.innerHTML;
    expect(html).toMatch(/<th[^>]*align="left"[^>]*>aaa<\/th>/);
    expect(html).toMatch(/<th[^>]*align="center"[^>]*>ccc<\/th>/);
    expect(html).toMatch(/<td[^>]*align="left"[^>]*>fff<\/td>/);
    expect(html).not.toMatch(/text-align/);
  });

  test('emits <thead> only with no <tbody> when the table has just a header row', async () => {
    const ast = makeRoot([table([tableRow([tableCell('abc', true), tableCell('def', true)])])]);
    const container = await renderAst(ast);
    const html = container.innerHTML;
    expect(html).toContain('<thead>');
    expect(html).not.toContain('<tbody');
  });
});

// GFM task lists: cmark-gfm's html_render emits `<input ... /> ` with a literal
// trailing space before the item text. The parser consumes the marker's
// separator whitespace (cmark's `spacechar+`), so the text node carries no
// leading space and the renderer emits the literal space here. See issue #144.
describe('GFM task list rendering', () => {
  function taskItem(checked: boolean, content: string) {
    return {
      type: 'list_item',
      checked,
      children: [text(content)],
    } as const;
  }

  test('preserves the leading space between the checkbox and the item text', async () => {
    const ast = makeRoot([
      {
        type: 'list',
        ordered: false,
        children: [taskItem(false, 'foo'), taskItem(true, 'bar')],
      },
    ]);
    const container = await renderAst(ast);
    const html = container.innerHTML;
    expect(html).toMatch(/<input[^>]*>\sfoo/);
    expect(html).toMatch(/<input[^>]*checked[^>]*>\sbar/);
    expect(html).not.toMatch(/<input[^>]*>foo/);
  });
});
