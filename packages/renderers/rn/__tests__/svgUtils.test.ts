import { describe, test, expect } from 'bun:test';
import { stripRootSvgSize } from '../src/svgUtils';

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
