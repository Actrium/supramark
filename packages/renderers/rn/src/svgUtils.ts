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
    for (const sel of selectorGroup.split(',').map(s => s.trim()).filter(Boolean)) {
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
function ruleMatches(rule: CssRule, tag: string, classes: string[], ancestorClasses: string[][]): boolean {
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
        const gClasses = attrs.match(/\bclass="([^"]*)"/)?.[1].split(/\s+/).filter(Boolean) ?? [];
        ancestorClasses.push(gClasses);
        return full;
      }
      if (!SHAPE_TAGS.test(lower)) return full;

      const classes = attrs.match(/\bclass="([^"]*)"/)?.[1].split(/\s+/).filter(Boolean) ?? [];
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
        (strokeWidth && !/(?:^|\s)stroke-width=/.test(attrs) ? ` stroke-width="${sanitizeCssValue(strokeWidth)}"` : '') +
        (fontFamily && !/(?:^|\s)font-family=/.test(attrs) ? ` font-family="${sanitizeCssValue(fontFamily)}"` : '') +
        (fontSize && !/(?:^|\s)font-size=/.test(attrs) ? ` font-size="${sanitizeCssValue(fontSize)}"` : '');
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
    if (!/font-family:/.test(style) && !hasFontFamilyAttr) style += `; font-family: ${defaultFontFamily}`;
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
  out = out.replace(/<foreignObject\b[^>]*>[\s\S]*?<\/foreignObject>/gi, (fo) => {
    const w = Number(fo.match(/\bwidth="([^"]*)"/)?.[1] ?? 0);
    const h = Number(fo.match(/\bheight="([^"]*)"/)?.[1] ?? 0);
    if (!w || !h) return '';
    // Collapse structural whitespace before marking line boundaries: the
    // renderer emits label HTML on a single line, and a pretty-printed
    // foreignObject must not gain a phantom line per indent. Then convert
    // <br> and block-level closing tags (</p>/</div>) to newlines: they mark
    // line boundaries, and collapsing them into spaces made
    // `Sub<br/>title` and `Sub title` render identically.
    const html = fo
      .replace(/\s+/g, ' ')
      .replace(/<br\s*\/?>/gi, '\n')
      .replace(/<\/(p|div|li|h[1-6])>/gi, '\n');
    // Every newline here is a painted line, matching the Rust side's
    // `split_label_lines`: an interior empty line stays (`a<br/><br/>b`
    // paints three lines and the foreignObject's height covers three line
    // boxes, so dropping the middle one would stretch lineHeight to h/2); a
    // trailing break opens no line (`a<br/>` is one line, and the
    // `</p>`-converted newline is structural), so trailing newlines go; a
    // leading break paints an empty first line (`<br/>a` is two), so a
    // leading newline stays.
    const text = html
      .replace(/<[^>]+>/g, '')
      .replace(/[\n ]+$/, '')
      .replace(/^ +/, '');
    if (!text) return '';
    const x = w / 2;
    // One <tspan> per line: react-native-svg draws a <text> as a single line and
    // does not break on newlines. A single line keeps the plain text it had
    // before, so nothing about one-line labels changes. The foreignObject's
    // height covers every painted line, so dividing it by the line count gives
    // the line box — for one line that is exactly the `h * 0.7` baseline this
    // used to hardcode.
    const lines = text.split('\n');
    const lineHeight = h / lines.length;
    const y = lineHeight * 0.7;
    const label =
      lines.length === 1
        ? text
        : lines
            .map((line, i) =>
              i === 0
                ? `<tspan x="${x}">${line}</tspan>`
                : `<tspan x="${x}" dy="${lineHeight}">${line}</tspan>`,
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
    return `<text x="${x}" y="${y}" text-anchor="middle" style="fill: ${labelFill}; font-family: ${defaultFontFamily}; font-size: 16px">${label}</text>`;
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
