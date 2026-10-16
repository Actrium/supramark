// Unified SVG preprocessing utilities for adapting SVG produced by
// Mermaid / d2 into a form better suited to react-native-svg rendering.

/**
 * Lightweight cleanup: for SVG that already has styles inlined and needs no
 * color processing (e.g. MathJax).
 */
export function normalizeSvgLight(xml: string): string {
  return xml
    .replace(/<\?xml[\s\S]*?\?>/gi, '')
    .replace(/<\?[\s\S]*?\?>/g, '')
    .replace(/<!doctype[\s\S]*?>/gi, '')
    .replace(/<!--[\s\S]*?-->/g, '')
    .replace(/<title[\s\S]*?<\/title>/gi, '')
    .replace(/<desc[\s\S]*?<\/desc>/gi, '')
    .replace(/<metadata[\s\S]*?<\/metadata>/gi, '')
    .replace(/<style[^>]*>[\s\S]*?<\/style>/gi, '')
    .replace(/>\s+</g, '><');
}

type ColorKey = 'fill' | 'stroke' | 'stroke-width' | 'font-family' | 'font-size' | 'color';
type CssDecls = Partial<Record<ColorKey, string>>;

// color: in CSS semantics only sets the text color and has no effect on a
// rect/path's fill. Kept separately here so inlineColors only treats color as
// a fill candidate for text elements — this avoids .box{fill:blue;color:red}
// coloring a rect red (it should stay blue).
const DECL_KEY_MAP: Record<string, ColorKey | undefined> = {
  fill: 'fill',
  stroke: 'stroke',
  'stroke-width': 'stroke-width',
  'font-family': 'font-family',
  'font-size': 'font-size',
  color: 'color',
};

// One segment of a selector, e.g. `.node rect` → { tag:'rect', classes:['node'] }, `rect.divider` → { tag:'rect', classes:['divider'] }.
type SelectorPart = { tag: string | null; classes: string[] };
type CssRule = { selector: SelectorPart[]; decls: CssDecls };

/**
 * Parses the CSS rules inside <style>. Selectors are stored as a full
 * "ancestor → self" chain, preserving source order.
 *
 * mermaid/d2 CSS is mostly scoped: `#id .node rect { fill:#ECECFF }`.
 * react-native-svg doesn't support CSS selectors, so colors need to be
 * inlined onto element attributes. The previous implementation built a flat
 * key from a selector's last segment, causing `.node rect` and `.cluster
 * rect` to both collapse to `rect` and overwrite each other. This version
 * keeps the full ancestor chain and matches an element's class plus its
 * ancestor class chain segment by segment.
 *
 * For the same selector, a later declaration overrides an earlier one (CSS
 * source order is the priority; mermaid's output is already sorted by
 * specificity, so no weight calculation is needed).
 * `!important` is stripped here — once inlined into an attribute value it's
 * invalid syntax and would have no effect.
 */
function parseCssRules(cssText: string): CssRule[] {
  const rules: CssRule[] = [];
  for (const [, selectorGroup, body] of cssText.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const decls: CssDecls = {};
    for (const part of body.split(';')) {
      const idx = part.indexOf(':');
      if (idx <= 0) continue;
      const key = part.slice(0, idx).trim().toLowerCase();
      const rawValue = part.slice(idx + 1).trim();
      const mapped = DECL_KEY_MAP[key];
      if (!mapped) continue;
      // Strip !important: inlined attribute values don't support it, and
      // copying it in as-is would make the attribute value invalid
      // (fill="#333 !important").
      const value = rawValue.replace(/\s*!important\s*$/i, '');
      decls[mapped] = value;
    }
    if (Object.keys(decls).length === 0) continue;
    for (const sel of selectorGroup
      .split(',')
      .map(s => s.trim())
      .filter(Boolean)) {
      const selector = parseSelector(sel);
      if (selector.length === 0) continue;
      rules.push({ selector, decls });
    }
  }
  return rules;
}

/** Parses one selector into an ancestor→self chain of segments. Ignores ids and pseudo-classes; a compound selector `.a.b rect` merges its classes. */
function parseSelector(sel: string): SelectorPart[] {
  const parts: SelectorPart[] = [];
  for (const chunk of sel.split(/\s+/).filter(Boolean)) {
    // A pure id selector (#m1) doesn't produce a constraint segment — mermaid's
    // scoped id doesn't participate in inline matching, so skip it to avoid
    // polluting the segment chain.
    if (chunk.startsWith('#')) continue;
    // Handle forms like rect.divider / .a.b tag — a tag and multiple classes
    // can coexist within one compound selector.
    const classes: string[] = [];
    let tag: string | null = null;
    for (const token of chunk.split(/(?=\.)/)) {
      const t = token.trim();
      if (!t) continue;
      if (t.startsWith('.')) classes.push(t.slice(1));
      else if (t.startsWith(':')) {
        /* pseudo-class, ignored */
      } else tag = t.toLowerCase();
    }
    parts.push({ tag, classes });
  }
  return parts;
}

/**
 * Whether a rule matches an element: compares from the selector's last
 * segment (self) backward against the top of the ancestor stack.
 * The last segment's tag/classes must match the current element; the
 * remaining segments match ancestor `g` classes one by one from the top of
 * the stack downward.
 * Both self and ancestor segments use array Array.includes for exact
 * word-level matching — the ancestor stack stores arrays rather than joined
 * strings, avoiding String.includes substring matches that would wrongly hit
 * .node on "nodes" or .label on "edgeLabel".
 */
function ruleMatches(
  rule: CssRule,
  tag: string,
  classes: string[],
  ancestorClasses: string[][]
): boolean {
  const parts = rule.selector;
  const self = parts[parts.length - 1];
  if (self.tag && self.tag !== tag) return false;
  if (self.classes.length && !self.classes.every(c => classes.includes(c))) return false;
  // The remaining segments are ancestor constraints, matched from the top of
  // the stack (nearest ancestor) downward.
  let ancIdx = ancestorClasses.length - 1;
  for (let i = parts.length - 2; i >= 0; i--) {
    const anc = parts[i];
    let found = false;
    while (ancIdx >= 0) {
      const ancCls: string[] = ancestorClasses[ancIdx];
      ancIdx--;
      // Ancestor classes use exact word-level matching (Array.includes), consistent with the self segment.
      if (anc.classes.length && anc.classes.every(c => ancCls.includes(c))) {
        found = true;
        break;
      }
    }
    if (!found) return false;
  }
  return true;
}

/** Converts double quotes to single quotes, avoiding nested double quotes when spliced into style="..." (d2 font-family "d2-<hash>-font-bold"). */
const sanitizeCssValue = (v: string): string => v.replace(/"/g, "'");

// line / polyline are stroke-only; the fill logic below stays conservative and only
// emits a fill when the CSS supplies one (e.g. fill:none), never a spurious default.
const SHAPE_TAGS = /^(rect|path|circle|ellipse|polygon|line|polyline|text)$/;

/**
 * Normalizes SVG (for mermaid / d2):
 * 1. Parses the CSS rules in <style> and inlines class-selector fill/stroke
 *    onto shape/text element attributes — otherwise, once <style> is
 *    removed, elements have no color source and react-native-svg defaults
 *    to a black fill.
 * 2. Removes <style> (react-native-svg doesn't support CSS selectors).
 * 3. foreignObject → text (react-native-svg doesn't render its HTML children).
 * 4. Strips the xml header/comments + whitespace between tags, protecting
 *    text inside text/foreignObject from being mistakenly removed.
 */
// ============================================================================
// Label line layout — soft wrapping for foreignObject labels
//
// The engine emits HTML labels the way a browser lays them out: a `<div>` with
// `max-width`/`width` from `flowchart.wrappingWidth` and
// `white-space: break-spaces`, which wraps every painted line inside that box
// (mermaid-little#254). react-native-svg has no CSS layout, so this module
// flattens the label into `<text>`/`<tspan>` and has to do the wrapping itself:
// without it a label the engine correctly sized to 200 px draws its source
// lines at full length and overflows the node box.
//
// Widths are estimated, not measured — RN cannot measure text from a string
// transform. The estimate mirrors the engine's own (`label_metrics::
// painted_line_width`: ~1em for the wide glyphs a browser paints full-width,
// ~0.56em for the rest), so the line count here matches the line count the
// engine reserved height for.
// ============================================================================

// East Asian Wide / Fullwidth code points as `[start, end, start, end, ...]`:
// the raw EAW (W + F) set, minus the combining/filler code points
// `unicode_width::UnicodeWidthChar::width` reports as 0 (`label_metrics::
// is_wide_glyph`), plus the Unicode 15.1 additions it already knows. Regenerate
// with `unicode_width`'s own table if it moves.
const WIDE_RANGES: readonly number[] = [
  0x1100, 0x115f, 0x17a4, 0x17a4, 0x231a, 0x231b, 0x2329, 0x232a, 0x23e9, 0x23ec, 0x23f0, 0x23f0,
  0x23f3, 0x23f3, 0x25fd, 0x25fe, 0x2614, 0x2615, 0x2648, 0x2653, 0x267f, 0x267f, 0x2693, 0x2693,
  0x26a1, 0x26a1, 0x26aa, 0x26ab, 0x26bd, 0x26be, 0x26c4, 0x26c5, 0x26ce, 0x26ce, 0x26d4, 0x26d4,
  0x26ea, 0x26ea, 0x26f2, 0x26f3, 0x26f5, 0x26f5, 0x26fa, 0x26fa, 0x26fd, 0x26fd, 0x2705, 0x2705,
  0x270a, 0x270b, 0x2728, 0x2728, 0x274c, 0x274c, 0x274e, 0x274e, 0x2753, 0x2755, 0x2757, 0x2757,
  0x2795, 0x2797, 0x27b0, 0x27b0, 0x27bf, 0x27bf, 0x2b1b, 0x2b1c, 0x2b50, 0x2b50, 0x2b55, 0x2b55,
  0x2e80, 0x2e99, 0x2e9b, 0x2ef3, 0x2f00, 0x2fd5, 0x2ff0, 0x3029, 0x3030, 0x303e, 0x3041, 0x3096,
  0x309b, 0x30ff, 0x3105, 0x312f, 0x3131, 0x3163, 0x3165, 0x318e, 0x3190, 0x31e3, 0x31ef, 0x321e,
  0x3220, 0x3247, 0x3250, 0x4dbf, 0x4e00, 0xa48c, 0xa490, 0xa4c6, 0xa960, 0xa97c, 0xac00, 0xd7a3,
  0xf900, 0xfaff, 0xfe10, 0xfe19, 0xfe30, 0xfe52, 0xfe54, 0xfe66, 0xfe68, 0xfe6b, 0xff01, 0xff60,
  0xffe0, 0xffe6, 0x16fe0, 0x16fe3, 0x16ff0, 0x16ff1, 0x17000, 0x187f7, 0x18800, 0x18cd5, 0x18d00,
  0x18d08, 0x1aff0, 0x1aff3, 0x1aff5, 0x1affb, 0x1affd, 0x1affe, 0x1b000, 0x1b122, 0x1b132, 0x1b132,
  0x1b150, 0x1b152, 0x1b155, 0x1b155, 0x1b164, 0x1b167, 0x1b170, 0x1b2fb, 0x1f004, 0x1f004, 0x1f0cf,
  0x1f0cf, 0x1f18e, 0x1f18e, 0x1f191, 0x1f19a, 0x1f200, 0x1f202, 0x1f210, 0x1f23b, 0x1f240, 0x1f248,
  0x1f250, 0x1f251, 0x1f260, 0x1f265, 0x1f300, 0x1f320, 0x1f32d, 0x1f335, 0x1f337, 0x1f37c, 0x1f37e,
  0x1f393, 0x1f3a0, 0x1f3ca, 0x1f3cf, 0x1f3d3, 0x1f3e0, 0x1f3f0, 0x1f3f4, 0x1f3f4, 0x1f3f8, 0x1f43e,
  0x1f440, 0x1f440, 0x1f442, 0x1f4fc, 0x1f4ff, 0x1f53d, 0x1f54b, 0x1f54e, 0x1f550, 0x1f567, 0x1f57a,
  0x1f57a, 0x1f595, 0x1f596, 0x1f5a4, 0x1f5a4, 0x1f5fb, 0x1f64f, 0x1f680, 0x1f6c5, 0x1f6cc, 0x1f6cc,
  0x1f6d0, 0x1f6d2, 0x1f6d5, 0x1f6d7, 0x1f6dc, 0x1f6df, 0x1f6eb, 0x1f6ec, 0x1f6f4, 0x1f6fc, 0x1f7e0,
  0x1f7eb, 0x1f7f0, 0x1f7f0, 0x1f90c, 0x1f93a, 0x1f93c, 0x1f945, 0x1f947, 0x1f9ff, 0x1fa70, 0x1fa7c,
  0x1fa80, 0x1fa88, 0x1fa90, 0x1fabd, 0x1fabf, 0x1fac5, 0x1face, 0x1fadb, 0x1fae0, 0x1fae8, 0x1faf0,
  0x1faf8, 0x20000, 0x2fffd, 0x30000, 0x3fffd,
];

/** Advance width of `ch` in the 16 px label font, in px. */
function glyphWidth(ch: string, fontSize: number): number {
  const cp = ch.codePointAt(0) ?? 0;
  let wide = false;
  for (let i = 0; i < WIDE_RANGES.length; i += 2) {
    if (cp < WIDE_RANGES[i]) break;
    if (cp <= WIDE_RANGES[i + 1]) {
      wide = true;
      break;
    }
  }
  return wide ? fontSize : fontSize * 0.56;
}

function estimateTextWidth(text: string, fontSize: number): number {
  let width = 0;
  for (const ch of text) width += glyphWidth(ch, fontSize);
  return width;
}

/** Whitespace the label may break at — `char::is_whitespace` minus the
 *  no-break spaces, which `break-spaces` never breaks at. */
function isBreakableSpace(ch: string): boolean {
  return (
    /[\t\n\v\f\r \u0085\u00A0\u1680\u2000-\u200A\u2028\u2029\u202F\u205F\u3000]/.test(ch) &&
    ch !== '\u00A0' &&
    ch !== '\u202F'
  );
}

/**
 * Greedy `white-space: break-spaces` wrapping of one painted line: a break
 * opportunity sits after every run of breakable whitespace and between any two
 * wide glyphs, and a single word wider than the box keeps a line to itself
 * instead of being broken (`overflow-wrap: normal`).
 */
function wrapLabelLine(line: string, maxWidth: number, fontSize: number): string[] {
  const chars = [...line];
  const segments: [number, number][] = [];
  let i = 0;
  while (i < chars.length) {
    const start = i;
    const ch = chars[i];
    i += 1;
    if (!isBreakableSpace(ch) && glyphWidth(ch, fontSize) >= fontSize) {
      // one wide glyph: breakable on either side
    } else if (!isBreakableSpace(ch)) {
      while (
        i < chars.length &&
        !isBreakableSpace(chars[i]) &&
        glyphWidth(chars[i], fontSize) < fontSize
      ) {
        i += 1;
      }
    }
    while (i < chars.length && isBreakableSpace(chars[i])) i += 1;
    segments.push([start, i]);
  }

  const lines: string[] = [];
  let from = 0;
  let width = 0;
  for (const [start, end] of segments) {
    const w = estimateTextWidth(chars.slice(start, end).join(''), fontSize);
    if (width > 0 && width + w > maxWidth) {
      lines.push(chars.slice(from, start).join(''));
      from = start;
      width = 0;
    }
    width += w;
  }
  lines.push(chars.slice(from).join(''));
  return lines;
}

/**
 * The label's own hard line breaks — a port of
 * `label_metrics::split_label_lines`, so the host paints exactly the lines the
 * engine reserved height for:
 *
 * - `<br>` in any spelling and a source `\n` end a painted line;
 * - `<p>` / `</p>` end a *block* (every other tag is transparent — `</div>`
 *   and friends must not end a line), and a break at the end of a block opens
 *   no new line — one blank per block, which is why `A<br/><br/>` paints two
 *   lines while `A</p></div>` paints one;
 * - a block with no text and no break paints nothing.
 */
function labelHardLines(markup: string): string[] {
  // Only `<br>` (attributes allowed, as `is_br_tag_body` accepts them) and
  // `<p>`/`</p>` are boundaries; every other tag is transparent — `</div>` and
  // friends must not end a line.
  const TOKEN = /(<br(?:\s[^>]*)?\/?>|<\/?p(?:\s[^>]*)?\/?>|\n)/i;
  const lines: string[] = [];
  let block: string[] = [];
  let segment = '';
  const blank = (s: string) => s.trim() === '';
  const endBlock = () => {
    if (block.length > 1 && blank(block[block.length - 1])) block.pop();
    if (!(block.length === 1 && blank(block[0]))) lines.push(...block);
    block = [];
  };
  for (const token of markup.split(TOKEN)) {
    if (/^<\/?p(?:\s[^>]*)?\/?>$/i.test(token)) {
      block.push(segment);
      segment = '';
      endBlock();
    } else if (TOKEN.test(token)) {
      block.push(segment);
      segment = '';
    } else {
      // Inline whitespace collapses; `<br>`-style breaks never reach here.
      segment += token.replace(/<[^>]+>/g, '').replace(/[^\S\n]+/g, ' ');
    }
  }
  block.push(segment);
  endBlock();
  return lines.length ? lines : [''];
}

export function normalizeSvg(xml: string): string {
  // 1. Parse the CSS rules from all <style> tags, preserving source order.
  const cssRules: CssRule[] = [];
  for (const [, cssText] of xml.matchAll(/<style\b[^>]*>([\s\S]*?)<\/style>/gi)) {
    cssRules.push(...parseCssRules(cssText));
  }
  const defaultTextFill = '#333';
  const defaultFontFamily = sanitizeCssValue('Arial, sans-serif');

  // 2. Single linear scan to inline colors: maintain an ancestor class stack,
  //    pushing on <g class> and popping on </g>; for shape/text elements,
  //    match CSS rules along the ancestor chain and merge decls in source
  //    order before writing them into the attributes.
  //    A self-closing tag's (<rect .../>) trailing / must not be swallowed
  //    into attrs, otherwise after color is added it becomes
  //    <rect .../ fill="..."> — the / lands mid-attribute and
  //    react-native-svg's parser throws, leaving the whole image blank.
  const ancestorClasses: string[][] = [];
  let out = xml.replace(
    /<(\/?)(\w+)([^>]*?)(\/?)>/g,
    (full, closing: string, tag: string, attrs: string, selfClose: string) => {
      const lower = tag.toLowerCase();
      // Closing tag: pop one `g` off the ancestor stack (only `g` is pushed; other containers don't participate in selector matching).
      if (closing) {
        if (lower === 'g' && ancestorClasses.length) ancestorClasses.pop();
        return full;
      }
      // Opening tag: push `g` first and return (it isn't inlined itself); shape/text elements are inlined then returned.
      if (lower === 'g') {
        // A self-closing <g .../> has no matching </g>, so don't push it —
        // otherwise its class would leak to sibling elements and the
        // subsequent </g> pop would be misaligned.
        if (selfClose) return full;
        const gClasses =
          attrs
            .match(/\bclass="([^"]*)"/)?.[1]
            .split(/\s+/)
            .filter(Boolean) ?? [];
        ancestorClasses.push(gClasses);
        return full;
      }
      if (!SHAPE_TAGS.test(lower)) return full;

      const classes =
        attrs
          .match(/\bclass="([^"]*)"/)?.[1]
          .split(/\s+/)
          .filter(Boolean) ?? [];
      // Merge decls from all matching rules in source order (later overrides earlier).
      const merged: CssDecls = {};
      for (const rule of cssRules) {
        if (ruleMatches(rule, lower, classes, ancestorClasses)) {
          Object.assign(merged, rule.decls);
        }
      }
      const pick = (key: ColorKey) => merged[key];
      // text elements: when fill is absent, fall back to color (CSS text-color semantics); shape elements ignore color.
      const fill = pick('fill') ?? (lower === 'text' ? pick('color') : undefined);
      const stroke = pick('stroke');
      const strokeWidth = pick('stroke-width');
      const fontFamily = pick('font-family');
      const fontSize = pick('font-size');
      // "already has attr" guards use (^|\s) rather than \b: \b also matches after the
      // hyphen in data-fill / data-stroke, which would wrongly skip real CSS inlining.
      const extra =
        (fill && !/(?:^|\s)fill=/.test(attrs) ? ` fill="${sanitizeCssValue(fill)}"` : '') +
        (stroke && !/(?:^|\s)stroke=/.test(attrs) ? ` stroke="${sanitizeCssValue(stroke)}"` : '') +
        (strokeWidth && !/(?:^|\s)stroke-width=/.test(attrs)
          ? ` stroke-width="${sanitizeCssValue(strokeWidth)}"`
          : '') +
        (fontFamily && !/(?:^|\s)font-family=/.test(attrs)
          ? ` font-family="${sanitizeCssValue(fontFamily)}"`
          : '') +
        (fontSize && !/(?:^|\s)font-size=/.test(attrs)
          ? ` font-size="${sanitizeCssValue(fontSize)}"`
          : '');
      return `<${tag}${attrs}${extra}${selfClose}>`;
    }
  );

  // 3. Give <text> elements without a fill a default color (d2's text has a
  //    style but no fill, which defaults to black).
  //    Before falling back, both style and attributes must be checked: step 2
  //    may have already inlined the class's fill/font-family into an
  //    attribute (fill="..."); in that case the style must not receive a
  //    default too — style has higher priority than attributes and would
  //    override the correct color inlined in step 2.
  // Capture an optional trailing slash so a self-closing <text .../> stays />-terminated
  // after we append style; otherwise the slash lands mid-attrs and breaks parsing.
  out = out.replace(/<text([^>]*?)(\/?)>/gi, (_m, attrs: string, slash: string) => {
    // (^|\s) guards (not \b): keep data-fill / data-font-size from masking real attributes.
    const hasFillAttr = /(?:^|\s)fill=/.test(attrs);
    const hasFontFamilyAttr = /(?:^|\s)font-family=/.test(attrs);
    const hasFontSizeAttr = /(?:^|\s)font-size=/.test(attrs);
    const styleMatch = attrs.match(/\bstyle="([^"]*)"/);
    if (!styleMatch) {
      // A text element with no style: skip if attributes already have all
      // three, otherwise fill the missing ones into style.
      const needFill = !hasFillAttr;
      const needFontFamily = !hasFontFamilyAttr;
      const needFontSize = !hasFontSizeAttr;
      if (!needFill && !needFontFamily && !needFontSize) return `<text${attrs}${slash}>`;
      const decls =
        (needFill ? `fill: ${defaultTextFill}; ` : '') +
        (needFontFamily ? `font-family: ${defaultFontFamily}; ` : '') +
        (needFontSize ? `font-size: 16px; ` : '');
      return `<text${attrs} style="${decls.trim().replace(/;$/, '')}"${slash}>`;
    }
    let style = styleMatch[1];
    // Only fill in the default when it's missing from both style and attributes.
    if (!/fill:/.test(style) && !hasFillAttr) style += `; fill: ${defaultTextFill}`;
    if (!/font-family:/.test(style) && !hasFontFamilyAttr)
      style += `; font-family: ${defaultFontFamily}`;
    if (!/font-size:/.test(style) && !hasFontSizeAttr) style += `; font-size: 16px`;
    // Function replacer: a literal string would let $& / $` / $' / $n inside the new style
    // value be interpreted as replacement patterns and corrupt it (as stripRootSvgSize does).
    return `<text${attrs.replace(/\bstyle="[^"]*"/, () => `style="${style}"`)}${slash}>`;
  });

  // 4. Remove <style> (colors are now inlined).
  out = out.replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, '');

  // 5. foreignObject → text: mermaid's node/edge labels all live inside
  //    <foreignObject>'s HTML (div/span/p); react-native-svg doesn't render
  //    foreignObject's HTML children, so the text would disappear.
  //    Convert to <text>: extract all text inside the foreignObject
  //    (<br> is first converted to a space to avoid lines running together),
  //    and center-position it using the foreignObject's width/height
  //    (x=width/2, y=height*0.7 as an approximate baseline,
  //    text-anchor=middle). foreignObject has no x/y — its position is
  //    determined by the parent <g> transform, and the converted <text>
  //    inherits the same parent transform, so the position is unchanged.
  //    A foreignObject with width=0 or no text is simply removed.
  out = out.replace(/<foreignObject\b[^>]*>[\s\S]*?<\/foreignObject>/gi, fo => {
    const w = Number(fo.match(/\bwidth="([^"]*)"/)?.[1] ?? 0);
    const h = Number(fo.match(/\bheight="([^"]*)"/)?.[1] ?? 0);
    if (!w || !h) return '';
    // Painted lines: `<br>` and the `<p>` block boundaries end a line (every
    // other tag is transparent), and each of those lines is soft-wrapped to the foreignObject's own
    // width — the box the engine sized from `flowchart.wrappingWidth`, which a
    // browser would wrap with `white-space: break-spaces` and this renderer has
    // to wrap itself. Plain text inside the foreignObject is taken regardless
    // of element (not limited to <p> — venn labels use <span>).
    // The size the label is finally painted in — the engine prefixes it onto
    // the `<div>`/`<span>` when a node carries an inline `font-size`, and the
    // box the label was measured against is in that same domain. Last match
    // wins, like the colour above (the innermost span is the label's own).
    let fontSize = 16;
    for (const sm of fo.matchAll(/style\s*=\s*"([^"]*)"/gi)) {
      const decl = sm[1].match(/(?:^|;)\s*font-size\s*:\s*([\d.]+)px/i);
      if (decl) fontSize = Number(decl[1]) || fontSize;
    }
    const hardLines = labelHardLines(fo);
    if (hardLines.every(line => !line)) return '';
    // The box height says how many lines the engine painted (one line box per
    // painted line, `font-size * 1.5` each). Only when the box is taller than
    // the source's own hard breaks did the engine soft-wrap something, and only
    // then may this renderer re-wrap: a label that fits was measured with the
    // real font metrics the engine has and this estimate does not, so
    // re-wrapping it on the estimate alone would break lines the engine left
    // whole.
    const paintedLines = Math.max(1, Math.round(h / (fontSize * 1.5)));
    const lines =
      paintedLines > hardLines.length
        ? hardLines.flatMap(line => wrapLabelLine(line, w, fontSize))
        : hardLines;
    if (lines.every(line => !line)) return '';
    const x = w / 2;
    // One <tspan> per painted line: react-native-svg draws a <text> as a
    // single line and neither breaks on newlines nor wraps. The foreignObject's
    // height covers every painted line, so dividing it by the line count gives
    // the line box — for one line that is exactly the `h * 0.7` baseline this
    // used to hardcode.
    // One line box per painted line. Flowchart labels are a clean `24 * n`, so
    // this reproduces the browser's baseline exactly; the state/note families
    // measure their first line at ~16.3 px rather than a full 24, which pulls
    // the line spacing slightly tighter than a browser's (known drift).
    const lineHeight = h / lines.length;
    const y = lineHeight * 0.7;
    const label =
      lines.length === 1
        ? lines[0]
        : lines
            .map(
              (line, i) => `<tspan x="${x}"${i > 0 ? ` dy="${lineHeight}"` : ''}>${line}</tspan>`
            )
            .join('');
    // Prefer the label's own text color, read only from a style="..." attribute (never the
    // visible label text) and only a real `color` property — the (?:^|;) boundary keeps
    // `background-color` from matching. Last match wins, i.e. the innermost span's color.
    // Strip a trailing !important: it's a cascade directive, not a valid SVG fill value.
    let labelFill = defaultTextFill;
    for (const sm of fo.matchAll(/style\s*=\s*"([^"]*)"/gi)) {
      const decl = sm[1].match(/(?:^|;)\s*color\s*:\s*([^;]+)/i);
      if (decl) labelFill = sanitizeCssValue(decl[1].replace(/\s*!important\s*$/i, '').trim());
    }
    return `<text x="${x}" y="${y}" text-anchor="middle" style="fill: ${labelFill}; font-family: ${defaultFontFamily}; font-size: ${fontSize}px">${label}</text>`;
  });

  // 6. Protect <text>, strip the xml header/comments + whitespace between
  //    tags, then restore. foreignObject was already fully converted away in
  //    step 5, so it no longer needs to be stashed.
  const preserved: string[] = [];
  const stash = (m: string) => {
    const token = `<ph data-i="${preserved.length}" />`;
    preserved.push(m);
    return token;
  };
  out = out
    .replace(/<text\b[\s\S]*?<\/text>/gi, stash)
    .replace(/<\?xml[\s\S]*?\?>/gi, '')
    .replace(/<\?[\s\S]*?\?>/g, '')
    .replace(/<!doctype[\s\S]*?>/gi, '')
    .replace(/<!--[\s\S]*?-->/g, '')
    .replace(/<title[\s\S]*?<\/title>/gi, '')
    .replace(/<desc[\s\S]*?<\/desc>/gi, '')
    .replace(/<metadata[\s\S]*?<\/metadata>/gi, '')
    .replace(/>\s+</g, '><')
    .replace(/<ph\s+data-i="(\d+)"\s*\/>/g, (_m, i: string) => preserved[Number(i)] ?? '');

  return out;
}

/**
 * Precisely removes the root <svg>'s width/height so the outer container's
 * dimensions take over rendering.
 *
 * Only matches the first <svg>: d2 outputs two nested svg layers, and the
 * inner d2-svg carries its own width/height with a viewBox that has a
 * non-zero left/top offset; if a global regex mistakenly removed the inner
 * width/height, on Android react-native-svg would use the viewBox dimensions
 * as the intrinsic size plus the offset, causing the content to be scaled up
 * and cropped down to just the top-left corner.
 *
 * The replacement uses a function form: this avoids a $ inside the root's
 * attribute values being interpreted as a replacement pattern ($& / $` / $').
 */
export function stripRootSvgSize(xml: string): string {
  const rootSvgMatch = xml.match(/<svg\b([^>]*)>/);
  if (!rootSvgMatch) return xml;
  const cleanedAttrs = rootSvgMatch[1]
    .replace(/\s+width="[^"]*"/, '')
    .replace(/\s+height="[^"]*"/, '');
  return xml.replace(rootSvgMatch[0], () => `<svg${cleanedAttrs}>`);
}
