import { parse } from '../src/plugin';
import type {
  SupramarkNode,
  SupramarkTextNode,
  SupramarkParentNode,
  SupramarkHeadingNode,
  SupramarkCodeNode,
  SupramarkListNode,
  SupramarkListItemNode,
  SupramarkDiagramNode,
} from '../src/ast';

describe('parse', () => {
  describe('AST v2 合同', () => {
    it('应该通过 parse facade 输出 root v2 信息', async () => {
      const ast = await parse('# 标题 😄');

      expect(ast.type).toBe('root');
      expect(ast.ast_version).toBe(2);
      expect(ast.diagnostics).toEqual([]);
      expect(ast.parser?.name).toBe('supramark-markdown');
      expect(ast.position?.start.utf16_offset).toBe(0);
      expect(ast.position?.end.utf16_offset).toBe('# 标题 😄'.length);
      expect(ast.position?.end.byte_offset).toBe(13);
    });

    it('应该省略普通列表项的 v2 可选字段', async () => {
      const ast = await parse('- plain item');
      const list = ast.children[0] as SupramarkListNode;
      const item = list.children[0] as SupramarkListItemNode;

      expect(list.type).toBe('list');
      expect(Object.prototype.hasOwnProperty.call(list, 'start')).toBe(false);
      expect(Object.prototype.hasOwnProperty.call(item, 'checked')).toBe(false);
    });

    it('应该输出 definition list v2 children 结构', async () => {
      const ast = await parse('Term\n:   Definition');
      const list = ast.children[0] as SupramarkParentNode;
      const item = list.children[0] as SupramarkParentNode;

      expect(list.type).toBe('definition_list');
      expect(item.type).toBe('definition_item');
      expect(Object.prototype.hasOwnProperty.call(item, 'term')).toBe(false);
      expect(Object.prototype.hasOwnProperty.call(item, 'descriptions')).toBe(false);
      expect(item.children[0].type).toBe('definition_term');
      expect(((item.children[0] as SupramarkParentNode).children[0] as SupramarkTextNode).value).toBe('Term');
      expect(item.children[1].type).toBe('definition_description');
      expect((item.children[1] as SupramarkParentNode).children[0].type).toBe('paragraph');
    });
  });

  describe('基础 Markdown 解析', () => {
    it('应该解析段落', async () => {
      const markdown = 'This is a paragraph.';
      const ast = await parse(markdown);

      expect(ast.type).toBe('root');
      expect(ast.children).toHaveLength(1);
      expect(ast.children[0].type).toBe('paragraph');
    });

    it('应该解析标题', async () => {
      const markdown = '# Heading 1\n## Heading 2';
      const ast = await parse(markdown);

      expect(ast.children).toHaveLength(2);
      expect(ast.children[0].type).toBe('heading');
      expect((ast.children[0] as SupramarkHeadingNode).depth).toBe(1);
      expect(ast.children[1].type).toBe('heading');
      expect((ast.children[1] as SupramarkHeadingNode).depth).toBe(2);
    });

    it('应该解析列表', async () => {
      const markdown = '- Item 1\n- Item 2\n- Item 3';
      const ast = await parse(markdown);

      expect(ast.children).toHaveLength(1);
      expect(ast.children[0].type).toBe('list');
      expect((ast.children[0] as SupramarkListNode).children).toHaveLength(3);
    });

    it('应该解析代码块', async () => {
      const markdown = '```javascript\nconst x = 1;\n```';
      const ast = await parse(markdown);

      expect(ast.children).toHaveLength(1);
      expect(ast.children[0].type).toBe('code');
      expect((ast.children[0] as SupramarkCodeNode).lang).toBe('javascript');
    });
  });

  describe('Inline 元素解析', () => {
    it('应该解析粗体文本', async () => {
      const markdown = 'This is **bold** text.';
      const ast = await parse(markdown);

      const paragraph = ast.children[0] as SupramarkParentNode;
      expect(paragraph.children.some((node: SupramarkNode) => node.type === 'strong')).toBe(true);
    });

    it('应该解析斜体文本', async () => {
      const markdown = 'This is *italic* text.';
      const ast = await parse(markdown);

      const paragraph = ast.children[0] as SupramarkParentNode;
      expect(paragraph.children.some((node: SupramarkNode) => node.type === 'emphasis')).toBe(true);
    });

    it('应该解析链接', async () => {
      const markdown = '[Link](https://example.com)';
      const ast = await parse(markdown);

      const paragraph = ast.children[0] as SupramarkParentNode;
      expect(paragraph.children.some((node: SupramarkNode) => node.type === 'link')).toBe(true);
    });

    it('应该解析行内代码', async () => {
      const markdown = 'This is `code` inline.';
      const ast = await parse(markdown);

      const paragraph = ast.children[0] as SupramarkParentNode;
      expect(paragraph.children.some((node: SupramarkNode) => node.type === 'inline_code')).toBe(true);
    });
  });

  describe('GFM 扩展', () => {
    it('应该解析删除线', async () => {
      const markdown = 'This is ~~deleted~~ text.';
      const ast = await parse(markdown);

      const paragraph = ast.children[0] as SupramarkParentNode;
      expect(paragraph.children.some((node: SupramarkNode) => node.type === 'delete')).toBe(true);
    });

    it('应该解析任务列表', async () => {
      const markdown = '- [x] Task 1\n- [ ] Task 2';
      const ast = await parse(markdown);

      const list = ast.children[0] as SupramarkListNode;
      expect(list.type).toBe('list');
      expect((list.children[0] as SupramarkListItemNode).checked).toBe(true);
      expect((list.children[1] as SupramarkListItemNode).checked).toBe(false);
    });

    it('应该解析表格', async () => {
      const markdown = '| Header 1 | Header 2 |\n|----------|----------|\n| Cell 1   | Cell 2   |';
      const ast = await parse(markdown);

      expect(ast.children).toHaveLength(1);
      expect(ast.children[0].type).toBe('table');
    });
  });

  describe('图表节点', () => {
    it('应该解析 mermaid 代码块为 diagram 节点', async () => {
      const markdown = '```mermaid\ngraph TD;\n  A-->B;\n```';
      const ast = await parse(markdown);

      expect(ast.children).toHaveLength(1);
      expect(ast.children[0].type).toBe('diagram');
      expect((ast.children[0] as SupramarkDiagramNode).engine).toBe('mermaid');
    });

    it('应该解析 plantuml 代码块为 diagram 节点', async () => {
      const markdown = '```plantuml\n@startuml\nA -> B\n@enduml\n```';
      const ast = await parse(markdown);

      expect(ast.children).toHaveLength(1);
      expect(ast.children[0].type).toBe('diagram');
      expect((ast.children[0] as SupramarkDiagramNode).engine).toBe('plantuml');
    });
  });

  describe('Math 节点', () => {
    it('应该解析行内公式', async () => {
      const markdown = 'Inline math: $E = mc^2$';
      const ast = await parse(markdown);

      const paragraph = ast.children[0] as SupramarkParentNode;
      expect(paragraph.children.some((node: SupramarkNode) => node.type === 'math_inline')).toBe(true);
    });

    it('应该解析块级公式', async () => {
      const markdown = '$$\n\\int_0^1 x^2 dx\n$$';
      const ast = await parse(markdown);

      expect(ast.children.some((node: SupramarkNode) => node.type === 'math_block')).toBe(true);
    });
  });

  describe('复杂文档', () => {
    it('应该解析包含多种元素的文档', async () => {
      const markdown = `# Title

This is a **paragraph** with *italic* and [link](https://example.com).

- Item 1
- Item 2

\`\`\`javascript
const x = 1;
\`\`\`

| Header |
|--------|
| Cell   |
`;

      const ast = await parse(markdown);

      // 验证包含多种节点类型
      expect(ast.children.length).toBeGreaterThan(1);
      expect(ast.children.some(node => node.type === 'heading')).toBe(true);
      expect(ast.children.some(node => node.type === 'paragraph')).toBe(true);
      expect(ast.children.some(node => node.type === 'list')).toBe(true);
      expect(ast.children.some(node => node.type === 'code')).toBe(true);
      expect(ast.children.some(node => node.type === 'table')).toBe(true);
    });
  });

  describe('空输入处理', () => {
    it('应该处理空字符串', async () => {
      const ast = await parse('');
      expect(ast.type).toBe('root');
      expect(ast.children).toHaveLength(0);
    });

    it('应该处理只有空白的字符串', async () => {
      const ast = await parse('   \n\n   ');
      expect(ast.type).toBe('root');
      expect(ast.children).toHaveLength(0);
    });
  });
});
