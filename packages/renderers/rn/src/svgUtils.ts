// 统一的 SVG 预处理工具，用于将 Mermaid / MathJax 生成的 SVG
// 调整为更适合 react-native-svg 渲染的形式。

/**
 * 对已经完成样式内联的 SVG 做轻量清理。
 */
export function normalizeSvgLight(xml: string): string {
  let normalized = xml;

  normalized = normalized
    .replace(/<\?xml[\s\S]*?\?>/gi, '')
    .replace(/<\?[\s\S]*?\?>/g, '')
    .replace(/<!doctype[\s\S]*?>/gi, '')
    .replace(/<!--[\s\S]*?-->/g, '')
    .replace(/<title[\s\S]*?<\/title>/gi, '')
    .replace(/<desc[\s\S]*?<\/desc>/gi, '')
    .replace(/<metadata[\s\S]*?<\/metadata>/gi, '');

  normalized = normalized.replace(/<style[^>]*>[\s\S]*?<\/style>/gi, '');
  normalized = normalized.replace(/>\s+</g, '><');

  return normalized;
}

/**
 * 规范化 SVG：
 * - 移除 <style> 标签（react-native-svg 不支持内联 CSS）
 * - 为常见元素添加一些默认样式（主要针对 Mermaid 导出的图表）
 */
export function normalizeSvg(xml: string): string {
  let normalized = xml;

  // 先保留语义 text 节点，避免后续移除裸文本时把标签内容也删掉。
  const preservedTextNodes: string[] = [];
  normalized = normalized.replace(/<text\b[\s\S]*?<\/text>/gi, match => {
    const token = `<smtext_placeholder data-i="${preservedTextNodes.length}" />`;
    preservedTextNodes.push(match);
    return token;
  });

  normalized = normalized
    .replace(/<\?xml[\s\S]*?\?>/gi, '')
    .replace(/<\?[\s\S]*?\?>/g, '')
    .replace(/<!doctype[\s\S]*?>/gi, '')
    .replace(/<!--[\s\S]*?-->/g, '')
    .replace(/<title[\s\S]*?<\/title>/gi, '')
    .replace(/<desc[\s\S]*?<\/desc>/gi, '')
    .replace(/<metadata[\s\S]*?<\/metadata>/gi, '');

  normalized = normalized.replace(/>\s+</g, '><');
  normalized = normalized.replace(/>[^<]+</g, '><');

  const styleMatch = normalized.match(/<style[^>]*>([\s\S]*?)<\/style>/i);
  let defaultTextFill = '#333';
  let defaultFontFamily = 'Arial, sans-serif';
  let defaultFontSize = '16px';
  let defaultNodeFill = '#ECECFF';
  let defaultStroke = '#9370DB';
  let defaultStrokeWidth = '1px';

  if (styleMatch) {
    const styleContent = styleMatch[1];

    const textFillMatch = styleContent.match(/\.label[^}]*fill\s*:\s*([^;}\s]+)/);
    if (textFillMatch) defaultTextFill = textFillMatch[1];

    const fontFamilyMatch = styleContent.match(/font-family\s*:\s*([^;}\n]+)/);
    if (fontFamilyMatch) defaultFontFamily = fontFamilyMatch[1];

    const fontSizeMatch = styleContent.match(/font-size\s*:\s*([^;}\s]+)/);
    if (fontSizeMatch) defaultFontSize = fontSizeMatch[1];

    const nodeFillMatch = styleContent.match(/\.node\s+rect[^}]*fill\s*:\s*([^;}\s]+)/);
    if (nodeFillMatch) defaultNodeFill = nodeFillMatch[1];

    const strokeMatch = styleContent.match(/\.node\s+rect[^}]*stroke\s*:\s*([^;}\s]+)/);
    if (strokeMatch) defaultStroke = strokeMatch[1];

    const strokeWidthMatch = styleContent.match(/\.node\s+rect[^}]*stroke-width\s*:\s*([^;}\s]+)/);
    if (strokeWidthMatch) defaultStrokeWidth = strokeWidthMatch[1];
  }

  normalized = normalized.replace(/<style[^>]*>[\s\S]*?<\/style>/gi, '');

  normalized = normalized.replace(
    /<text([^>]*?)style="([^"]*)"/gi,
    (_match: string, attrs: string, existingStyle: string) => {
      const styles: string[] = [];
      if (existingStyle && existingStyle.trim()) styles.push(existingStyle.trim());

      if (!existingStyle.includes('fill:')) styles.push(`fill: ${defaultTextFill}`);
      if (!existingStyle.includes('font-family:')) styles.push(`font-family: ${defaultFontFamily}`);
      if (!existingStyle.includes('font-size:')) styles.push(`font-size: ${defaultFontSize}`);

      const attrsWithSpace = attrs && attrs.trim() ? attrs + ' ' : attrs;
      return `<text${attrsWithSpace}style="${styles.join('; ')}"`;
    }
  );

  normalized = normalized.replace(
    /<rect([^>]*?)style="([^"]*)"/gi,
    (_match: string, attrs: string, existingStyle: string) => {
      const styles: string[] = [];
      if (existingStyle && existingStyle.trim()) styles.push(existingStyle.trim());

      if (!existingStyle?.includes('fill:')) styles.push(`fill: ${defaultNodeFill}`);
      if (!existingStyle?.includes('stroke:')) styles.push(`stroke: ${defaultStroke}`);
      if (!existingStyle?.includes('stroke-width:')) {
        styles.push(`stroke-width: ${defaultStrokeWidth}`);
      }

      const attrsWithSpace = attrs && attrs.trim() ? attrs + ' ' : attrs;
      return `<rect${attrsWithSpace}style="${styles.join('; ')}"`;
    }
  );

  normalized = normalized.replace(
    /<path\s+([^>]*?)style="([^"]*)"/gi,
    (_match: string, attrs: string, existingStyle: string) => {
      const styles: string[] = [];
      if (existingStyle && existingStyle.trim()) {
        styles.push(existingStyle.trim());
      }

      if (!existingStyle?.includes('fill:')) {
        styles.push(`fill: ${defaultStroke}`);
      }
      if (!existingStyle?.includes('stroke:') && !existingStyle?.includes('stroke-width:')) {
        styles.push(`stroke: ${defaultStroke}`);
      }

      return `<path ${attrs}style="${styles.join('; ')}"`;
    }
  );

  normalized = normalized.replace(
    /<smtext_placeholder\s+data-i="(\d+)"\s*\/>/g,
    (_match, indexText) => preservedTextNodes[Number(indexText)] ?? ''
  );

  return normalized;
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
