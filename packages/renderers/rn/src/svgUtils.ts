// 统一的 SVG 预处理工具，用于将 Mermaid / d2 生成的 SVG
// 调整为更适合 react-native-svg 渲染的形式。

/**
 * 轻量清理：用于已经完成样式内联、无需颜色处理的 SVG（如 MathJax）。
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

// color: 在 CSS 语义里只设置文本颜色，对 rect/path 的 fill 无影响。这里单独收存，
// inlineColors 时仅对 text 元素把 color 当 fill 候选——避免 .box{fill:blue;color:red}
// 把 rect 染成 red（应是 blue）。
const DECL_KEY_MAP: Record<string, ColorKey | undefined> = {
  fill: 'fill',
  stroke: 'stroke',
  'stroke-width': 'stroke-width',
  'font-family': 'font-family',
  'font-size': 'font-size',
  color: 'color',
};

// 选择器的一段，如 `.node rect` → { tag:'rect', classes:['node'] }，`rect.divider` → { tag:'rect', classes:['divider'] }。
type SelectorPart = { tag: string | null; classes: string[] };
type CssRule = { selector: SelectorPart[]; decls: CssDecls };

/**
 * 解析 <style> 里的 CSS 规则。选择器按「祖先→自身」存为完整链，保留源序。
 *
 * mermaid/d2 的 CSS 多为 scoped：`#id .node rect { fill:#ECECFF }`。
 * react-native-svg 不支持 CSS 选择器，需要把颜色内联到元素属性。
 * 旧实现按选择器末段建扁平 key，导致 `.node rect` 与 `.cluster rect` 都塌缩成 `rect`
 * 互相覆盖。这里改为保留完整祖先链，匹配时按元素的 class + 祖先 class 链逐条比对。
 *
 * 同选择器后写覆盖先写（CSS 源序即优先级，mermaid 输出已按特异性排好序，无需算权重）。
 * `!important` 在此剥离——内联成属性值后是非法语法，会失效。
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
      // 剥 !important：内联属性值不支持，原样拷进去会让属性值非法（fill="#333 !important"）。
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

/** 解析单条选择器为祖先→自身的段链。忽略 id 与伪类；复合选择器 `.a.b rect` 合并 classes。 */
function parseSelector(sel: string): SelectorPart[] {
  const parts: SelectorPart[] = [];
  for (const chunk of sel.split(/\s+/).filter(Boolean)) {
    // 纯 id 选择器（#m1）不产生约束段——mermaid scoped id 不参与内联匹配，跳过避免污染段链。
    if (chunk.startsWith('#')) continue;
    // 处理形如 rect.divider / .a.b tag —— 同一复合选择器内可能 tag 与多个 class 并存。
    const classes: string[] = [];
    let tag: string | null = null;
    for (const token of chunk.split(/(?=\.)/)) {
      const t = token.trim();
      if (!t) continue;
      if (t.startsWith('.')) classes.push(t.slice(1));
      else if (t.startsWith(':')) {
        /* 伪类忽略 */
      } else tag = t.toLowerCase();
    }
    parts.push({ tag, classes });
  }
  return parts;
}

/**
 * 规则是否匹配某元素：从选择器末段（自身）往前比对祖先栈顶。
 * 末段 tag/classes 必须匹配当前元素；其余段从栈顶往下逐一匹配祖先 g 的 class。
 * 自身段与祖先段都用数组 Array.includes 按词精确匹配——祖先栈存数组而非 join 字符串，
 * 避免字符串 String.includes 子串匹配把 .node 误命中 nodes / .label 误命中 edgeLabel。
 */
function ruleMatches(rule: CssRule, tag: string, classes: string[], ancestorClasses: string[][]): boolean {
  const parts = rule.selector;
  const self = parts[parts.length - 1];
  if (self.tag && self.tag !== tag) return false;
  if (self.classes.length && !self.classes.every(c => classes.includes(c))) return false;
  // 剩余段是祖先约束，从栈顶（最近祖先）往下匹配。
  let ancIdx = ancestorClasses.length - 1;
  for (let i = parts.length - 2; i >= 0; i--) {
    const anc = parts[i];
    let found = false;
    while (ancIdx >= 0) {
      const ancCls: string[] = ancestorClasses[ancIdx];
      ancIdx--;
      // 祖先 class 按词精确匹配（Array.includes），与自身段语义一致。
      if (anc.classes.length && anc.classes.every(c => ancCls.includes(c))) {
        found = true;
        break;
      }
    }
    if (!found) return false;
  }
  return true;
}

/** 双引号转单引号，避免拼进 style="..." 产生嵌套双引号（d2 font-family "d2-<hash>-font-bold"）。 */
const sanitizeCssValue = (v: string): string => v.replace(/"/g, "'");

// line / polyline are stroke-only; the fill logic below stays conservative and only
// emits a fill when the CSS supplies one (e.g. fill:none), never a spurious default.
const SHAPE_TAGS = /^(rect|path|circle|ellipse|polygon|line|polyline|text)$/;

/**
 * 规范化 SVG（用于 mermaid / d2）：
 * 1. 解析 <style> 的 CSS 规则，把 class 选择器的 fill/stroke 内联到形状/text 元素属性
 *    ——否则删 <style> 后元素无颜色源，react-native-svg 默认黑色填充。
 * 2. 删除 <style>（react-native-svg 不支持 CSS 选择器）。
 * 3. foreignObject → text（react-native-svg 不渲染其 HTML 子节点）。
 * 4. 删 xml 头/注释 + 标签间空白，保护 text/foreignObject 内文本不被误删。
 */
export function normalizeSvg(xml: string): string {
  // 1. 解析所有 <style> 的 CSS 规则，保留源序。
  const cssRules: CssRule[] = [];
  for (const [, cssText] of xml.matchAll(/<style\b[^>]*>([\s\S]*?)<\/style>/gi)) {
    cssRules.push(...parseCssRules(cssText));
  }
  const defaultTextFill = '#333';
  const defaultFontFamily = sanitizeCssValue('Arial, sans-serif');

  // 2. 单次线性扫描内联颜色：维护祖先 class 栈，遇 <g class> push、</g> pop，
  //    对形状/文本元素用祖先链匹配 CSS 规则，按源序合并 decls 后补进属性。
  //    自闭合标签（<rect .../>）的结尾 / 不能被吞进 attrs，否则补色后变成
  //    <rect .../ fill="..."> —— / 落在属性中间，react-native-svg 解析抛错整图空白。
  const ancestorClasses: string[][] = [];
  let out = xml.replace(
    /<(\/?)(\w+)([^>]*?)(\/?)>/g,
    (full, closing: string, tag: string, attrs: string, selfClose: string) => {
      const lower = tag.toLowerCase();
      // 闭标签：从祖先栈弹出一个 g（仅 g 入栈，其它容器不参与选择器匹配）。
      if (closing) {
        if (lower === 'g' && ancestorClasses.length) ancestorClasses.pop();
        return full;
      }
      // 开标签：g 先入栈再返回（自身不内联），形状/text 内联后返回。
      if (lower === 'g') {
        // 自闭合 <g .../> 无对应 </g>，不入栈——否则 class 泄漏给兄弟元素且后续 </g> 弹栈错位。
        if (selfClose) return full;
        const gClasses = attrs.match(/\bclass="([^"]*)"/)?.[1].split(/\s+/).filter(Boolean) ?? [];
        ancestorClasses.push(gClasses);
        return full;
      }
      if (!SHAPE_TAGS.test(lower)) return full;

      const classes = attrs.match(/\bclass="([^"]*)"/)?.[1].split(/\s+/).filter(Boolean) ?? [];
      // 按源序合并所有命中规则的 decls（后写覆盖先写）。
      const merged: CssDecls = {};
      for (const rule of cssRules) {
        if (ruleMatches(rule, lower, classes, ancestorClasses)) {
          Object.assign(merged, rule.decls);
        }
      }
      const pick = (key: ColorKey) => merged[key];
      // text 元素：fill 缺省时回退到 color（CSS 文本颜色语义）；形状元素忽略 color。
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

  // 3. 给无 fill 的 <text> 兜底默认色（d2 的 text 有 style 但无 fill，会默认黑色）。
  //    兜底前必须同时检查 style 和属性：step-2 可能已把 class 的 fill/font-family 内联成
  //    属性（fill="..."），此时不能再往 style 补默认值——style 优先级高于属性，会覆盖掉
  //    step-2 内联的正确颜色。
  // Capture an optional trailing slash so a self-closing <text .../> stays />-terminated
  // after we append style; otherwise the slash lands mid-attrs and breaks parsing.
  out = out.replace(/<text([^>]*?)(\/?)>/gi, (_m, attrs: string, slash: string) => {
    // (^|\s) guards (not \b): keep data-fill / data-font-size from masking real attributes.
    const hasFillAttr = /(?:^|\s)fill=/.test(attrs);
    const hasFontFamilyAttr = /(?:^|\s)font-family=/.test(attrs);
    const hasFontSizeAttr = /(?:^|\s)font-size=/.test(attrs);
    const styleMatch = attrs.match(/\bstyle="([^"]*)"/);
    if (!styleMatch) {
      // 无 style 的 text：属性已有全部三者就不补，否则补缺的到 style。
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
    // style 里缺、且属性里也没有时才补默认值。
    if (!/fill:/.test(style) && !hasFillAttr) style += `; fill: ${defaultTextFill}`;
    if (!/font-family:/.test(style) && !hasFontFamilyAttr) style += `; font-family: ${defaultFontFamily}`;
    if (!/font-size:/.test(style) && !hasFontSizeAttr) style += `; font-size: 16px`;
    // Function replacer: a literal string would let $& / $` / $' / $n inside the new style
    // value be interpreted as replacement patterns and corrupt it (as stripRootSvgSize does).
    return `<text${attrs.replace(/\bstyle="[^"]*"/, () => `style="${style}"`)}${slash}>`;
  });

  // 4. 删除 <style>（颜色已内联）。
  out = out.replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, '');

  // 5. foreignObject → text：mermaid 的节点/连线标签全在 <foreignObject> 的 HTML 里
  //    （div/span/p），react-native-svg 不渲染 foreignObject 的 HTML 子节点，文字会消失。
  //    转换成 <text>：提取 foreignObject 内全部文本（<br> 先转空格避免行粘连），用
  //    foreignObject 的 width/height 居中定位（x=width/2, y=height*0.7 近似基线，
  //    text-anchor=middle）。foreignObject 无 x/y，位置由父 <g> transform 决定，转换后的
  //    <text> 继承同样的父 transform，位置不变。width=0 或无文本的 foreignObject 直接删除。
  out = out.replace(/<foreignObject\b[^>]*>[\s\S]*?<\/foreignObject>/gi, (fo) => {
    const w = Number(fo.match(/\bwidth="([^"]*)"/)?.[1] ?? 0);
    const h = Number(fo.match(/\bheight="([^"]*)"/)?.[1] ?? 0);
    if (!w || !h) return '';
    // <br> 和块级闭合标签（</p>/</div>）先转空格作为行/块边界，避免行粘连；再剥其余标签，
    // 取 foreignObject 内全部纯文本（不限 <p>，venn 标签是 <span>）。
    const html = fo.replace(/<br\s*\/?>/gi, ' ').replace(/<\/(p|div|li|h[1-6])>/gi, ' ');
    const text = html.replace(/<[^>]+>/g, '').replace(/\s+/g, ' ').trim();
    if (!text) return '';
    const x = w / 2;
    const y = h * 0.7;
    // Prefer the label's own text color, read only from a style="..." attribute (never the
    // visible label text) and only a real `color` property — the (?:^|;) boundary keeps
    // `background-color` from matching. Last match wins, i.e. the innermost span's color.
    // Strip a trailing !important: it's a cascade directive, not a valid SVG fill value.
    let labelFill = defaultTextFill;
    for (const sm of fo.matchAll(/style\s*=\s*"([^"]*)"/gi)) {
      const decl = sm[1].match(/(?:^|;)\s*color\s*:\s*([^;]+)/i);
      if (decl) labelFill = sanitizeCssValue(decl[1].replace(/\s*!important\s*$/i, '').trim());
    }
    return `<text x="${x}" y="${y}" text-anchor="middle" style="fill: ${labelFill}; font-family: ${defaultFontFamily}; font-size: 16px">${text}</text>`;
  });

  // 6. 保护 <text>，删 xml 头/注释 + 标签间空白，再恢复。
  //    foreignObject 在 step-5 已全部转走，无需再 stash。
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
 * 精确删除根 <svg> 的 width/height，让外层容器的尺寸接管渲染。
 *
 * 只匹配第一个 <svg>：d2 输出两层嵌套 svg，内层 d2-svg 自带 width/height 且
 * viewBox 带非零 left/top 偏移；若用全局正则误删内层 width/height，Android 上
 * react-native-svg 会用 viewBox 维度当固有尺寸并叠加偏移，内容放大裁切只剩左上角。
 *
 * replacement 用函数式：避免根属性值里的 $ 被当作替换模式（$& / $` / $'）解释。
 */
export function stripRootSvgSize(xml: string): string {
  const rootSvgMatch = xml.match(/<svg\b([^>]*)>/);
  if (!rootSvgMatch) return xml;
  const cleanedAttrs = rootSvgMatch[1]
    .replace(/\s+width="[^"]*"/, '')
    .replace(/\s+height="[^"]*"/, '');
  return xml.replace(rootSvgMatch[0], () => `<svg${cleanedAttrs}>`);
}
