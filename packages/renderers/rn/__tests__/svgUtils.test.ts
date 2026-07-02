import { describe, test, expect } from 'bun:test';
import { normalizeSvg, normalizeSvgLight, stripRootSvgSize } from '../src/svgUtils';

// ============================================================================
// normalizeSvgLight — 轻量清理（用于 MathJax 这类已内联样式的 SVG）
// ============================================================================

test('normalizeSvgLight 删除 xml 头 / 注释 / style / title / desc / metadata', () => {
  const input =
    '<?xml version="1.0"?><!doctype svg><!-- c -->' +
    '<svg xmlns="x"><title>t</title><desc>d</desc><metadata>m</metadata>' +
    '<style>.a{fill:red}</style><rect/></svg>';
  const out = normalizeSvgLight(input);
  // 标签内属性空格保留，标签间空白被压缩
  expect(out).toBe('<svg xmlns="x"><rect/></svg>');
  expect(out).not.toMatch(/<\?xml|<!--|<style|<title|<desc|<metadata|<!doctype/i);
});

test('normalizeSvgLight 压缩标签间空白', () => {
  expect(normalizeSvgLight('<svg>\n  <rect/>\n</svg>')).toBe('<svg><rect/></svg>');
});

// ============================================================================
// normalizeSvg — mermaid 颜色内联
// ============================================================================

test('normalizeSvg 把 scoped CSS class 选择器的颜色内联到 rect 属性', () => {
  // 模拟 mermaid 真实结构：rect 无 fill（靠 #id .node rect { fill:..; stroke:.. }）
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

test('normalizeSvg 元素已有 fill 属性时不覆盖', () => {
  const input =
    '<svg><style>.node rect{fill:#ECECFF}</style>' +
    '<rect class="node" fill="#FF0000" width="1" height="1"/></svg>';
  const out = normalizeSvg(input);
  const rect = out.match(/<rect[^>]*>/)?.[0] ?? '';
  expect(rect).toMatch(/fill="#FF0000"/);
  expect(rect).not.toMatch(/fill="#ECECFF"/);
});

test('normalizeSvg 没有 <style> 的 SVG 不补默认色到 rect（保持原样）', () => {
  // 无 CSS 时 inlineColors 找不到匹配的规则，rect 维持输入形态（不强行补色）
  const input = '<svg><rect class="x" width="1" height="1"/></svg>';
  const out = normalizeSvg(input);
  const rect = out.match(/<rect[^>]*>/)?.[0] ?? '';
  expect(rect).not.toMatch(/fill=/);
});

// ============================================================================
// normalizeSvg — foreignObject → text 转换（mermaid 文字）
// ============================================================================

test('normalizeSvg 把含文本的 foreignObject 转成 <text>', () => {
  // 模拟 mermaid 节点标签结构：foreignObject 内 div/span/p 文本
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
  // 转换出的 text 用 foreignObject width 居中（x=20）、height*0.7 近似基线（y≈11.2）
  const text = out.match(/<text[^>]*>Start<\/text>/)?.[0] ?? '';
  expect(text).toMatch(/x="20"/);
  expect(text).toMatch(/text-anchor="middle"/);
  expect(text).toMatch(/fill: #333/);
});

test('normalizeSvg 把空 foreignObject（width=0 或无文本）删除', () => {
  const input =
    '<svg><g>' +
    '<foreignObject width="0" height="16"><div><span class="edgeLabel"></span></div></foreignObject>' +
    '<foreignObject width="10" height="16"><div><span></span></div></foreignObject>' +
    '</g></svg>';
  const out = normalizeSvg(input);
  expect(out).not.toContain('<foreignObject');
  expect(out).not.toContain('<text');
});

test('normalizeSvg foreignObject 多个 <p> 文本拼接为单个 text', () => {
  const input =
    '<svg><foreignObject width="20" height="16">' +
    '<div><p>line1</p><p>line2</p></div>' +
    '</foreignObject></svg>';
  const out = normalizeSvg(input);
  expect(out).toContain('>line1 line2<');
});

// ============================================================================
// normalizeSvg — d2 text 补色 + font-family 引号转义
// ============================================================================

test('normalizeSvg 给无 fill 的 <text style> 补默认色', () => {
  // d2 text 真实结构：style 含 text-anchor/font-size 但无 fill，也无 fill 属性
  const input =
    '<svg><text x="1" y="2" class="text-bold" style="text-anchor:middle;font-size:16px">a</text></svg>';
  const out = normalizeSvg(input);
  const text = out.match(/<text[^>]*>/)?.[0] ?? '';
  expect(text).toMatch(/fill: #333/);
  expect(text).toMatch(/font-family:/);
});

test('normalizeSvg text 已有 fill 属性时不补默认色（不覆盖 step-2 内联）', () => {
  // step-2 把 class 的 fill 内联成属性后，step-3 不能再往 style 补 #333——style 优先级
  // 高于属性，会覆盖掉正确颜色。这是 review 问题 6/8 的回归防护。
  const input =
    '<svg><style>.title{fill:#ff0000}</style><text class="title" style="font-size:20px">Hi</text></svg>';
  const out = normalizeSvg(input);
  const text = out.match(/<text[^>]*>/)?.[0] ?? '';
  expect(text).toMatch(/fill="#ff0000"/);
  expect(text).not.toMatch(/fill:\s*#333/);
});

test('normalizeSvg inlineColors 把 CSS 值里的双引号转单引号（防属性嵌套）', () => {
  // 若 CSS 的 fill/stroke 值带双引号（少见但可能），拼进 <rect fill="..."> 会嵌套。
  // sanitizeCssValue 把双引号转单引号，保证属性合法、SvgXml 能解析。
  const input =
    '<svg><style>.x{fill:"weird value"}</style><rect class="x" width="1" height="1"/></svg>';
  const out = normalizeSvg(input);
  const rect = out.match(/<rect[^>]*>/)?.[0] ?? '';
  expect(rect).toMatch(/fill="'weird value'"/);
  // 整个 rect 标签不出现 "..." 内嵌 "..."（嵌套双引号会让属性提前关闭）
  expect(rect).not.toMatch(/fill="[^"]*"[^"]+"/);
});

// ============================================================================
// normalizeSvg — 不破坏既有结构
// ============================================================================

test('normalizeSvg 不误删 rect 的 class/style 属性（安全正则回归）', () => {
  // 原版 />[^<]+</ 会把 rect 的 class 属性串当裸文本破坏，导致按 class 匹配 CSS 失效。
  const input =
    '<svg><style>.label-container{fill:#ECECFF}</style>' +
    '<rect class="basic label-container" style="" width="1" height="1"/></svg>';
  const out = normalizeSvg(input);
  const rect = out.match(/<rect[^>]*>/)?.[0] ?? '';
  // class 被保留才能让 inlineColors 匹配上 CSS、补出 fill
  expect(rect).toMatch(/class="basic label-container"/);
  expect(rect).toMatch(/fill="#ECECFF"/);
});

test('normalizeSvg 保护 <text> 内的裸文本不被删', () => {
  const input = '<svg><text x="0" y="0">Hello World</text></svg>';
  const out = normalizeSvg(input);
  expect(out).toContain('>Hello World<');
});

// ============================================================================
// normalizeSvg — well-formedness 与真实多规则回归（覆盖 review 阻断缺陷）
// ============================================================================

// 自闭合形状补色后必须仍以 /> 结尾——/ 落在属性中间会让 react-native-svg 解析抛错整图空白。
test('normalizeSvg 自闭合 rect 补色后保持 /> 结尾（阻断 1 回归）', () => {
  const input =
    '<svg><style>.node rect{fill:#ECECFF;stroke:#9370DB}</style>' +
    '<g class="node"><rect class="basic label-container" width="10" height="10"/></g></svg>';
  const out = normalizeSvg(input);
  // 任何开标签都不能出现「/ 后跟属性」的畸形（阻断 1 的特征）
  expect(out).not.toMatch(/\/\s+\w+="[^"]*"/);
  const rect = out.match(/<rect[^>]*>/)?.[0] ?? '';
  expect(rect).toMatch(/fill="#ECECFF".*\/>$/);
});

// .node rect 与 .cluster rect 末段都塌缩成 rect 时不能互相覆盖——按祖先链区分。
test('normalizeSvg .node rect 与 .cluster rect 按祖先链分别上色（阻断 2 回归）', () => {
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

// !important 必须剥离——内联成属性值后是非法语法（fill="#333 !important" 会失效变黑）。
test('normalizeSvg 剥离 CSS 值里的 !important', () => {
  const input =
    '<svg><style>.root .anchor path{fill:#333 !important}</style>' +
    '<g class="root"><g class="anchor"><path class="anchor" d="M0 0"/></g></g></svg>';
  const out = normalizeSvg(input);
  expect(out).not.toContain('!important');
  expect(out).toMatch(/fill="#333"/);
});

// foreignObject 内只有 <span> 无 <p>（venn 标签）也要提取文本，不能整段删除。
test('normalizeSvg foreignObject 内 <span> 文本也被提取', () => {
  const input =
    '<svg><g transform="translate(10,10)">' +
    '<foreignObject width="40" height="16"><div xmlns="x"><span class="nodeLabel">vennLabel</span></div></foreignObject>' +
    '</g></svg>';
  const out = normalizeSvg(input);
  expect(out).toContain('>vennLabel<');
});

// <br/> 是行边界，剥标签前必须转空格，否则 Line1<br/>Line2 粘成 Line1Line2。
test('normalizeSvg foreignObject 内 <br/> 转空格避免行粘连', () => {
  const input =
    '<svg><foreignObject width="40" height="32">' +
    '<div xmlns="x"><span class="nodeLabel"><p>Line1<br/>Line2</p></span></div></foreignObject></svg>';
  const out = normalizeSvg(input);
  expect(out).toContain('>Line1 Line2<');
  expect(out).not.toContain('>Line1Line2<');
});

// d2 裸 <text>（无 style 无 fill）也要兜底默认色，否则默认黑。
test('normalizeSvg 无 style 的裸 <text> 补默认 fill', () => {
  const input = '<svg><text class="text-mono" x="0" y="10">code</text></svg>';
  const out = normalizeSvg(input);
  const text = out.match(/<text[^>]*>/)?.[0] ?? '';
  expect(text).toMatch(/fill:\s*#333|fill="#333"/);
});

// 复合选择器 rect.divider 必须命中（tag + class 同段），不能把整段当 key 永不匹配。
test('normalizeSvg 复合选择器 rect.divider 命中', () => {
  const input =
    '<svg><style>rect.divider{stroke:#999}</style><rect class="divider" width="1" height="1"/></svg>';
  const out = normalizeSvg(input);
  expect(out).toMatch(/stroke="#999"/);
});

// color: 在 CSS 语义里只设置文本颜色，对 rect 的 fill 无影响。
// .box{fill:blue;color:red} 对 rect 应产出 fill=blue，不能被 color:red 覆盖。
test('normalizeSvg color: 不影响 rect 的 fill（仅对 text 生效）', () => {
  const input =
    '<svg><style>.box{fill:blue;color:red}</style><rect class="box" width="1" height="1"/></svg>';
  const out = normalizeSvg(input);
  expect(out).toMatch(/fill="blue"/);
  expect(out).not.toMatch(/fill="red"/);
});

// color: 对 text 是 fill 候选（radar 标题等用 color: 上色）。
test('normalizeSvg color: 作为 text 的 fill 候选', () => {
  const input =
    '<svg><style>.title{color:#ff6600}</style><text class="title" x="0" y="0">radar</text></svg>';
  const out = normalizeSvg(input);
  const text = out.match(/<text[^>]*>/)?.[0] ?? '';
  expect(text).toMatch(/fill="#ff6600"/);
});

// 整体 well-formedness：所有开标签以 > 或 /> 结尾，不残留畸形 / 在属性中间。
test('normalizeSvg 输出所有标签 well-formed', () => {
  const input =
    '<svg id="m1"><style>.node rect{fill:#ECECFF}.cluster rect{fill:#ffffde}</style>' +
    '<g class="cluster"><rect class="cluster" width="10" height="10"/></g>' +
    '<g class="node"><rect class="basic label-container" width="10" height="10"/></g>' +
    '<g class="node"><foreignObject width="40" height="16"><div><p>label</p></div></foreignObject></g>' +
    '<text x="0" y="0">t</text></svg>';
  const out = normalizeSvg(input);
  // 不允许「/ 后跟属性」的畸形标签（阻断 1 特征）
  expect(out).not.toMatch(/\/\s+\w+="[^"]*"/);
  // 不允许属性值里残留 !important
  expect(out).not.toContain('!important');
  // 不残留 <style>
  expect(out).not.toMatch(/<style/i);
});

// ============================================================================
// 祖先 class 按词精确匹配（防子串误命中，对抗探针）
// ============================================================================

// 祖先段必须按词精确匹配，不能用字符串子串：.node 选择器不能命中 class="nodes"
//（复数）的祖先。mermaid 里 node/nodes、cluster/clusters 成对共存，子串匹配会染错色。
test('normalizeSvg .node 不子串命中 class="nodes" 祖先', () => {
  const input =
    '<svg><style>.node rect{fill:#9370DB}</style>' +
    '<g class="nodes"><rect class="basic" width="10" height="10"/></g></svg>';
  const out = normalizeSvg(input);
  expect(out).not.toMatch(/fill="#9370DB"/);
});

// .label 选择器不能命中 class="edgeLabel"/"nodeLabel" 等子串祖先。
test('normalizeSvg .label 不子串命中 class="edgeLabel" 祖先', () => {
  const input =
    '<svg><style>.label rect{fill:#ffffde}</style>' +
    '<g class="edgeLabel"><rect class="basic" width="10" height="10"/></g></svg>';
  const out = normalizeSvg(input);
  expect(out).not.toMatch(/fill="#ffffde"/);
});

// 自闭合 <g class="x"/> 无 </g>，其 class 不应入栈泄漏给后续兄弟元素。
test('normalizeSvg 自闭合 <g/> 的 class 不泄漏给兄弟元素', () => {
  // 自闭合 g 带 class="node"，紧随其后的 rect 在另一个 g 里（class 为空）。
  // 若自闭合 g 错误入栈，rect 会被误认为有 node 祖先而染上 #9370DB。
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
// stripRootSvgSize — 精确删除根 <svg> 的 width/height（来自 upstream/main）
// ============================================================================

describe('stripRootSvgSize', () => {
  test('d2 双层嵌套：外层无 width/height 时原样返回，内层 viewport 保留', () => {
    const svg =
      '<svg xmlns="http://www.w3.org/2000/svg">' +
      '<svg class="d2-svg" width="350" height="400" viewBox="-1 -1 350 400">' +
      '<g/></svg></svg>';
    expect(stripRootSvgSize(svg)).toBe(svg);
  });

  test('d2 + scale：外层 width/height 被删，内层 d2-svg 完整保留', () => {
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

  test('mermaid：根 width="100%" 被删且不留双空格，viewBox 与 id 保留', () => {
    const svg =
      '<svg id="mermaid-abc123" width="100%" viewBox="0 0 100 50" xmlns="http://www.w3.org/2000/svg">' +
      '<g/></svg>';
    const result = stripRootSvgSize(svg);
    expect(result).not.toContain('width=');
    expect(result).toContain('viewBox="0 0 100 50"');
    expect(result).toContain('id="mermaid-abc123"');
    expect(result).not.toContain('"  ');
  });

  test('根属性含 $ 模式字符：函数式 replacement 不被替换模式吃掉', () => {
    const svg = '<svg width="100%" height="50" data-token="$&amp;bar"><g/></svg>';
    const result = stripRootSvgSize(svg);
    expect(result).toContain('data-token="$&amp;bar"');
    expect(result).not.toContain('width=');
    expect(result).not.toContain('height=');
  });
});
