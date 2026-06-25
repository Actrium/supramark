import type {
  SupramarkFeature,
  SupramarkNode,
  SupramarkRootNode,
  FeatureConfigWithOptions,
} from '@supramark/core';
import { coreMarkdownExamples } from './examples.js';
import { makeFeatureConfigHelpers } from '@supramark/core';

/**
 * Core Markdown Feature
 *
 * 基础 Markdown 语法（段落 / 标题 / 列表等）的规范性 Feature。
 *
 * - 描述「非扩展」的 Markdown 节点集合；
 * - 覆盖 paragraph / heading / list / blockquote / text / strong / emphasis / link 等；
 * - 显式排除 diagram / math / footnote / definition-list / admonition / table / delete 等扩展节点。
 *
 * @example
 * ```markdown
 * # 标题
 *
 * 段落文本 **粗体** 和 *斜体*，还有 `inline code` 与 [链接](https://example.com)。
 *
 * - 列表项 1
 * - 列表项 2
 *
 * > 引用段落
 * ```
 */
export const coreMarkdownFeature: SupramarkFeature<SupramarkNode> = {
  metadata: {
    id: '@supramark/feature-core-markdown',
    name: 'Core Markdown',
    version: '0.1.0',
    author: 'Supramark Team',
    description: '基础 Markdown 语法（段落 / 标题 / 列表等）',
    license: 'Apache-2.0',
    tags: ['core', 'markdown', 'block', 'inline'],
    syntaxFamily: 'main',
  },
  // 基础 Markdown - 无依赖
  // dependencies: [] - 不显式声明空依赖
  // 基础 Markdown - 无依赖
  // dependencies: [] - 不显式声明空依赖

  syntax: {
    ast: {
      /**
       * 虚拟入口类型："core-markdown"
       *
       * 通过 selector 精确匹配「基础语法节点」：
       * - Block: root / paragraph / heading / list / list_item / blockquote / thematic_break / code
       * - Inline: text / strong / emphasis / inline_code / link / image / break
       *
       * 显式排除：
       * - diagram / math_* / footnote_* / definition_* / admonition / table_* / delete
       */
      type: 'core-markdown',
      selector: (node: SupramarkNode) => {
        const coreTypes: string[] = [
          'root',
          'paragraph',
          'heading',
          'code',
          'list',
          'list_item',
          'blockquote',
          'thematic_break',
          'text',
          'strong',
          'emphasis',
          'inline_code',
          'link',
          'image',
          'break',
        ];
        return coreTypes.includes(node.type as string);
      },

      /**
       * 注意：Core Markdown 是虚拟节点，不对应单一的 AST 节点类型
       * 它通过 selector 匹配基础 Markdown 的多种实际节点
       * 因此不定义具体的 interface
       */
      // interface: undefined (虚拟节点不需要 interface)

      constraints: {
        allowedParents: ['root'],
        allowedChildren: [],
      },

      examples: [
        {
          type: 'root',
          ast_version: 2,
          diagnostics: [],
          children: [],
        } as SupramarkNode,
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
        // Web 端使用标准 HTML 元素（p / h1-h6 / ul / ol / blockquote 等）
        needsClientScript: false,
        // 无需 Worker
        needsWorker: false,
        // 无需缓存
        needsCache: false,
      },

      // 无外部依赖（使用标准 HTML 元素）
      dependencies: [],
    },

    // React Native 平台渲染器
    rn: {
      platform: 'rn',

      // 基础设施需求
      infrastructure: {
        // RN 端使用基础 View + Text 组件
        needsWorker: false,
        // 无需缓存
        needsCache: false,
      },

      // 无外部依赖（使用 View / Text 组件）
      dependencies: [],
    },
  },

  // 使用示例
  examples: coreMarkdownExamples,

  // 测试定义
  testing: {
    // Markdown → AST 语法测试
    syntaxTests: {
      cases: [
        {
          name: '解析段落',
          input: '这是一个段落',
          expected: {
            type: 'paragraph',
            children: [{ type: 'text', value: '这是一个段落' }],
          } as SupramarkNode,
          options: {
            typeOnly: false,
            ignoreFields: ['position', 'data'],
          },
        },
        {
          name: '解析标题',
          input: '# 一级标题',
          expected: {
            type: 'heading',
            depth: 1,
            children: [{ type: 'text', value: '一级标题' }],
          } as SupramarkNode,
          options: {
            typeOnly: false,
            ignoreFields: ['position', 'data'],
          },
        },
        {
          name: '解析列表',
          input: '- 列表项1\n- 列表项2',
          expected: {
            type: 'list',
            ordered: false,
          } as SupramarkNode,
          options: {
            typeOnly: false,
            ignoreFields: ['children', 'start', 'tight', 'position', 'data'],
          },
        },
        {
          name: '解析代码块',
          input: '```javascript\nconst x = 1;\n```',
          expected: {
            type: 'code',
            lang: 'javascript',
            value: 'const x = 1;',
          } as SupramarkNode,
          options: {
            typeOnly: false,
            ignoreFields: ['meta', 'position', 'data'],
          },
        },
      ],
    },

    // AST → 渲染输出测试
    renderTests: {
      web: [
        {
          name: 'Web 渲染段落',
          input: {
            type: 'paragraph',
            children: [{ type: 'text', value: '段落文本' }],
          } as SupramarkNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
        {
          name: 'Web 渲染标题',
          input: {
            type: 'heading',
            depth: 2,
            children: [{ type: 'text', value: '二级标题' }],
          } as SupramarkNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
        {
          name: 'Web 渲染强调',
          input: {
            type: 'strong',
            children: [{ type: 'text', value: '粗体' }],
          } as SupramarkNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
      ],
      rn: [
        {
          name: 'RN 渲染列表',
          input: {
            type: 'list',
            ordered: false,
            children: [{ type: 'list_item', children: [{ type: 'text', value: '项目1' }] }],
          } as SupramarkNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
        {
          name: 'RN 渲染链接',
          input: {
            type: 'link',
            url: 'https://example.com',
            children: [{ type: 'text', value: '链接' }],
          } as SupramarkNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
      ],
    },

    // 端到端集成测试
    integrationTests: {
      cases: [
        {
          name: 'CoreMarkdown 端到端：标题 + 段落',
          input: '# 标题\n\n这是段落',
          validate: result => {
            if (!result || typeof result !== 'object') return false;
            const nodes = (result as SupramarkRootNode).children || [];
            const hasHeading = nodes.some((n: SupramarkNode) => n.type === 'heading');
            const hasParagraph = nodes.some((n: SupramarkNode) => n.type === 'paragraph');
            return hasHeading && hasParagraph;
          },
          platforms: ['web', 'rn'],
        },
        {
          name: 'CoreMarkdown 端到端：列表 + 代码块',
          input: '- 列表\n\n```js\ncode\n```',
          validate: result => {
            if (!result || typeof result !== 'object') return false;
            const nodes = (result as SupramarkRootNode).children || [];
            const hasList = nodes.some((n: SupramarkNode) => n.type === 'list');
            const hasCode = nodes.some((n: SupramarkNode) => n.type === 'code');
            return hasList && hasCode;
          },
          platforms: ['web', 'rn'],
        },
        {
          name: 'CoreMarkdown 端到端：行内格式',
          input: '**粗体** 和 *斜体* 和 `代码`',
          validate: result => {
            if (!result || typeof result !== 'object') return false;
            const nodes = (result as SupramarkRootNode).children || [];
            return nodes.some(
              (n: SupramarkNode) =>
                n.type === 'paragraph' &&
                Array.isArray(n.children) &&
                n.children.some((c: SupramarkNode) => c.type === 'strong') &&
                n.children.some((c: SupramarkNode) => c.type === 'emphasis') &&
                n.children.some((c: SupramarkNode) => c.type === 'inline_code')
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
# Core Markdown Feature

为 Supramark 提供核心 Markdown 语法支持。

## 功能

- 标题
- 段落
- 列表
- 代码块
- 强调

## 使用

查看 examples 目录获取更多示例。
    `.trim(),

    api: {
      interfaces: [
        {
          name: 'CoreMarkdownFeatureOptions',
          description: 'Core Markdown Feature 的配置选项接口（当前为空，保留用于未来扩展）',
          fields: [],
        },
        {
          name: 'SupramarkRootNode',
          description: '根节点接口，表示整个 Markdown 文档的根',
          fields: [
            {
              name: 'type',
              type: "'root'",
              description: '节点类型标识，固定为 "root"',
              required: true,
            },
            {
              name: 'children',
              type: 'SupramarkNode[]',
              description: '文档的顶层节点列表',
              required: true,
            },
          ],
        },
        {
          name: 'SupramarkParagraphNode',
          description: '段落节点接口，表示一个段落块',
          fields: [
            {
              name: 'type',
              type: "'paragraph'",
              description: '节点类型标识，固定为 "paragraph"',
              required: true,
            },
            {
              name: 'children',
              type: 'SupramarkNode[]',
              description: '段落内的行内节点（文本、强调、链接等）',
              required: true,
            },
          ],
        },
        {
          name: 'SupramarkHeadingNode',
          description: '标题节点接口，表示 Markdown 标题（# ... ######）',
          fields: [
            {
              name: 'type',
              type: "'heading'",
              description: '节点类型标识，固定为 "heading"',
              required: true,
            },
            {
              name: 'depth',
              type: '1 | 2 | 3 | 4 | 5 | 6',
              description: '标题级别，1 表示一级标题（#），6 表示六级标题（######）',
              required: true,
            },
            {
              name: 'children',
              type: 'SupramarkNode[]',
              description: '标题文本的行内节点',
              required: true,
            },
          ],
        },
        {
          name: 'SupramarkCodeNode',
          description: '代码块节点接口，表示围栏代码块（```...```）',
          fields: [
            {
              name: 'type',
              type: "'code'",
              description: '节点类型标识，固定为 "code"',
              required: true,
            },
            {
              name: 'lang',
              type: 'string',
              description: '代码语言标识（如 javascript、python 等）',
              required: false,
            },
            {
              name: 'meta',
              type: 'string',
              description: '代码块元数据（语言标识后的额外信息）',
              required: false,
            },
            {
              name: 'value',
              type: 'string',
              description: '代码内容',
              required: true,
            },
          ],
        },
        {
          name: 'SupramarkListNode',
          description: '列表节点接口，表示有序或无序列表',
          fields: [
            {
              name: 'type',
              type: "'list'",
              description: '节点类型标识，固定为 "list"',
              required: true,
            },
            {
              name: 'ordered',
              type: 'boolean',
              description: '是否为有序列表（true）或无序列表（false）',
              required: true,
            },
            {
              name: 'start',
              type: 'number | null',
              description: '有序列表的起始编号（仅对有序列表有效）',
              required: false,
            },
            {
              name: 'tight',
              type: 'boolean',
              description: '是否为紧凑模式（列表项之间无空行）',
              required: false,
            },
            {
              name: 'children',
              type: 'SupramarkListItemNode[]',
              description: '列表项节点数组',
              required: true,
            },
          ],
        },
        {
          name: 'SupramarkListItemNode',
          description: '列表项节点接口',
          fields: [
            {
              name: 'type',
              type: "'list_item'",
              description: '节点类型标识，固定为 "list_item"',
              required: true,
            },
            {
              name: 'children',
              type: 'SupramarkNode[]',
              description: '列表项内容节点',
              required: true,
            },
          ],
        },
        {
          name: 'SupramarkBlockquoteNode',
          description: '引用块节点接口，表示 Markdown 引用（> ...）',
          fields: [
            {
              name: 'type',
              type: "'blockquote'",
              description: '节点类型标识，固定为 "blockquote"',
              required: true,
            },
            {
              name: 'children',
              type: 'SupramarkNode[]',
              description: '引用块内的内容节点',
              required: true,
            },
          ],
        },
        {
          name: 'SupramarkTextNode',
          description: '文本节点接口，表示纯文本内容',
          fields: [
            {
              name: 'type',
              type: "'text'",
              description: '节点类型标识，固定为 "text"',
              required: true,
            },
            {
              name: 'value',
              type: 'string',
              description: '文本内容',
              required: true,
            },
          ],
        },
        {
          name: 'SupramarkStrongNode',
          description: '粗体节点接口，表示强调文本（**...**）',
          fields: [
            {
              name: 'type',
              type: "'strong'",
              description: '节点类型标识，固定为 "strong"',
              required: true,
            },
            {
              name: 'children',
              type: 'SupramarkNode[]',
              description: '粗体内的行内节点',
              required: true,
            },
          ],
        },
        {
          name: 'SupramarkEmphasisNode',
          description: '斜体节点接口，表示强调文本（*...*）',
          fields: [
            {
              name: 'type',
              type: "'emphasis'",
              description: '节点类型标识，固定为 "emphasis"',
              required: true,
            },
            {
              name: 'children',
              type: 'SupramarkNode[]',
              description: '斜体内的行内节点',
              required: true,
            },
          ],
        },
        {
          name: 'SupramarkInlineCodeNode',
          description: '行内代码节点接口，表示行内代码（`...`）',
          fields: [
            {
              name: 'type',
              type: "'inline_code'",
              description: '节点类型标识，固定为 "inline_code"',
              required: true,
            },
            {
              name: 'value',
              type: 'string',
              description: '代码内容',
              required: true,
            },
          ],
        },
        {
          name: 'SupramarkLinkNode',
          description: '链接节点接口，表示 Markdown 链接',
          fields: [
            {
              name: 'type',
              type: "'link'",
              description: '节点类型标识，固定为 "link"',
              required: true,
            },
            {
              name: 'url',
              type: 'string',
              description: '链接目标 URL',
              required: true,
            },
            {
              name: 'title',
              type: 'string',
              description: '链接标题（鼠标悬停时显示）',
              required: false,
            },
            {
              name: 'children',
              type: 'SupramarkNode[]',
              description: '链接文本的行内节点',
              required: true,
            },
          ],
        },
        {
          name: 'SupramarkImageNode',
          description: '图片节点接口，表示 Markdown 图片',
          fields: [
            {
              name: 'type',
              type: "'image'",
              description: '节点类型标识，固定为 "image"',
              required: true,
            },
            {
              name: 'url',
              type: 'string',
              description: '图片 URL',
              required: true,
            },
            {
              name: 'alt',
              type: 'string',
              description: '图片替代文本',
              required: false,
            },
            {
              name: 'title',
              type: 'string',
              description: '图片标题',
              required: false,
            },
          ],
        },
      ],

      functions: [
        {
          name: 'createCoreMarkdownFeatureConfig',
          description:
            '创建 Core Markdown Feature 配置对象，用于在 SupramarkConfig 中启用基础 Markdown 语法支持',
          parameters: [
            {
              name: 'enabled',
              type: 'boolean',
              description: '是否启用 Core Markdown Feature',
              optional: false,
            },
            {
              name: 'options',
              type: 'CoreMarkdownFeatureOptions',
              description: 'Core Markdown Feature 配置选项（当前为空对象）',
              optional: true,
            },
          ],
          returns: 'FeatureConfigWithOptions<CoreMarkdownFeatureOptions>',
          examples: [
            `import { createCoreMarkdownFeatureConfig } from '@supramark/feature-core-markdown';

const config = {
  features: [
    createCoreMarkdownFeatureConfig(true),
  ],
};`,
          ],
        },
        {
          name: 'getCoreMarkdownFeatureOptions',
          description: '从 SupramarkConfig 中提取 Core Markdown Feature 的配置选项',
          parameters: [
            {
              name: 'config',
              type: 'SupramarkConfig',
              description: 'Supramark 配置对象',
              optional: true,
            },
          ],
          returns: 'CoreMarkdownFeatureOptions | undefined',
          examples: [
            `import { getCoreMarkdownFeatureOptions } from '@supramark/feature-core-markdown';

const options = getCoreMarkdownFeatureOptions(config);`,
          ],
        },
      ],

      types: [
        {
          name: 'CoreMarkdownFeatureConfig',
          description:
            'Core Markdown Feature 配置类型，是 FeatureConfigWithOptions<CoreMarkdownFeatureOptions> 的类型别名',
          definition:
            'type CoreMarkdownFeatureConfig = FeatureConfigWithOptions<CoreMarkdownFeatureOptions>',
        },
      ],
    },

    bestPractices: [
      '使用 # 表示标题，数量代表标题级别（# 到 ######）',
      '段落之间使用空行分隔',
      '列表项使用 - 或 * 表示无序列表，使用数字加点表示有序列表',
      '代码块使用三个反引号包裹，并指定语言以启用语法高亮',
      '行内格式：**粗体**、*斜体*、`代码`',
      '链接格式：[文本](URL "标题")',
      '图片格式：![替代文本](URL "标题")',
    ],

    faq: [
      {
        question: 'Core Markdown Feature 包含哪些功能？',
        answer:
          'Core Markdown Feature 包含所有基础 Markdown 语法，包括标题、段落、列表、代码块、引用、强调、链接、图片等核心元素。',
      },
      {
        question: 'Core Markdown 与扩展功能有什么区别？',
        answer:
          'Core Markdown 提供标准 Markdown 语法支持，扩展功能（如 GFM、Math、Footnote 等）提供额外的语法能力。',
      },
      {
        question: '是否必须启用 Core Markdown Feature？',
        answer:
          '是的。Core Markdown Feature 提供了基础的 Markdown 解析能力，是其他扩展功能的基础。',
      },
    ],
  },
};

/**
 * Core Markdown Feature 的配置项。
 */
export interface CoreMarkdownFeatureOptions {
  // 当前为空，保留用于未来扩展
}

export type CoreMarkdownFeatureConfig = FeatureConfigWithOptions<CoreMarkdownFeatureOptions>;

const coreMarkdownHelpers = makeFeatureConfigHelpers<CoreMarkdownFeatureOptions>(
  '@supramark/feature-core-markdown'
);
export const createCoreMarkdownFeatureConfig = coreMarkdownHelpers.create;
export const getCoreMarkdownFeatureOptions = coreMarkdownHelpers.getOptions;
