import type {
  SupramarkFeature,
  SupramarkNode,
  SupramarkRootNode,
  SupramarkMathInlineNode,
  SupramarkMathBlockNode,
  FeatureConfigWithOptions,
} from '@supramark/core';
import { makeFeatureConfigHelpers } from '@supramark/core';
import { mathExamples } from './examples.js';

/**
 * Math Feature
 *
 * Canonical description of Math / LaTeX formula capability for supramark.
 *
 * - Reuses the `math_inline` / `math_block` AST already defined in core;
 * - Does not own the actual parsing/rendering logic (implemented by @supramark/core / @supramark/web / @supramark/rn);
 * - Mainly used for: documentation, capability discovery, and the FeatureRegistry configuration bridge.
 *
 * @example
 * ```markdown
 * Inline formula: this is the famous $E = mc^2$.
 *
 * Block formula:
 *
 * $$
 * \frac{1}{\sqrt{2\pi\sigma^2}} e^{-\frac{(x - \mu)^2}{2\sigma^2}}
 * $$
 * ```
 */
export const mathFeature: SupramarkFeature<SupramarkMathInlineNode | SupramarkMathBlockNode> = {
  metadata: {
    id: '@supramark/feature-math',
    name: 'Math',
    version: '0.1.0',
    author: 'Supramark Team',
    description: 'LaTeX math formula support',
    license: 'Apache-2.0',
    tags: ['math', 'latex', 'formula'],
    syntaxFamily: 'main',
  },
  // Math - no dependencies (a standalone LaTeX syntax, just a value string)
  // dependencies: [] - do not declare an empty dependency array explicitly

  syntax: {
    ast: {
      // Uses inline Math as the primary type, and covers block Math via the selector
      type: 'math_inline',
      selector: (node: SupramarkNode) => node.type === 'math_inline' || node.type === 'math_block',

      // Optional: describes the node interface
      interface: {
        required: ['type', 'value'],
        optional: [],
        fields: {
          type: {
            type: 'string',
            description: 'Node type: "math_inline" for inline formulas, "math_block" for block formulas.',
          },
          value: {
            type: 'string',
            description: 'Raw TeX text content, without the wrapping $ / $$.',
          },
        },
      },

      // Optional: node constraints
      constraints: {
        // Inline formulas typically appear in paragraphs, list items, table cells, etc;
        // block formulas are usually standalone blocks under root / list_item.
        allowedParents: ['root', 'paragraph', 'list_item', 'table_cell'],
        allowedChildren: [],
      },

      // Optional: example nodes
      examples: [
        {
          type: 'math_inline',
          value: 'E = mc^2',
        } as SupramarkMathInlineNode,
        {
          type: 'math_block',
          value: '\\frac{1}{\\sqrt{2\\pi\\sigma^2}} e^{-\\frac{(x - \\mu)^2}{2\\sigma^2}}',
        } as SupramarkMathBlockNode,
      ],
    },

    // Optional: validation rules
    // validator: {
    //   validate: (node) => {
    //     // TODO: add validation logic
    //     return { valid: true, errors: [] };
    //   }
    // },
  },

  // Renderer definitions
  renderers: {
    // Web platform renderer
    web: {
      platform: 'web',

      // Infrastructure requirements
      infrastructure: {
        // On Web, rendering is done via a client-side script (KaTeX)
        needsClientScript: true,
        // No worker needed
        needsWorker: false,
        // No cache needed (KaTeX renders fast)
        needsCache: false,
      },

      // External library dependencies
      dependencies: [
        {
          name: 'katex',
          version: '^0.16.9',
          type: 'cdn',
          cdnUrl: 'https://cdn.jsdelivr.net/npm/katex@0.16.9/dist/katex.min.js',
          optional: false,
        },
        {
          name: 'katex-css',
          version: '^0.16.9',
          type: 'cdn',
          cdnUrl: 'https://cdn.jsdelivr.net/npm/katex@0.16.9/dist/katex.min.css',
          optional: false,
        },
      ],
    },

    // React Native platform renderer
    rn: {
      platform: 'rn',

      // Infrastructure requirements
      infrastructure: {
        // On RN, MathJax renders directly to SVG locally
        needsWorker: false,
        // Cache needed (MathJax's first-time initialization and complex-formula rendering are costly)
        needsCache: true,
        cacheConfig: {
          maxSize: 100,
          ttl: 600000, // 10 minutes
        },
      },

      // External library dependencies
      dependencies: [
        {
          name: 'react-native-svg',
          version: '^13.0.0',
          type: 'npm',
          optional: false,
        },
        {
          name: 'mathjax-full',
          version: '^3.2.2',
          type: 'npm',
          optional: false,
        },
      ],
    },
  },

  // Usage examples
  examples: mathExamples,

  // Test definitions
  testing: {
    // Markdown → AST syntax tests
    syntaxTests: {
      cases: [
        {
          name: 'parses an inline math formula',
          input: 'This is the $E = mc^2$ formula',
          expected: {
            type: 'math_inline',
            value: 'E = mc^2',
          } as SupramarkMathInlineNode,
          options: {
            typeOnly: false,
          },
        },
        {
          name: 'parses a block math formula',
          input: '$$\n\\frac{1}{2}\n$$',
          expected: {
            type: 'math_block',
            value: '\\frac{1}{2}',
          } as SupramarkMathBlockNode,
          options: {
            typeOnly: false,
          },
        },
        {
          name: 'parses a complex inline formula',
          input: 'By the formula $\\sum_{i=1}^{n} i = \\frac{n(n+1)}{2}$ we know',
          expected: {
            type: 'math_inline',
            value: '\\sum_{i=1}^{n} i = \\frac{n(n+1)}{2}',
          } as SupramarkMathInlineNode,
          options: {
            typeOnly: false,
          },
        },
        {
          // Regression for #207: escaped braces (`\{`, `\}`) inside inline
          // math must not break delimiter scanning, and the value must keep
          // the raw escapes for the TeX engine.
          name: 'parses inline math with escaped braces and widehat (#207)',
          input: 'text $\\widehat{\\rho}_{\\Gamma,q}(a,s) \\in \\{0, ?, 1\\}$ text',
          expected: {
            type: 'math_inline',
            value: '\\widehat{\\rho}_{\\Gamma,q}(a,s) \\in \\{0, ?, 1\\}',
          } as SupramarkMathInlineNode,
          options: {
            typeOnly: false,
          },
        },
        {
          name: 'parses block math with escaped braces and widehat (#207)',
          input: '$$\n\\widehat{\\rho}_{\\Gamma,q}(a,s) \\in \\{0, ?, 1\\}\n$$',
          expected: {
            type: 'math_block',
            value: '\\widehat{\\rho}_{\\Gamma,q}(a,s) \\in \\{0, ?, 1\\}',
          } as SupramarkMathBlockNode,
          options: {
            typeOnly: false,
          },
        },
      ],
    },

    // AST → render output tests
    renderTests: {
      web: [
        {
          name: 'Web renders an inline formula',
          input: {
            type: 'math_inline',
            value: 'x^2',
          } as SupramarkMathInlineNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
        {
          name: 'Web renders a fraction formula',
          input: {
            type: 'math_inline',
            value: '\\frac{a}{b}',
          } as SupramarkMathInlineNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
      ],
      rn: [
        {
          name: 'RN renders a block formula',
          input: {
            type: 'math_block',
            value: '\\sum_{i=1}^{n}',
          } as SupramarkMathBlockNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
        {
          name: 'RN renders a complex formula',
          input: {
            type: 'math_block',
            value: '\\int_0^1 x^2 dx',
          } as SupramarkMathBlockNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
      ],
    },

    // End-to-end integration tests
    integrationTests: {
      cases: [
        {
          name: 'Math end-to-end: inline + block formulas',
          input: 'Test $x^2$ and\n\n$$\\int_0^1$$',
          validate: result => {
            if (!result || typeof result !== 'object') return false;
            const nodes = (result as SupramarkRootNode).children || [];
            const hasMathInline = nodes.some(
              (n: SupramarkNode) =>
                n.type === 'paragraph' &&
                n.children?.some((c: SupramarkNode) => c.type === 'math_inline')
            );
            const hasMathBlock = nodes.some((n: SupramarkNode) => n.type === 'math_block');
            return hasMathInline && hasMathBlock;
          },
          platforms: ['web', 'rn'],
        },
        {
          name: 'Math end-to-end: multiple inline formulas',
          input: 'Formulas $a^2$ and $b^2$ and $c^2$',
          validate: result => {
            if (!result || typeof result !== 'object') return false;
            const nodes = (result as SupramarkRootNode).children || [];
            return nodes.some(
              (n: SupramarkNode) =>
                n.type === 'paragraph' &&
                Array.isArray(n.children) &&
                n.children.filter((c: SupramarkNode) => c.type === 'math_inline').length >= 3
            );
          },
          platforms: ['web', 'rn'],
        },
      ],
    },

    // Coverage requirements
    coverageRequirements: {
      statements: 80,
      branches: 75,
      functions: 80,
      lines: 80,
    },
  },

  // Documentation definitions
  documentation: {
    readme: `
# Math Feature

Provides LaTeX math formula support for Supramark.

## Features

- Inline formulas: \`$...$\`
- Block formulas: \`$$...$$\`

## Example

Inline formula: this is the famous $E = mc^2$.

Block formula:

$$
\\frac{1}{\\sqrt{2\\pi\\sigma^2}} e^{-\\frac{(x - \\mu)^2}{2\\sigma^2}}
$$

## Configuration

\`\`\`typescript
import { createMathFeatureConfig } from '@supramark/feature-math';

const config = createMathFeatureConfig(true, {
  engine: 'katex', // or 'mathjax'
});
\`\`\`
    `.trim(),

    api: {
      interfaces: [
        {
          name: 'MathFeatureOptions',
          description: 'Configuration options interface for the Math Feature',
          fields: [
            {
              name: 'engine',
              type: "'katex' | 'mathjax'",
              description: 'Math formula rendering engine, used to select KaTeX or MathJax',
              required: false,
              default: 'katex',
            },
          ],
        },
        {
          name: 'SupramarkMathInlineNode',
          description: 'AST node interface for inline math formulas, representing inline math ($...$) in Markdown',
          fields: [
            {
              name: 'type',
              type: "'math_inline'",
              description: 'Node type identifier, always "math_inline"',
              required: true,
            },
            {
              name: 'value',
              type: 'string',
              description: 'LaTeX formula content (without the wrapping $), e.g. "E = mc^2"',
              required: true,
            },
          ],
        },
        {
          name: 'SupramarkMathBlockNode',
          description: 'AST node interface for block math formulas, representing block math ($$...$$) in Markdown',
          fields: [
            {
              name: 'type',
              type: "'math_block'",
              description: 'Node type identifier, always "math_block"',
              required: true,
            },
            {
              name: 'value',
              type: 'string',
              description: 'LaTeX formula content (without the wrapping $$), supports multi-line formulas',
              required: true,
            },
          ],
        },
      ],

      functions: [
        {
          name: 'createMathFeatureConfig',
          description: 'Creates a Math Feature config object, used to enable math formula support in SupramarkConfig',
          parameters: [
            {
              name: 'enabled',
              type: 'boolean',
              description: 'Whether to enable the Math Feature',
              optional: false,
            },
            {
              name: 'options',
              type: 'MathFeatureOptions',
              description: 'Math Feature configuration options, can specify the rendering engine and other parameters',
              optional: true,
            },
          ],
          returns: 'FeatureConfigWithOptions<MathFeatureOptions>',
          examples: [
            `import { createMathFeatureConfig } from '@supramark/feature-math';

const config = {
  features: [
    createMathFeatureConfig(true, {
      engine: 'katex',
    }),
  ],
};`,
            `// Using the MathJax engine
const config = {
  features: [
    createMathFeatureConfig(true, {
      engine: 'mathjax',
    }),
  ],
};`,
          ],
        },
        {
          name: 'getMathFeatureOptions',
          description: 'Extracts the Math Feature configuration options from a SupramarkConfig',
          parameters: [
            {
              name: 'config',
              type: 'SupramarkConfig',
              description: 'The Supramark configuration object',
              optional: true,
            },
          ],
          returns: 'MathFeatureOptions | undefined',
          examples: [
            `import { getMathFeatureOptions } from '@supramark/feature-math';

const options = getMathFeatureOptions(config);
if (options) {
  console.log('Current rendering engine:', options.engine);
}`,
          ],
        },
      ],

      types: [
        {
          name: 'MathFeatureConfig',
          description:
            'Math Feature configuration type, a type alias for FeatureConfigWithOptions<MathFeatureOptions>',
          definition: 'type MathFeatureConfig = FeatureConfigWithOptions<MathFeatureOptions>',
        },
      ],
    },

    bestPractices: [
      'Wrap inline formulas with $ and block formulas with $$',
      'For complex formulas, prefer the block format for better readability',
      'Ensure the LaTeX syntax is correct to avoid rendering errors',
    ],

    faq: [
      {
        question: 'Which LaTeX syntax is supported?',
        answer:
          'Most of the standard LaTeX math-mode syntax is supported, depending on the chosen rendering engine (KaTeX or MathJax).',
      },
      {
        question: 'How do I switch rendering engines?',
        answer: 'Set the options.engine field; valid values are "katex" or "mathjax".',
      },
    ],
  },
};

// Register the Feature (optional)
// FeatureRegistry.register(mathFeature);

/**
 * Configuration options for the Math Feature.
 *
 * - engine: reserved for future selection of the rendering engine ('katex' | 'mathjax');
 *   the current implementation defaults to MathJax, this field is kept for future evolution.
 */
export interface MathFeatureOptions {
  engine?: 'katex' | 'mathjax';
}

export type MathFeatureConfig = FeatureConfigWithOptions<MathFeatureOptions>;

const mathHelpers = makeFeatureConfigHelpers<MathFeatureOptions>('@supramark/feature-math');
export const createMathFeatureConfig = mathHelpers.create;
export const getMathFeatureOptions = mathHelpers.getOptions;
