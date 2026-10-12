import { parse } from '../src/plugin';
import { CJK_HEADING_SAMPLE } from './fixtures/cjk-samples';
import type {
  SupramarkNode,
  SupramarkTextNode,
  SupramarkParentNode,
  SupramarkHeadingNode,
  SupramarkCodeNode,
  SupramarkListNode,
  SupramarkListItemNode,
  SupramarkDiagramNode,
  SupramarkMathInlineNode,
  SupramarkMathBlockNode,
} from '../src/ast';

describe('parse', () => {
  describe('AST v2 contract', () => {
    it('emits root v2 info via the parse facade', async () => {
      const ast = await parse(CJK_HEADING_SAMPLE);

      expect(ast.type).toBe('root');
      expect(ast.ast_version).toBe(2);
      expect(ast.diagnostics).toEqual([]);
      expect(ast.parser?.name).toBe('supramark-markdown');
      expect(ast.position?.start.utf16_offset).toBe(0);
      expect(ast.position?.end.utf16_offset).toBe(CJK_HEADING_SAMPLE.length);
      expect(ast.position?.end.byte_offset).toBe(13);
    });

    it('omits v2 optional fields for a plain list item', async () => {
      const ast = await parse('- plain item');
      const list = ast.children[0] as SupramarkListNode;
      const item = list.children[0] as SupramarkListItemNode;

      expect(list.type).toBe('list');
      expect(Object.prototype.hasOwnProperty.call(list, 'start')).toBe(false);
      expect(Object.prototype.hasOwnProperty.call(item, 'checked')).toBe(false);
    });

    it('emits the definition list v2 children structure', async () => {
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

  describe('basic Markdown parsing', () => {
    it('parses a paragraph', async () => {
      const markdown = 'This is a paragraph.';
      const ast = await parse(markdown);

      expect(ast.type).toBe('root');
      expect(ast.children).toHaveLength(1);
      expect(ast.children[0].type).toBe('paragraph');
    });

    it('parses a heading', async () => {
      const markdown = '# Heading 1\n## Heading 2';
      const ast = await parse(markdown);

      expect(ast.children).toHaveLength(2);
      expect(ast.children[0].type).toBe('heading');
      expect((ast.children[0] as SupramarkHeadingNode).depth).toBe(1);
      expect(ast.children[1].type).toBe('heading');
      expect((ast.children[1] as SupramarkHeadingNode).depth).toBe(2);
    });

    it('parses a list', async () => {
      const markdown = '- Item 1\n- Item 2\n- Item 3';
      const ast = await parse(markdown);

      expect(ast.children).toHaveLength(1);
      expect(ast.children[0].type).toBe('list');
      expect((ast.children[0] as SupramarkListNode).children).toHaveLength(3);
    });

    it('parses a code block', async () => {
      const markdown = '```javascript\nconst x = 1;\n```';
      const ast = await parse(markdown);

      expect(ast.children).toHaveLength(1);
      expect(ast.children[0].type).toBe('code');
      expect((ast.children[0] as SupramarkCodeNode).lang).toBe('javascript');
    });
  });

  describe('inline element parsing', () => {
    it('parses bold text', async () => {
      const markdown = 'This is **bold** text.';
      const ast = await parse(markdown);

      const paragraph = ast.children[0] as SupramarkParentNode;
      expect(paragraph.children.some((node: SupramarkNode) => node.type === 'strong')).toBe(true);
    });

    it('parses italic text', async () => {
      const markdown = 'This is *italic* text.';
      const ast = await parse(markdown);

      const paragraph = ast.children[0] as SupramarkParentNode;
      expect(paragraph.children.some((node: SupramarkNode) => node.type === 'emphasis')).toBe(true);
    });

    it('parses a link', async () => {
      const markdown = '[Link](https://example.com)';
      const ast = await parse(markdown);

      const paragraph = ast.children[0] as SupramarkParentNode;
      expect(paragraph.children.some((node: SupramarkNode) => node.type === 'link')).toBe(true);
    });

    it('parses inline code', async () => {
      const markdown = 'This is `code` inline.';
      const ast = await parse(markdown);

      const paragraph = ast.children[0] as SupramarkParentNode;
      expect(paragraph.children.some((node: SupramarkNode) => node.type === 'inline_code')).toBe(true);
    });
  });

  describe('GFM extensions', () => {
    it('parses strikethrough', async () => {
      const markdown = 'This is ~~deleted~~ text.';
      const ast = await parse(markdown);

      const paragraph = ast.children[0] as SupramarkParentNode;
      expect(paragraph.children.some((node: SupramarkNode) => node.type === 'delete')).toBe(true);
    });

    it('parses a task list', async () => {
      const markdown = '- [x] Task 1\n- [ ] Task 2';
      const ast = await parse(markdown);

      const list = ast.children[0] as SupramarkListNode;
      expect(list.type).toBe('list');
      expect((list.children[0] as SupramarkListItemNode).checked).toBe(true);
      expect((list.children[1] as SupramarkListItemNode).checked).toBe(false);
    });

    it('parses a table', async () => {
      const markdown = '| Header 1 | Header 2 |\n|----------|----------|\n| Cell 1   | Cell 2   |';
      const ast = await parse(markdown);

      expect(ast.children).toHaveLength(1);
      expect(ast.children[0].type).toBe('table');
    });
  });

  describe('diagram nodes', () => {
    it('parses a mermaid code block as a diagram node', async () => {
      const markdown = '```mermaid\ngraph TD;\n  A-->B;\n```';
      const ast = await parse(markdown);

      expect(ast.children).toHaveLength(1);
      expect(ast.children[0].type).toBe('diagram');
      expect((ast.children[0] as SupramarkDiagramNode).engine).toBe('mermaid');
      expect((ast.children[0] as SupramarkDiagramNode).fence_closed).toBe(true);
    });

    it('flags a diagram fence that has not been closed yet (streaming)', async () => {
      const markdown = '```mermaid\ngraph TD;\n  A-->B;';
      const ast = await parse(markdown);

      expect(ast.children).toHaveLength(1);
      expect(ast.children[0].type).toBe('diagram');
      expect((ast.children[0] as SupramarkDiagramNode).fence_closed).toBe(false);
    });

    it('parses a plantuml code block as a diagram node', async () => {
      const markdown = '```plantuml\n@startuml\nA -> B\n@enduml\n```';
      const ast = await parse(markdown);

      expect(ast.children).toHaveLength(1);
      expect(ast.children[0].type).toBe('diagram');
      expect((ast.children[0] as SupramarkDiagramNode).engine).toBe('plantuml');
    });
  });

  describe('Math nodes', () => {
    it('parses an inline formula', async () => {
      const markdown = 'Inline math: $E = mc^2$';
      const ast = await parse(markdown);

      const paragraph = ast.children[0] as SupramarkParentNode;
      expect(paragraph.children.some((node: SupramarkNode) => node.type === 'math_inline')).toBe(true);
    });

    it('parses a block-level formula', async () => {
      const markdown = '$$\n\\int_0^1 x^2 dx\n$$';
      const ast = await parse(markdown);

      expect(ast.children.some((node: SupramarkNode) => node.type === 'math_block')).toBe(true);
    });

    // Regression for #207: inline math whose TeX source contains cmark
    // backslash-escaped punctuation (`\{`, `\}`) used to collapse to literal
    // text because cmark decodes `\{`→`{` before the math scanner runs,
    // breaking the `$` delimiter parity check. The value must preserve the
    // raw escapes so the TeX engine receives `\{0, ?, 1\}`, not `{0, ?, 1}`.
    it('parses inline math with escaped braces and widehat (#207)', async () => {
      const inputs: Array<{ src: string; expected: string }> = [
        { src: '$\\widehat{\\rho}=1$', expected: '\\widehat{\\rho}=1' },
        { src: '$\\widehat\\rho=1$', expected: '\\widehat\\rho=1' },
        {
          src: 'text $\\widehat{\\rho}_{\\Gamma,q}(a,s) \\in \\{0, ?, 1\\}$ text',
          expected: '\\widehat{\\rho}_{\\Gamma,q}(a,s) \\in \\{0, ?, 1\\}',
        },
        { src: 'text $\\{0\\}$ text', expected: '\\{0\\}' },
      ];
      for (const { src, expected } of inputs) {
        const ast = await parse(src);
        const paragraph = ast.children[0] as SupramarkParentNode;
        const math = paragraph.children.find(
          (node: SupramarkNode) => node.type === 'math_inline'
        ) as SupramarkMathInlineNode | undefined;
        expect(math).toBeDefined();
        expect(math?.value).toBe(expected);
      }
    });

    it('parses block math with escaped braces and widehat (#207)', async () => {
      const markdown = '$$\n\\widehat{\\rho}_{\\Gamma,q}(a,s) \\in \\{0, ?, 1\\}\n$$';
      const ast = await parse(markdown);
      const math = ast.children.find(
        (node: SupramarkNode) => node.type === 'math_block'
      ) as SupramarkMathBlockNode | undefined;
      expect(math?.value).toBe('\\widehat{\\rho}_{\\Gamma,q}(a,s) \\in \\{0, ?, 1\\}');
    });
  });

  describe('a complex document', () => {
    it('parses a document containing multiple element types', async () => {
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

      // Verify that multiple node types are present
      expect(ast.children.length).toBeGreaterThan(1);
      expect(ast.children.some(node => node.type === 'heading')).toBe(true);
      expect(ast.children.some(node => node.type === 'paragraph')).toBe(true);
      expect(ast.children.some(node => node.type === 'list')).toBe(true);
      expect(ast.children.some(node => node.type === 'code')).toBe(true);
      expect(ast.children.some(node => node.type === 'table')).toBe(true);
    });
  });

  describe('empty input handling', () => {
    it('handles an empty string', async () => {
      const ast = await parse('');
      expect(ast.type).toBe('root');
      expect(ast.children).toHaveLength(0);
    });

    it('handles a string containing only whitespace', async () => {
      const ast = await parse('   \n\n   ');
      expect(ast.type).toBe('root');
      expect(ast.children).toHaveLength(0);
    });
  });
});
