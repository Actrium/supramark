import { parse } from '../src/plugin';
import type { SupramarkConfig } from '../src/feature';
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
  SupramarkWikiLinkNode,
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

  describe('WikiLink nodes', () => {
    it('leaves [[...]] as text by default', async () => {
      const ast = await parse('See [[Project Plan]] here.');
      const paragraph = ast.children[0] as SupramarkParentNode;
      expect(paragraph.children.some((node: SupramarkNode) => node.type === 'wiki_link')).toBe(false);
      expect(paragraph.children.some((node: SupramarkNode) => node.type === 'text')).toBe(true);
    });

    it('parses all wikilink forms with the wikilink option', async () => {
      const cases: Array<[string, string, string | undefined, string | undefined]> = [
        ['[[Project Plan]]', 'Project Plan', undefined, undefined],
        ['[[Project Plan|the plan]]', 'Project Plan', undefined, 'the plan'],
        ['[[Project Plan#Roadmap]]', 'Project Plan', 'Roadmap', undefined],
        ['[[Project Plan#Roadmap|the plan]]', 'Project Plan', 'Roadmap', 'the plan'],
        ['[[#Roadmap]]', '', 'Roadmap', undefined],
      ];
      for (const [input, target, section, label] of cases) {
        const ast = await parse(input, { wikilink: true });
        const paragraph = ast.children[0] as SupramarkParentNode;
        expect(paragraph.children).toHaveLength(1);
        const wikilink = paragraph.children[0] as SupramarkWikiLinkNode;
        expect(wikilink.type).toBe('wiki_link');
        expect(wikilink.target).toBe(target);
        expect(wikilink.section).toBe(section);
        expect(wikilink.label).toBe(label);
        expect(wikilink.position?.start.byte_offset).toBe(0);
      }
    });

    it('keeps escaped, code-span and malformed forms literal with the option on', async () => {
      const ast = await parse('`[[code]]` and \\[[escaped]] and [[unclosed', { wikilink: true });
      const paragraph = ast.children[0] as SupramarkParentNode;
      const types = paragraph.children.map((node: SupramarkNode) => node.type);
      expect(types).not.toContain('wiki_link');
      expect(types).toContain('inline_code');
      const inlineCode = paragraph.children.find(
        (node: SupramarkNode) => node.type === 'inline_code'
      ) as { value: string };
      expect(inlineCode.value).toBe('[[code]]');
    });

    it('enables wikilink parsing via config.features (no explicit flag)', async () => {
      const config: SupramarkConfig = {
        features: [{ id: '@supramark/feature-wikilink', enabled: true, options: {} }],
      };
      const ast = await parse('See [[Project Plan#Roadmap|the plan]].', { config });
      const paragraph = ast.children[0] as SupramarkParentNode;
      expect(paragraph.children).toHaveLength(3);
      const wikilink = paragraph.children[1] as SupramarkWikiLinkNode;
      expect(wikilink.type).toBe('wiki_link');
      expect(wikilink.target).toBe('Project Plan');
      expect(wikilink.section).toBe('Roadmap');
      expect(wikilink.label).toBe('the plan');
    });

    it('keeps wikilink off when the feature config is disabled', async () => {
      const config: SupramarkConfig = {
        features: [{ id: '@supramark/feature-wikilink', enabled: false }],
      };
      const ast = await parse('See [[Project Plan]].', { config });
      const paragraph = ast.children[0] as SupramarkParentNode;
      expect(paragraph.children.some((node: SupramarkNode) => node.type === 'wiki_link')).toBe(false);
    });

    it('defers to inline math: $[[foo]]$ stays one math_inline', async () => {
      const ast = await parse('pre $[[foo]]$ post', { wikilink: true });
      const paragraph = ast.children[0] as SupramarkParentNode;
      const types = paragraph.children.map((node: SupramarkNode) => node.type);
      expect(types).toContain('math_inline');
      expect(types).not.toContain('wiki_link');
      const math = paragraph.children.find(
        (node: SupramarkNode) => node.type === 'math_inline'
      ) as { value: string };
      expect(math.value).toBe('[[foo]]');
    });

    it('expands wikilinks inside transparent container bodies', async () => {
      const ast = await parse(':::note\nSee [[Project Plan]].\n:::', { wikilink: true });
      const container = ast.children[0] as SupramarkParentNode;
      expect(container.type).toBe('container');
      const paragraph = container.children[0] as SupramarkParentNode;
      expect(paragraph.children.some((node: SupramarkNode) => node.type === 'wiki_link')).toBe(true);
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
