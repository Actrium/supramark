import type {
  SupramarkFeature,
  SupramarkNode,
  SupramarkRootNode,
  SupramarkTextNode,
  FeatureConfigWithOptions,
} from '@supramark/core';
import { emojiExamples } from './examples.js';
import { makeFeatureConfigHelpers } from '@supramark/core';

/**
 * Emoji Feature
 *
 * Emoji / 短代码支持（:smile: → 😄）的规范定义。
 *
 * - 由 supramark-markdown AST v2 parser 将 `:smile:` 等短代码转换为 Unicode emoji；
 * - supramark AST 不引入单独的 emoji 节点，直接体现在 `text.value` 中；
 * - 解析和渲染逻辑由 @supramark/core / RN / Web 渲染器负责。
 *
 * @example
 * ```markdown
 * 支持 GitHub 风格短代码：
 *
 * - :smile: :joy: :wink:
 * - :rocket: :tada: :warning:
 *
 * 也可以直接输入原生 Emoji 😄🚀🎉。
 * ```
 */
export const emojiFeature: SupramarkFeature<SupramarkTextNode> = {
  metadata: {
    id: '@supramark/feature-emoji',
    name: 'Emoji',
    version: '0.1.0',
    author: 'Supramark Team',
    description: 'Emoji / 短代码支持（:smile: → 😄）',
    license: 'Apache-2.0',
    tags: ['emoji', 'shortcode'],
    syntaxFamily: 'main',
  },
  // Emoji - 无依赖（独立的字符替换功能）
  // dependencies: [] - 不显式声明空依赖
  // Emoji - 无依赖（独立的字符替换功能）
  // dependencies: [] - 不显式声明空依赖

  syntax: {
    ast: {
      type: 'text',

      interface: {
        required: ['type', 'value'],
        optional: [],
        fields: {
          type: {
            type: 'string',
            description: '节点类型，固定为 "text"。',
          },
          value: {
            type: 'string',
            description:
              '文本内容，其中的 Emoji 已经由 AST v2 parser 从短代码转换为 Unicode 字符。',
          },
        },
      },

      constraints: {
        allowedParents: ['paragraph', 'heading', 'list_item', 'table_cell', 'admonition'],
        allowedChildren: [],
      },

      examples: [
        {
          type: 'text',
          value: '这是一个包含 Emoji 😄🚀 的文本。',
        } as SupramarkTextNode,
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
        // Web 端使用 Unicode 字符 + 可选 Twemoji CDN
        needsClientScript: false,
        // 无需 Worker
        needsWorker: false,
        // 无需缓存
        needsCache: false,
      },

      // 依赖的外部库（可选）
      dependencies: [
        {
          name: 'twemoji',
          version: '^14.0.2',
          type: 'cdn',
          cdnUrl: 'https://cdn.jsdelivr.net/npm/twemoji@14.0.2/dist/twemoji.min.js',
          optional: true, // 可选依赖，默认使用系统 emoji
        },
      ],
    },

    // React Native 平台渲染器
    rn: {
      platform: 'rn',

      // 基础设施需求
      infrastructure: {
        // RN 端使用系统 emoji（Unicode 字符）
        needsWorker: false,
        // 无需缓存
        needsCache: false,
      },

      // 无外部依赖（使用系统 emoji）
      dependencies: [],
    },
  },

  // 使用示例
  examples: emojiExamples,

  // 测试定义
  testing: {
    // Markdown → AST 语法测试
    syntaxTests: {
      cases: [
        {
          name: '解析 emoji 短代码为 Unicode',
          input: ':smile:',
          expected: {
            type: 'text',
            value: '😄',
          } as SupramarkTextNode,
          options: {
            typeOnly: false,
            ignoreFields: ['position', 'data'],
          },
        },
        {
          name: '解析多个 emoji 短代码',
          input: ':rocket: :tada: :heart:',
          expected: {
            type: 'text',
            value: '🚀 🎉 ❤️',
          } as SupramarkTextNode,
          options: {
            typeOnly: false,
            ignoreFields: ['position', 'data'],
          },
        },
        {
          name: '解析文本中的 emoji',
          input: '我喜欢 :coffee: 和 :tea:',
          expected: {
            type: 'text',
            value: '我喜欢 ☕ 和 🍵',
          } as SupramarkTextNode,
          options: {
            typeOnly: false,
            ignoreFields: ['position', 'data'],
          },
        },
      ],
    },

    // AST → 渲染输出测试
    renderTests: {
      web: [
        {
          name: 'Web 渲染 emoji 文本',
          input: {
            type: 'text',
            value: '😄🚀🎉',
          } as SupramarkTextNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
      ],
      rn: [
        {
          name: 'RN 渲染 emoji 文本',
          input: {
            type: 'text',
            value: '❤️✨🌟',
          } as SupramarkTextNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
      ],
    },

    // 端到端集成测试
    integrationTests: {
      cases: [
        {
          name: 'Emoji 端到端：短代码转换',
          input: '测试 :smile: 和 :rocket:',
          validate: result => {
            if (!result || typeof result !== 'object') return false;
            const nodes = (result as SupramarkRootNode).children || [];
            return nodes.some(
              (n: SupramarkNode) =>
                n.type === 'paragraph' &&
                n.children?.some(
                  (c: SupramarkNode) =>
                    c.type === 'text' && (c.value.includes('😄') || c.value.includes('🚀'))
                )
            );
          },
          platforms: ['web', 'rn'],
        },
        {
          name: 'Emoji 端到端：原生 emoji',
          input: '直接使用 😄🚀',
          validate: result => {
            if (!result || typeof result !== 'object') return false;
            const nodes = (result as SupramarkRootNode).children || [];
            return nodes.some(
              (n: SupramarkNode) =>
                n.type === 'paragraph' &&
                n.children?.some((c: SupramarkNode) => c.type === 'text' && c.value.includes('😄'))
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
# Emoji Feature

为 Supramark 提供 Emoji 短代码支持。

## 功能

- GitHub 风格短代码
- 原生 Emoji

## 使用

查看 examples 目录获取更多示例。
    `.trim(),

    api: {
      interfaces: [
        {
          name: 'EmojiFeatureOptions',
          description: 'Emoji Feature 的配置选项接口（当前为空，保留用于未来扩展）',
          fields: [],
        },
        {
          name: 'SupramarkTextNode (with emoji)',
          description: '文本 AST 节点接口，Emoji 会被转换为 Unicode 字符嵌入在文本节点中',
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
              description:
                '文本内容，其中的 Emoji 已经由 AST v2 parser 从短代码（:smile:）转换为 Unicode 字符（😄）',
              required: true,
            },
          ],
        },
      ],

      functions: [
        {
          name: 'createEmojiFeatureConfig',
          description:
            '创建 Emoji Feature 配置对象，用于在 SupramarkConfig 中启用 Emoji 短代码支持',
          parameters: [
            {
              name: 'enabled',
              type: 'boolean',
              description: '是否启用 Emoji Feature',
              optional: false,
            },
            {
              name: 'options',
              type: 'EmojiFeatureOptions',
              description: 'Emoji Feature 配置选项（当前为空对象）',
              optional: true,
            },
          ],
          returns: 'FeatureConfigWithOptions<EmojiFeatureOptions>',
          examples: [
            `import { createEmojiFeatureConfig } from '@supramark/feature-emoji';

const config = {
  features: [
    createEmojiFeatureConfig(true),
  ],
};`,
          ],
        },
        {
          name: 'getEmojiFeatureOptions',
          description: '从 SupramarkConfig 中提取 Emoji Feature 的配置选项',
          parameters: [
            {
              name: 'config',
              type: 'SupramarkConfig',
              description: 'Supramark 配置对象',
              optional: true,
            },
          ],
          returns: 'EmojiFeatureOptions | undefined',
          examples: [
            `import { getEmojiFeatureOptions } from '@supramark/feature-emoji';

const options = getEmojiFeatureOptions(config);`,
          ],
        },
      ],

      types: [
        {
          name: 'EmojiFeatureConfig',
          description:
            'Emoji Feature 配置类型，是 FeatureConfigWithOptions<EmojiFeatureOptions> 的类型别名',
          definition: 'type EmojiFeatureConfig = FeatureConfigWithOptions<EmojiFeatureOptions>',
        },
      ],
    },

    bestPractices: [
      '使用 GitHub 风格的短代码格式，例如 :smile: :rocket: :heart:',
      '短代码使用英文冒号包裹，中间为 emoji 名称',
      '也可以直接输入原生 Unicode Emoji 字符',
      '常用 emoji 短代码：:+1: (👍)、:-1: (👎)、:tada: (🎉)、:sparkles: (✨)',
    ],

    faq: [
      {
        question: 'Emoji Feature 支持哪些短代码？',
        answer:
          '支持 GitHub 风格的 emoji 短代码，完整列表可参考 GitHub Emoji API。',
      },
      {
        question: 'Emoji 在 AST 中如何表示？',
        answer:
          'Emoji Feature 不创建单独的 AST 节点类型，而是将短代码转换为 Unicode 字符后嵌入到 text 节点的 value 中。',
      },
      {
        question: '可以直接使用 Unicode Emoji 吗？',
        answer: '可以。除了使用短代码，也可以直接在 Markdown 中输入原生 Unicode Emoji 字符。',
      },
    ],
  },
};

/**
 * Emoji Feature 的配置项。
 */
export interface EmojiFeatureOptions {
  // 当前为空，保留用于未来扩展
}

export type EmojiFeatureConfig = FeatureConfigWithOptions<EmojiFeatureOptions>;

const emojiHelpers = makeFeatureConfigHelpers<EmojiFeatureOptions>('@supramark/feature-emoji');
export const createEmojiFeatureConfig = emojiHelpers.create;
export const getEmojiFeatureOptions = emojiHelpers.getOptions;
