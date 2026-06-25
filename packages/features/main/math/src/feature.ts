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
 * 为 supramark 提供 Math / LaTeX 公式能力的规范描述。
 *
 * - 复用 core 中已定义的 `math_inline` / `math_block` AST；
 * - 不负责实际解析与渲染逻辑（由 @supramark/core / @supramark/web / @supramark/rn 实现）；
 * - 主要用于：文档、能力发现、FeatureRegistry 配置桥梁。
 *
 * @example
 * ```markdown
 * 行内公式：这是著名的 $E = mc^2$。
 *
 * 块级公式：
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
    description: 'LaTeX 数学公式支持',
    license: 'Apache-2.0',
    tags: ['math', 'latex', 'formula'],
    syntaxFamily: 'main',
  },
  // Math - 无依赖（独立的 LaTeX 语法，只有 value 字符串）
  // dependencies: [] - 不显式声明空依赖
  // Math - 无依赖（独立的 LaTeX 语法，只有 value 字符串）
  // dependencies: [] - 不显式声明空依赖

  syntax: {
    ast: {
      // 以 inline Math 作为主类型，通过 selector 覆盖 block Math
      type: 'math_inline',
      selector: (node: SupramarkNode) => node.type === 'math_inline' || node.type === 'math_block',

      // 可选：描述节点接口
      interface: {
        required: ['type', 'value'],
        optional: [],
        fields: {
          type: {
            type: 'string',
            description: '节点类型，行内公式为 "math_inline"，块级公式为 "math_block"。',
          },
          value: {
            type: 'string',
            description: '原始 TeX 文本内容，不含包裹的 $ / $$。',
          },
        },
      },

      // 可选：节点约束
      constraints: {
        // 行内公式通常出现在段落、列表项、表格单元格等位置；
        // 块级公式则多为 root / list_item 下的独立块。
        allowedParents: ['root', 'paragraph', 'list_item', 'table_cell'],
        allowedChildren: [],
      },

      // 可选：示例节点
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

    // 可选：验证规则
    // validator: {
    //   validate: (node) => {
    //     // TODO: 添加验证逻辑
    //     return { valid: true, errors: [] };
    //   }
    // },
  },

  // 渲染器定义
  renderers: {
    // Web 平台渲染器
    web: {
      platform: 'web',

      // 基础设施需求
      infrastructure: {
        // Web 端使用客户端脚本（KaTeX）进行渲染
        needsClientScript: true,
        // 无需 Worker
        needsWorker: false,
        // 无需缓存（KaTeX 渲染很快）
        needsCache: false,
      },

      // 依赖的外部库
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

    // React Native 平台渲染器
    rn: {
      platform: 'rn',

      // 基础设施需求
      infrastructure: {
        // RN 端本地使用 MathJax 直接渲染为 SVG
        needsWorker: false,
        // 需要缓存（MathJax 首次初始化与复杂公式渲染成本较高）
        needsCache: true,
        cacheConfig: {
          maxSize: 100,
          ttl: 600000, // 10 分钟
        },
      },

      // 依赖的外部库
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

  // 使用示例
  examples: mathExamples,

  // 测试定义
  testing: {
    // Markdown → AST 语法测试
    syntaxTests: {
      cases: [
        {
          name: '解析行内数学公式',
          input: '这是 $E = mc^2$ 公式',
          expected: {
            type: 'math_inline',
            value: 'E = mc^2',
          } as SupramarkMathInlineNode,
          options: {
            typeOnly: false,
          },
        },
        {
          name: '解析块级数学公式',
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
          name: '解析复杂行内公式',
          input: '根据公式 $\\sum_{i=1}^{n} i = \\frac{n(n+1)}{2}$ 可知',
          expected: {
            type: 'math_inline',
            value: '\\sum_{i=1}^{n} i = \\frac{n(n+1)}{2}',
          } as SupramarkMathInlineNode,
          options: {
            typeOnly: false,
          },
        },
      ],
    },

    // AST → 渲染输出测试
    renderTests: {
      web: [
        {
          name: 'Web 渲染行内公式',
          input: {
            type: 'math_inline',
            value: 'x^2',
          } as SupramarkMathInlineNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
        {
          name: 'Web 渲染分数公式',
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
          name: 'RN 渲染块级公式',
          input: {
            type: 'math_block',
            value: '\\sum_{i=1}^{n}',
          } as SupramarkMathBlockNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
        {
          name: 'RN 渲染复杂公式',
          input: {
            type: 'math_block',
            value: '\\int_0^1 x^2 dx',
          } as SupramarkMathBlockNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
      ],
    },

    // 端到端集成测试
    integrationTests: {
      cases: [
        {
          name: 'Math 端到端：行内 + 块级公式',
          input: '测试 $x^2$ 和\n\n$$\\int_0^1$$',
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
          name: 'Math 端到端：多个行内公式',
          input: '公式 $a^2$ 和 $b^2$ 以及 $c^2$',
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

    // 覆盖率要求
    coverageRequirements: {
      statements: 80,
      branches: 75,
      functions: 80,
      lines: 80,
    },
  },

  // 文档定义
  documentation: {
    readme: `
# Math Feature

为 Supramark 提供 LaTeX 数学公式支持。

## 功能

- 行内公式：\`$...$\`
- 块级公式：\`$$...$$\`

## 示例

行内公式：这是著名的 $E = mc^2$。

块级公式：

$$
\\frac{1}{\\sqrt{2\\pi\\sigma^2}} e^{-\\frac{(x - \\mu)^2}{2\\sigma^2}}
$$

## 配置

\`\`\`typescript
import { createMathFeatureConfig } from '@supramark/feature-math';

const config = createMathFeatureConfig(true, {
  engine: 'katex', // 或 'mathjax'
});
\`\`\`
    `.trim(),

    api: {
      interfaces: [
        {
          name: 'MathFeatureOptions',
          description: 'Math Feature 的配置选项接口',
          fields: [
            {
              name: 'engine',
              type: "'katex' | 'mathjax'",
              description: '数学公式渲染引擎，用于选择 KaTeX 或 MathJax 作为渲染引擎',
              required: false,
              default: 'katex',
            },
          ],
        },
        {
          name: 'SupramarkMathInlineNode',
          description: '行内数学公式 AST 节点接口，用于表示 Markdown 中的行内数学公式（$...$）',
          fields: [
            {
              name: 'type',
              type: "'math_inline'",
              description: '节点类型标识，固定为 "math_inline"',
              required: true,
            },
            {
              name: 'value',
              type: 'string',
              description: 'LaTeX 公式内容（不含 $ 包裹符），例如 "E = mc^2"',
              required: true,
            },
          ],
        },
        {
          name: 'SupramarkMathBlockNode',
          description: '块级数学公式 AST 节点接口，用于表示 Markdown 中的块级数学公式（$$...$$）',
          fields: [
            {
              name: 'type',
              type: "'math_block'",
              description: '节点类型标识，固定为 "math_block"',
              required: true,
            },
            {
              name: 'value',
              type: 'string',
              description: 'LaTeX 公式内容（不含 $$ 包裹符），支持多行公式',
              required: true,
            },
          ],
        },
      ],

      functions: [
        {
          name: 'createMathFeatureConfig',
          description: '创建 Math Feature 配置对象，用于在 SupramarkConfig 中启用数学公式支持',
          parameters: [
            {
              name: 'enabled',
              type: 'boolean',
              description: '是否启用 Math Feature',
              optional: false,
            },
            {
              name: 'options',
              type: 'MathFeatureOptions',
              description: 'Math Feature 配置选项，可指定渲染引擎等参数',
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
            `// 使用 MathJax 引擎
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
          description: '从 SupramarkConfig 中提取 Math Feature 的配置选项',
          parameters: [
            {
              name: 'config',
              type: 'SupramarkConfig',
              description: 'Supramark 配置对象',
              optional: true,
            },
          ],
          returns: 'MathFeatureOptions | undefined',
          examples: [
            `import { getMathFeatureOptions } from '@supramark/feature-math';

const options = getMathFeatureOptions(config);
if (options) {
  console.log('当前使用的渲染引擎:', options.engine);
}`,
          ],
        },
      ],

      types: [
        {
          name: 'MathFeatureConfig',
          description:
            'Math Feature 配置类型，是 FeatureConfigWithOptions<MathFeatureOptions> 的类型别名',
          definition: 'type MathFeatureConfig = FeatureConfigWithOptions<MathFeatureOptions>',
        },
      ],
    },

    bestPractices: [
      '使用 $ 包裹行内公式，使用 $$ 包裹块级公式',
      '复杂公式建议使用块级格式以提高可读性',
      '确保 LaTeX 语法正确，避免渲染错误',
    ],

    faq: [
      {
        question: '支持哪些 LaTeX 语法？',
        answer:
          '支持标准 LaTeX 数学模式的大部分语法，具体取决于所选的渲染引擎（KaTeX 或 MathJax）。',
      },
      {
        question: '如何切换渲染引擎？',
        answer: '通过配置 options.engine 字段，可选值为 "katex" 或 "mathjax"。',
      },
    ],
  },
};

// 注册 Feature（可选）
// FeatureRegistry.register(mathFeature);

/**
 * Math Feature 的配置项。
 *
 * - engine: 未来用于选择渲染引擎（'katex' | 'mathjax'）；
 *   当前实现默认使用 MathJax，保留此字段便于后续演进。
 */
export interface MathFeatureOptions {
  engine?: 'katex' | 'mathjax';
}

export type MathFeatureConfig = FeatureConfigWithOptions<MathFeatureOptions>;

const mathHelpers = makeFeatureConfigHelpers<MathFeatureOptions>('@supramark/feature-math');
export const createMathFeatureConfig = mathHelpers.create;
export const getMathFeatureOptions = mathHelpers.getOptions;
