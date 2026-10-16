// cjk-allow-file: label fixtures quote real diagram text (CJK glyphs are the
// data under test — their width is what pushes a line past the wrapping width)
import { describe, test, expect } from 'bun:test';
import { normalizeSvg, normalizeSvgLight, stripRootSvgSize } from '../src/svgUtils';

// ============================================================================
// normalizeSvgLight — lightweight cleanup (for already style-inlined SVG such as MathJax)
// ============================================================================

test('normalizeSvgLight removes xml header / comments / style / title / desc / metadata', () => {
  const input =
    '<?xml version="1.0"?><!doctype svg><!-- c -->' +
    '<svg xmlns="x"><title>t</title><desc>d</desc><metadata>m</metadata>' +
    '<style>.a{fill:red}</style><rect/></svg>';
  const out = normalizeSvgLight(input);
  // Spaces inside tag attributes are preserved; whitespace between tags is collapsed
  expect(out).toBe('<svg xmlns="x"><rect/></svg>');
  expect(out).not.toMatch(/<\?xml|<!--|<style|<title|<desc|<metadata|<!doctype/i);
});

test('normalizeSvgLight collapses whitespace between tags', () => {
  expect(normalizeSvgLight('<svg>\n  <rect/>\n</svg>')).toBe('<svg><rect/></svg>');
});

// ============================================================================
// normalizeSvg — mermaid color inlining
// ============================================================================

test('normalizeSvg inlines a scoped CSS class selector color onto the rect attribute', () => {
  // Simulate mermaid's real structure: rect has no fill (relies on #id .node rect { fill:..; stroke:.. })
  const input =
    '<svg id="m1" viewBox="0 0 100 100">' +
    '<style>#m1 .node rect{fill:#ECECFF;stroke:#9370DB;stroke-width:1px}</style>' +
    '<g class="node"><rect class="basic label-container" style="" x="0" y="0" width="10" height="10"/></g>' +
    '</svg>';
  const out = normalizeSvg(input);
  const rect = out.match(/<rect[^>]*>/)?.[0] ?? '';
  expect(rect).toMatch(/fill="#ECECFF"/);
  expect(rect).toMatch(/stroke="#9370DB"/);
  expect(rect).toMatch(/stroke-width="1px"/);
  expect(out).not.toContain('<style');
});

test('normalizeSvg does not overwrite an element that already has a fill attribute', () => {
  const input =
    '<svg><style>.node rect{fill:#ECECFF}</style>' +
    '<rect class="node" fill="#FF0000" width="1" height="1"/></svg>';
  const out = normalizeSvg(input);
  const rect = out.match(/<rect[^>]*>/)?.[0] ?? '';
  expect(rect).toMatch(/fill="#FF0000"/);
  expect(rect).not.toMatch(/fill="#ECECFF"/);
});

test('normalizeSvg leaves rect untouched when the SVG has no <style> (no default color added)', () => {
  // With no CSS, inlineColors finds no matching rule, so rect stays in its input shape (color is never forced)
  const input = '<svg><rect class="x" width="1" height="1"/></svg>';
  const out = normalizeSvg(input);
  const rect = out.match(/<rect[^>]*>/)?.[0] ?? '';
  expect(rect).not.toMatch(/fill=/);
});

// ============================================================================
// normalizeSvg — foreignObject → text conversion (mermaid text)
// ============================================================================

test('normalizeSvg converts a foreignObject containing text into <text>', () => {
  // Simulate mermaid's node label structure: div/span/p text inside a foreignObject
  const input =
    '<svg viewBox="0 0 100 100">' +
    '<g transform="translate(50,50)">' +
    '<foreignObject width="40" height="16">' +
    '<div xmlns="x"><span class="nodeLabel"><p>Start</p></span></div>' +
    '</foreignObject>' +
    '</g></svg>';
  const out = normalizeSvg(input);
  expect(out).not.toContain('<foreignObject');
  expect(out).toContain('>Start<');
  // The converted text is centered using the foreignObject width (x=20), with height*0.7 as an approximate baseline (y≈11.2)
  const text = out.match(/<text[^>]*>Start<\/text>/)?.[0] ?? '';
  expect(text).toMatch(/x="20"/);
  expect(text).toMatch(/text-anchor="middle"/);
  expect(text).toMatch(/fill: #333/);
});

test('normalizeSvg removes an empty foreignObject (width=0 or no text)', () => {
  const input =
    '<svg><g>' +
    '<foreignObject width="0" height="16"><div><span class="edgeLabel"></span></div></foreignObject>' +
    '<foreignObject width="10" height="16"><div><span></span></div></foreignObject>' +
    '</g></svg>';
  const out = normalizeSvg(input);
  expect(out).not.toContain('<foreignObject');
  expect(out).not.toContain('<text');
});

test('normalizeSvg keeps multiple <p> texts inside a foreignObject as separate lines', () => {
  const input =
    '<svg><foreignObject width="20" height="32">' +
    '<div><p>line1</p><p>line2</p></div>' +
    '</foreignObject></svg>';
  const out = normalizeSvg(input);
  expect(out).toContain('<tspan x="10">line1</tspan>');
  expect(out).toContain('<tspan x="10" dy="16">line2</tspan>');
});

// ============================================================================
// normalizeSvg — d2 text default color + font-family quote escaping
// ============================================================================

test('normalizeSvg adds a default color to a <text style> without fill', () => {
  // d2 text's real structure: style has text-anchor/font-size but no fill, and no fill attribute either
  const input =
    '<svg><text x="1" y="2" class="text-bold" style="text-anchor:middle;font-size:16px">a</text></svg>';
  const out = normalizeSvg(input);
  const text = out.match(/<text[^>]*>/)?.[0] ?? '';
  expect(text).toMatch(/fill: #333/);
  expect(text).toMatch(/font-family:/);
});

test('normalizeSvg does not add a default color when text already has a fill attribute (does not overwrite step-2 inlining)', () => {
  // Once step 2 has inlined the class's fill into an attribute, step 3 must
  // not add #333 to style — style has higher priority than attributes and
  // would override the correct color. This is regression protection for
  // review issues 6/8.
  const input =
    '<svg><style>.title{fill:#ff0000}</style><text class="title" style="font-size:20px">Hi</text></svg>';
  const out = normalizeSvg(input);
  const text = out.match(/<text[^>]*>/)?.[0] ?? '';
  expect(text).toMatch(/fill="#ff0000"/);
  expect(text).not.toMatch(/fill:\s*#333/);
});

test('normalizeSvg inlineColors converts double quotes in CSS values to single quotes (prevents attribute nesting)', () => {
  // If a CSS fill/stroke value contains double quotes (rare but possible),
  // splicing it into <rect fill="..."> would nest quotes.
  // sanitizeCssValue converts double quotes to single quotes so the
  // attribute stays valid and SvgXml can parse it.
  const input =
    '<svg><style>.x{fill:"weird value"}</style><rect class="x" width="1" height="1"/></svg>';
  const out = normalizeSvg(input);
  const rect = out.match(/<rect[^>]*>/)?.[0] ?? '';
  expect(rect).toMatch(/fill="'weird value'"/);
  // The whole rect tag must not contain "..." nested inside "..." (nested double quotes would close the attribute early)
  expect(rect).not.toMatch(/fill="[^"]*"[^"]+"/);
});

// ============================================================================
// normalizeSvg — doesn't break existing structure
// ============================================================================

test("normalizeSvg does not accidentally strip a rect's class/style attributes (safe-regex regression)", () => {
  // The original />[^<]+</ regex would treat rect's class attribute string as raw text and corrupt it, breaking class-based CSS matching.
  const input =
    '<svg><style>.label-container{fill:#ECECFF}</style>' +
    '<rect class="basic label-container" style="" width="1" height="1"/></svg>';
  const out = normalizeSvg(input);
  const rect = out.match(/<rect[^>]*>/)?.[0] ?? '';
  // class must be preserved for inlineColors to match the CSS and produce the fill
  expect(rect).toMatch(/class="basic label-container"/);
  expect(rect).toMatch(/fill="#ECECFF"/);
});

test('normalizeSvg protects raw text inside <text> from being removed', () => {
  const input = '<svg><text x="0" y="0">Hello World</text></svg>';
  const out = normalizeSvg(input);
  expect(out).toContain('>Hello World<');
});

// ============================================================================
// normalizeSvg — well-formedness and real multi-rule regressions (covers review blocker defects)
// ============================================================================

// A self-closing shape must still end with /> after color is added — a / landing mid-attribute makes react-native-svg's parser throw, leaving the whole image blank.
test('normalizeSvg keeps a self-closing rect />-terminated after color is added (blocker 1 regression)', () => {
  const input =
    '<svg><style>.node rect{fill:#ECECFF;stroke:#9370DB}</style>' +
    '<g class="node"><rect class="basic label-container" width="10" height="10"/></g></svg>';
  const out = normalizeSvg(input);
  // No opening tag may show the "/ followed by an attribute" malformation (the signature of blocker 1)
  expect(out).not.toMatch(/\/\s+\w+="[^"]*"/);
  const rect = out.match(/<rect[^>]*>/)?.[0] ?? '';
  expect(rect).toMatch(/fill="#ECECFF".*\/>$/);
});

// When .node rect and .cluster rect both collapse to rect at their last segment, they must not overwrite each other — they're distinguished by the ancestor chain.
test('normalizeSvg colors .node rect and .cluster rect separately via the ancestor chain (blocker 2 regression)', () => {
  const input =
    '<svg id="m1">' +
    '<style>' +
    '#m1 .node rect{fill:#ECECFF;stroke:#9370DB;stroke-width:1px}' +
    '#m1 .cluster rect{fill:#ffffde;stroke:#aaaa33}' +
    '</style>' +
    '<g class="cluster"><rect class="cluster" width="200" height="200"/></g>' +
    '<g class="node"><rect class="basic label-container" width="100" height="40"/></g>' +
    '</svg>';
  const out = normalizeSvg(input);
  const rects = out.match(/<rect[^>]*>/g) ?? [];
  const nodeRect = rects.find(r => r.includes('label-container')) ?? '';
  const clusterRect = rects.find(r => r.includes('"cluster"')) ?? '';
  expect(nodeRect).toMatch(/fill="#ECECFF"/);
  expect(nodeRect).not.toMatch(/fill="#ffffde"/);
  expect(clusterRect).toMatch(/fill="#ffffde"/);
});

// !important must be stripped — once inlined into an attribute value it's invalid syntax (fill="#333 !important" fails and renders black).
test('normalizeSvg strips !important from CSS values', () => {
  const input =
    '<svg><style>.root .anchor path{fill:#333 !important}</style>' +
    '<g class="root"><g class="anchor"><path class="anchor" d="M0 0"/></g></g></svg>';
  const out = normalizeSvg(input);
  expect(out).not.toContain('!important');
  expect(out).toMatch(/fill="#333"/);
});

// A foreignObject with only a <span> and no <p> (venn labels) must also have its text extracted, not have the whole block deleted.
test('normalizeSvg also extracts <span> text inside a foreignObject', () => {
  const input =
    '<svg><g transform="translate(10,10)">' +
    '<foreignObject width="40" height="16"><div xmlns="x"><span class="nodeLabel">vennLabel</span></div></foreignObject>' +
    '</g></svg>';
  const out = normalizeSvg(input);
  expect(out).toContain('>vennLabel<');
});

// <br/> is a painted line boundary: react-native-svg draws one line per
// <text>, so each line becomes its own <tspan> — collapsing them into a space
// made `Line1<br/>Line2` and `Line1 Line2` render identically.
test('normalizeSvg keeps <br/> inside a foreignObject as separate lines', () => {
  const input =
    '<svg><foreignObject width="40" height="32">' +
    '<div xmlns="x"><span class="nodeLabel"><p>Line1<br/>Line2</p></span></div></foreignObject></svg>';
  const out = normalizeSvg(input);
  expect(out).toContain('<tspan x="20">Line1</tspan>');
  expect(out).toContain('<tspan x="20" dy="16">Line2</tspan>');
  expect(out).not.toContain('>Line1Line2<');
});

// A label the engine sized *smaller* than its own text — the flowchart
// `wrappingWidth` clamp (mermaid-little#254): the browser wraps it with
// `white-space: break-spaces` + `max-width`, so this renderer has to wrap it
// too, or the text overflows the node box.
test('normalizeSvg soft-wraps a clamped label to the foreignObject width', () => {
  // The issue's own repro: 4 painted lines, the third one 223 px at 16 px in a
  // 200 px box, and a 120 px tall box for the 5 lines the engine wrapped.
  const input =
    '<svg><foreignObject width="200" height="120">' +
    '<div><span class="nodeLabel"><p>python_codegen<br/>unittest + protoc 冒烟<br/>' +
    'compileall 无 legacy import 泄漏<br/>python -m build</p></span></div></foreignObject></svg>';
  const out = normalizeSvg(input);
  const tspans = out.match(/<tspan[^>]*>[^<]*<\/tspan>/g) ?? [];
  expect(tspans).toHaveLength(5);
  // 120 px / 5 painted lines, so the first baseline sits at 24 * 0.7.
  expect(out).toContain('y="16.799999999999997"');
  expect(tspans[2]).toContain('compileall 无 legacy');
  expect(tspans[3]).toContain('import 泄漏');
  expect(out).toMatch(/dy="24"/);
});

// A break at the end of a block opens no new line, but a *pair* of breaks does
// paint an empty one — the box height counts it, so it must survive.
test('normalizeSvg keeps an empty line between two <br/> breaks', () => {
  const input =
    '<svg><foreignObject width="40" height="72">' +
    '<div><span class="nodeLabel"><p>A<br/><br/>B</p></span></div></foreignObject></svg>';
  const out = normalizeSvg(input);
  const tspans = out.match(/<tspan[^>]*>[^<]*<\/tspan>/g) ?? [];
  expect(tspans).toHaveLength(3);
  expect(tspans[1]).toBe('<tspan x="20" dy="24"></tspan>');
});

// The expected counts here come from the engine itself
// (`label_metrics::split_label_lines`): the host must paint exactly the lines
// the box height was reserved for. Only `<br>` (attributes allowed) and
// `<p>`/`</p>` are boundaries — `</div>` and friends are transparent — and a
// break at the end of a block opens no line.
test('normalizeSvg paints the same hard lines the engine reserved', () => {
  const cases: Array<[string, number]> = [
    ['<p>A</p><p>B</p>', 2],
    ['A<br/><br/>B', 3],
    ['A<br/><br/>', 2],
    ['A<br/>', 1],
    ['<br/>A', 2],
    ['<p>A</div>B</p>', 1],
    ['<p>A</li>B</p>', 1],
    ['<p>A</h1>B</p>', 1],
    ['A<p>B', 2],
    ['<p>one<p>two</p>', 2],
    ['<p>A<br class="x">B</p>', 2],
    ['<p>a<br/>b<br/>c</p>', 3],
    ['<p>A<br>B<br />C</p>', 3],
    // A text element that merely *starts* with `<p` is not a boundary: only a
    // whole `<p …>` / `</p>` token is.
    ['<p><pre>code</pre></p>', 1],
    ['line1<br/><pre>code</pre>', 2],
    ['<p><path d="M0 0"/>label</p>', 1],
    ['<p><polygon points="0,0"/>label</p>', 1],
    ['line1<br/><path d="M0 0"/>label', 2],
    // Multiple trailing slashes are still the tag, as `is_p_tag_body` /
    // `is_br_tag_body` trim them all.
    ['a<p//>b', 2],
    ['a<br//>b', 2],
    // `<p/>` and `</p/>` are still p tags to the engine.
    ['one<p/>two', 2],
    ['one</p/>two', 2],
  ];
  for (const [body, expected] of cases) {
    // A box exactly as tall as those lines, so the soft wrap cannot fire.
    const input =
      `<svg><foreignObject width="400" height="${expected * 24}">` +
      `<div><span class="nodeLabel">${body}</span></div></foreignObject></svg>`;
    const out = normalizeSvg(input);
    const tspans = out.match(/<tspan[^>]*>[^<]*<\/tspan>/g) ?? [];
    const painted = tspans.length || (out.includes('<text') ? 1 : 0);
    expect({ body, painted }).toEqual({ body, painted: expected });
  }
});

// A label with no painted glyph paints nothing: react-native-svg would draw an
// empty line box where the engine reserved one, and there is nothing to show.
// (A guard, not a regression test: this holds before and after the wrap work.)
test('normalizeSvg drops a label with no text', () => {
  for (const body of ['<p></p>', '<p>   </p>']) {
    const input = `<svg><foreignObject width="40" height="24"><div><span class="nodeLabel">${body}</span></div></foreignObject></svg>`;
    expect(normalizeSvg(input)).not.toContain('<text');
  }
});

// The engine prefixes the node's inline `font-size` onto the label, and the box
// it reserved is in that same domain — so the host has to paint (and wrap) at
// that size, not at a hardcoded 16 px.
test('normalizeSvg paints a styled label at its own font size', () => {
  const input =
    '<svg><foreignObject width="200" height="90">' +
    '<div style="font-size: 30px !important; display: table; white-space: break-spaces; line-height: 1.5; max-width: 200px; text-align: center; width: 200px;">' +
    '<span style="font-size:30px !important" class="nodeLabel "><p>fits at sixteen px</p></span>' +
    '</div></foreignObject></svg>';
  const out = normalizeSvg(input);
  expect(out).toContain('font-size: 30px');
  // 90 px of box at a 30 px font is two line boxes, and the text only fits two
  // lines when it is wrapped at that size.
  const tspans = out.match(/<tspan[^>]*>[^<]*<\/tspan>/g) ?? [];
  expect(tspans).toHaveLength(2);
  expect(out).toContain('dy="45"');
});

// A label that fits its box is left alone: the engine measured it with real
// font metrics, and this module only estimates them, so re-wrapping on the
// estimate would break lines the engine deliberately left whole.
test('normalizeSvg does not re-wrap a label that fits its box', () => {
  const input =
    '<svg><foreignObject width="200" height="24">' +
    '<div><span class="nodeLabel"><p>iiiiiiiiiiiiiiiiiiiiii</p></span></div></foreignObject></svg>';
  const out = normalizeSvg(input);
  expect(out).toContain('>iiiiiiiiiiiiiiiiiiiiii<');
  expect(out).not.toContain('<tspan');
});

// A bare d2 <text> (no style, no fill) must also get a default color fallback, otherwise it defaults to black.
test('normalizeSvg adds a default fill to a bare <text> with no style', () => {
  const input = '<svg><text class="text-mono" x="0" y="10">code</text></svg>';
  const out = normalizeSvg(input);
  const text = out.match(/<text[^>]*>/)?.[0] ?? '';
  expect(text).toMatch(/fill:\s*#333|fill="#333"/);
});

// A compound selector rect.divider must match (tag + class in the same segment); the whole segment must not be treated as a key that never matches.
test('normalizeSvg matches the compound selector rect.divider', () => {
  const input =
    '<svg><style>rect.divider{stroke:#999}</style><rect class="divider" width="1" height="1"/></svg>';
  const out = normalizeSvg(input);
  expect(out).toMatch(/stroke="#999"/);
});

// color: only sets the text color in CSS semantics and has no effect on a rect's fill.
// .box{fill:blue;color:red} should produce fill=blue for a rect and must not be overridden by color:red.
test("normalizeSvg color: does not affect a rect's fill (only applies to text)", () => {
  const input =
    '<svg><style>.box{fill:blue;color:red}</style><rect class="box" width="1" height="1"/></svg>';
  const out = normalizeSvg(input);
  expect(out).toMatch(/fill="blue"/);
  expect(out).not.toMatch(/fill="red"/);
});

// color: is a fill candidate for text (e.g. radar titles are colored via color:).
test('normalizeSvg color: acts as a fill candidate for text', () => {
  const input =
    '<svg><style>.title{color:#ff6600}</style><text class="title" x="0" y="0">radar</text></svg>';
  const out = normalizeSvg(input);
  const text = out.match(/<text[^>]*>/)?.[0] ?? '';
  expect(text).toMatch(/fill="#ff6600"/);
});

// Overall well-formedness: every opening tag ends with > or />, with no malformed / left in the middle of attributes.
test('normalizeSvg outputs all tags well-formed', () => {
  const input =
    '<svg id="m1"><style>.node rect{fill:#ECECFF}.cluster rect{fill:#ffffde}</style>' +
    '<g class="cluster"><rect class="cluster" width="10" height="10"/></g>' +
    '<g class="node"><rect class="basic label-container" width="10" height="10"/></g>' +
    '<g class="node"><foreignObject width="40" height="16"><div><p>label</p></div></foreignObject></g>' +
    '<text x="0" y="0">t</text></svg>';
  const out = normalizeSvg(input);
  // Malformed tags with "/ followed by an attribute" are not allowed (blocker 1 signature)
  expect(out).not.toMatch(/\/\s+\w+="[^"]*"/);
  // No leftover !important in attribute values
  expect(out).not.toContain('!important');
  // No leftover <style>
  expect(out).not.toMatch(/<style/i);
});

// ============================================================================
// Ancestor classes match by exact word (guards against substring false positives)
// ============================================================================

// Ancestor segments must match by exact word, not string substring: the
// .node selector must not match an ancestor with class="nodes" (plural).
// mermaid has node/nodes and cluster/clusters coexisting in pairs; substring
// matching would color the wrong element.
test('normalizeSvg does not substring-match .node against a class="nodes" ancestor', () => {
  const input =
    '<svg><style>.node rect{fill:#9370DB}</style>' +
    '<g class="nodes"><rect class="basic" width="10" height="10"/></g></svg>';
  const out = normalizeSvg(input);
  expect(out).not.toMatch(/fill="#9370DB"/);
});

// The .label selector must not match substring ancestors like class="edgeLabel"/"nodeLabel".
test('normalizeSvg does not substring-match .label against a class="edgeLabel" ancestor', () => {
  const input =
    '<svg><style>.label rect{fill:#ffffde}</style>' +
    '<g class="edgeLabel"><rect class="basic" width="10" height="10"/></g></svg>';
  const out = normalizeSvg(input);
  expect(out).not.toMatch(/fill="#ffffde"/);
});

// A self-closing <g class="x"/> has no </g>, so its class must not be pushed onto the stack and leak to later sibling elements.
test("normalizeSvg does not leak a self-closing <g/>'s class to sibling elements", () => {
  // The self-closing g carries class="node"; the rect right after it sits in
  // a different g (with an empty class).
  // If the self-closing g were mistakenly pushed onto the stack, the rect
  // would be wrongly treated as having a node ancestor and colored #9370DB.
  const input =
    '<svg><style>.node rect{fill:#9370DB}</style>' +
    '<g class="node"/><g><rect class="basic" width="10" height="10"/></g></svg>';
  const out = normalizeSvg(input);
  expect(out).not.toMatch(/fill="#9370DB"/);
});

// ============================================================================
// Review nit hardening — one before/after regression probe per fix
// ============================================================================

// Nit 1: rewriting a <text> style must use a function replacer. A literal-string
// replacement lets $& / $` / $' / $n inside the new style value be interpreted as
// replacement patterns, corrupting the attribute value.
test('normalizeSvg <text> style rewrite is not mangled by $ replacement patterns', () => {
  const input = `<svg><text style="font-family:'a$&b';font-size:16px">x</text></svg>`;
  const out = normalizeSvg(input);
  // After fix: the style value survives verbatim, with the default fill appended.
  expect(out).toContain(`<text style="font-family:'a$&b';font-size:16px; fill: #333">`);
  // Before fix: $& expands to the whole match, producing a nested style=" ('a' then style=").
  expect(out).not.toContain('astyle=');
});

// Nit 2: a self-closing <text .../> must stay />-terminated after default-fill. The
// trailing slash must be captured separately and re-emitted, otherwise it lands in the
// middle of the attrs as the malformed <text ... fill="red"/ style=...>.
test('normalizeSvg self-closing <text/> stays />-terminated after styling', () => {
  const input = '<svg><style>.x{fill:red}</style><text class="x"/></svg>';
  const out = normalizeSvg(input);
  const text = out.match(/<text[^>]*>/)?.[0] ?? '';
  expect(text).toMatch(/fill="red"/); // fill comes from the step-2 inlining
  expect(text).toMatch(/\/>$/); // the tag is />-terminated
  expect(out).not.toMatch(/\/\s+\w+="[^"]*"/); // no "slash followed by attribute" malformation
});

// Nit 3: the "already has attr" guard uses (^|\s), not \b. \b also matches after the
// hyphen in data-fill / data-stroke, so data-fill="x" would be mistaken for an existing
// fill and skip the real CSS inlining.
test('normalizeSvg data-fill does not block real CSS fill inlining', () => {
  const input =
    '<svg><style>.c{fill:red}</style><rect data-fill="x" class="c" width="1" height="1"/></svg>';
  const out = normalizeSvg(input);
  const rect = out.match(/<rect[^>]*>/)?.[0] ?? '';
  expect(rect).toMatch(/fill="red"/); // data-fill is "x"; fill="red" can only be the inlined one
  expect(rect).toContain('data-fill="x"');
});

// Nit 4: line / polyline are stroke-bearing shapes; they must receive the CSS stroke and
// must not be given a spurious solid fill.
test('normalizeSvg <line> receives CSS stroke without a forced fill', () => {
  const input =
    '<svg><style>.edge{stroke:#333}</style><line class="edge" x1="0" y1="0" x2="10" y2="10"/></svg>';
  const out = normalizeSvg(input);
  const line = out.match(/<line[^>]*>/)?.[0] ?? '';
  expect(line).toMatch(/stroke="#333"/);
  expect(line).not.toContain('fill='); // stroke-only: never invent a fill when CSS omits it
});

test('normalizeSvg <polyline> matches a class selector and gets stroke', () => {
  const input =
    '<svg><style>.grid{stroke:#ccc}</style><polyline class="grid" points="0,0 10,10"/></svg>';
  const out = normalizeSvg(input);
  const poly = out.match(/<polyline[^>]*>/)?.[0] ?? '';
  expect(poly).toMatch(/stroke="#ccc"/);
});

// Nit 4b: a stroked shape whose CSS explicitly sets fill:none must keep fill="none"
// (it must not be treated as "no fill" and dropped).
test('normalizeSvg <line> keeps an explicit CSS fill:none', () => {
  const input =
    '<svg><style>.edge{stroke:#333;fill:none}</style><line class="edge" x1="0" y1="0" x2="1" y2="1"/></svg>';
  const out = normalizeSvg(input);
  const line = out.match(/<line[^>]*>/)?.[0] ?? '';
  expect(line).toMatch(/fill="none"/);
  expect(line).toMatch(/stroke="#333"/);
});

// Nit 5: the foreignObject -> text label color should inherit the inner span/div inline
// color; it falls back to #333 when no color is present.
test('normalizeSvg foreignObject label inherits the inner color', () => {
  const input =
    '<svg><g transform="translate(10,10)">' +
    '<foreignObject width="40" height="16"><div xmlns="x"><span style="color:#ff0000">Hi</span></div></foreignObject>' +
    '</g></svg>';
  const out = normalizeSvg(input);
  const text = out.match(/<text[^>]*>Hi<\/text>/)?.[0] ?? '';
  expect(text).toMatch(/fill:\s*#ff0000/);
});

test('normalizeSvg foreignObject falls back to #333 and ignores background-color', () => {
  const input =
    '<svg><foreignObject width="40" height="16">' +
    '<div style="background-color:#fff"><span class="nodeLabel">Plain</span></div></foreignObject></svg>';
  const out = normalizeSvg(input);
  const text = out.match(/<text[^>]*>Plain<\/text>/)?.[0] ?? '';
  expect(text).toMatch(/fill:\s*#333/);
  expect(text).not.toMatch(/#fff/);
});

test('normalizeSvg foreignObject color comes from a style attr, not visible text', () => {
  // A label whose visible text literally contains "color: red" must not be read
  // as a CSS declaration; the fill stays the default.
  const input =
    '<svg><foreignObject width="80" height="16">' +
    '<div><span class="nodeLabel">set color: red here</span></div></foreignObject></svg>';
  const out = normalizeSvg(input);
  const text = out.match(/<text[^>]*>[^<]*<\/text>/)?.[0] ?? '';
  expect(text).toMatch(/fill:\s*#333/);
  expect(text).not.toMatch(/fill:\s*red/);
});

test('normalizeSvg foreignObject inner span color wins over the outer', () => {
  const input =
    '<svg><foreignObject width="40" height="16">' +
    '<div style="color:#111111"><span style="color:#00ff00">Hi</span></div></foreignObject></svg>';
  const out = normalizeSvg(input);
  const text = out.match(/<text[^>]*>Hi<\/text>/)?.[0] ?? '';
  expect(text).toMatch(/fill:\s*#00ff00/);
});

test('normalizeSvg foreignObject color drops a trailing !important', () => {
  const input =
    '<svg><foreignObject width="40" height="16">' +
    '<div><span style="color:#0000ff !important">Hi</span></div></foreignObject></svg>';
  const out = normalizeSvg(input);
  const text = out.match(/<text[^>]*>Hi<\/text>/)?.[0] ?? '';
  expect(text).toMatch(/fill:\s*#0000ff/);
  expect(text).not.toMatch(/!important/);
});

// ============================================================================
// stripRootSvgSize — precisely removes the root <svg>'s width/height (from upstream/main)
// ============================================================================

describe('stripRootSvgSize', () => {
  test('d2 double nesting: returns unchanged when the outer has no width/height, inner viewport preserved', () => {
    const svg =
      '<svg xmlns="http://www.w3.org/2000/svg">' +
      '<svg class="d2-svg" width="350" height="400" viewBox="-1 -1 350 400">' +
      '<g/></svg></svg>';
    expect(stripRootSvgSize(svg)).toBe(svg);
  });

  test('d2 + scale: outer width/height is removed, inner d2-svg fully preserved', () => {
    const svg =
      '<svg xmlns="http://www.w3.org/2000/svg" width="700" height="800">' +
      '<svg class="d2-svg" width="350" height="400" viewBox="-1 -1 350 400">' +
      '<g/></svg></svg>';
    const result = stripRootSvgSize(svg);
    expect(result).not.toContain('width="700"');
    expect(result).not.toContain('height="800"');
    expect(result).toContain(
      '<svg class="d2-svg" width="350" height="400" viewBox="-1 -1 350 400">'
    );
  });

  test('mermaid: root width="100%" is removed with no leftover double space, viewBox and id preserved', () => {
    const svg =
      '<svg id="mermaid-abc123" width="100%" viewBox="0 0 100 50" xmlns="http://www.w3.org/2000/svg">' +
      '<g/></svg>';
    const result = stripRootSvgSize(svg);
    expect(result).not.toContain('width=');
    expect(result).toContain('viewBox="0 0 100 50"');
    expect(result).toContain('id="mermaid-abc123"');
    expect(result).not.toContain('"  ');
  });

  test('root attribute containing a $ pattern character: the function-form replacement is not swallowed by a replacement pattern', () => {
    const svg = '<svg width="100%" height="50" data-token="$&amp;bar"><g/></svg>';
    const result = stripRootSvgSize(svg);
    expect(result).toContain('data-token="$&amp;bar"');
    expect(result).not.toContain('width=');
    expect(result).not.toContain('height=');
  });
});
