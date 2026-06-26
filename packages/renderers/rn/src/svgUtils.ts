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

type ColorKey = 'fill' | 'stroke' | 'stroke-width' | 'font-family' | 'font-size';
type CssDecls = Partial<Record<ColorKey, string>>;

const COLOR_ATTRS_RE = /^(fill|stroke|stroke-width|font-family|font-size)$/;

/**
 * 解析 <style> 里的 CSS 规则，按选择器末尾的 class/标签名索引。
 *
 * mermaid/d2 的 CSS 多为 scoped：`#id .node rect { fill:#ECECFF; stroke:#9370DB; }`。
 * react-native-svg 不支持 CSS 选择器，需要把颜色内联到元素属性。这里按末尾选择器
 *（如 `#id .node rect` 取 `rect`、`.label-container` 取 `label-container`）建索引，
 * 后续按元素 class 或标签名匹配内联。同 key 多规则后者覆盖前者。
 */
function parseCssRules(cssText: string): Map<string, CssDecls> {
  const rules = new Map<string, CssDecls>();
  for (const [, selectorGroup, body] of cssText.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const decls: CssDecls = {};
    for (const part of body.split(';')) {
      const idx = part.indexOf(':');
      if (idx <= 0) continue;
      const key = part.slice(0, idx).trim();
      const value = part.slice(idx + 1).trim();
      if (COLOR_ATTRS_RE.test(key)) decls[key as ColorKey] = value;
    }
    if (Object.keys(decls).length === 0) continue;
    for (const sel of selectorGroup.split(',').map(s => s.trim()).filter(Boolean)) {
      const parts = sel.split(/\s+/).filter(Boolean);
      const last = parts[parts.length - 1] ?? sel;
      const key = last.startsWith('.') ? last.slice(1) : last;
      rules.set(key, { ...(rules.get(key) ?? {}), ...decls });
    }
  }
  return rules;
}

/** 双引号转单引号，避免拼进 style="..." 产生嵌套双引号（d2 font-family "d2-<hash>-font-bold"）。 */
const sanitizeCssValue = (v: string): string => v.replace(/"/g, "'");

/**
 * 规范化 SVG（用于 mermaid / d2）：
 * 1. 解析 <style> 的 CSS 规则，把 class 选择器的 fill/stroke 内联到 rect/path 等元素属性
 *    ——否则删 <style> 后元素无颜色源，react-native-svg 默认黑色填充。
 * 2. 删除 <style>（react-native-svg 不支持 CSS 选择器）。
 * 3. 保护 <text> / <foreignObject>，再用安全正则删标签间空白——原版 />[^<]+</ 会误删
 *    rect 的 class/style 属性串和 foreignObject 的标签文字。
 */
export function normalizeSvg(xml: string): string {
  // 1. 解析 CSS 规则，提取默认色 + class → decls 映射。
  const cssRules = new Map<string, CssDecls>();
  for (const [, cssText] of xml.matchAll(/<style\b[^>]*>([\s\S]*?)<\/style>/gi)) {
    for (const [key, decls] of parseCssRules(cssText)) {
      cssRules.set(key, { ...(cssRules.get(key) ?? {}), ...decls });
    }
  }
  const defaultTextFill = '#333';
  const defaultFontFamily = sanitizeCssValue('Arial, sans-serif');

  // 2. 内联 class 颜色到形状元素 fill/stroke 属性（按 class + 标签名匹配）。
  const inlineColors = (tag: string, attrs: string): string => {
    const classes = attrs.match(/\bclass="([^"]*)"/)?.[1].split(/\s+/).filter(Boolean) ?? [];
    const matched = [...classes, tag]
      .map(k => cssRules.get(k))
      .filter((d): d is CssDecls => Boolean(d));
    const pick = (key: ColorKey) => matched.find(d => d[key])?.[key];
    const fill = pick('fill');
    const stroke = pick('stroke');
    const strokeWidth = pick('stroke-width');
    const extra =
      (fill && !/\bfill=/.test(attrs) ? ` fill="${sanitizeCssValue(fill)}"` : '') +
      (stroke && !/\bstroke=/.test(attrs) ? ` stroke="${sanitizeCssValue(stroke)}"` : '') +
      (strokeWidth && !/\bstroke-width=/.test(attrs) ? ` stroke-width="${sanitizeCssValue(strokeWidth)}"` : '');
    return `<${tag}${attrs}${extra}>`;
  };
  let out = xml.replace(/<(\w+)((?:[^>]*?))>/g, (full, tag: string, attrs: string) =>
    /^(rect|path|circle|ellipse|polygon)$/.test(tag) ? inlineColors(tag, attrs) : full
  );

  // 3. 给 <text> 补 inline 默认色（d2 的 text 有 style 但无 fill，会默认黑色）。
  out = out.replace(/<text([^>]*?)style="([^"]*)"/gi, (_m, attrs: string, style: string) => {
    const extra =
      (style.includes('fill:') ? '' : `; fill: ${defaultTextFill}`) +
      (style.includes('font-family:') ? '' : `; font-family: ${defaultFontFamily}`) +
      (style.includes('font-size:') ? '' : `; font-size: 16px`);
    return `<text${attrs}style="${style}${extra}"`;
  });

  // 4. 删除 <style>（颜色已内联）。
  out = out.replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, '');

  // 5. foreignObject → text：mermaid 的节点/连线标签全在 <foreignObject> 的 HTML 里
  //    （div/span/p），react-native-svg 不渲染 foreignObject 的 HTML 子节点，文字会消失。
  //    转换成 <text>：提取 <p> 里的纯文本，用 foreignObject 的 width/height 居中定位
  //    （x=width/2, y=height*0.7 近似基线，text-anchor=middle）。foreignObject 无 x/y，
  //    位置由父 <g> transform 决定，转换后的 <text> 继承同样的父 transform，位置不变。
  //    width=0 或无文本的 foreignObject（空 edgeLabel 占位）直接删除。
  out = out.replace(/<foreignObject\b[^>]*>[\s\S]*?<\/foreignObject>/gi, (fo) => {
    const w = Number(fo.match(/\bwidth="([^"]*)"/)?.[1] ?? 0);
    const h = Number(fo.match(/\bheight="([^"]*)"/)?.[1] ?? 0);
    if (!w || !h) return '';
    // 提取所有 <p>...</p> 里的文本，拼接（mermaid 多行标签少见，多数单个 <p>）
    const texts = [...fo.matchAll(/<p\b[^>]*>([\s\S]*?)<\/p>/gi)]
      .map(m => m[1].replace(/<[^>]+>/g, '').trim())
      .filter(Boolean);
    if (texts.length === 0) return '';
    const text = texts.join(' ');
    const x = w / 2;
    const y = h * 0.7;
    return `<text x="${x}" y="${y}" text-anchor="middle" style="fill: ${defaultTextFill}; font-family: ${defaultFontFamily}; font-size: 16px">${text}</text>`;
  });

  // 6. 保护 <text> / <foreignObject>，删 xml 头/注释 + 标签间空白，再恢复。
  const preserved: string[] = [];
  const stash = (m: string) => {
    const token = `<ph data-i="${preserved.length}" />`;
    preserved.push(m);
    return token;
  };
  out = out
    .replace(/<text\b[\s\S]*?<\/text>/gi, stash)
    .replace(/<foreignObject\b[\s\S]*?<\/foreignObject>/gi, stash)
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
