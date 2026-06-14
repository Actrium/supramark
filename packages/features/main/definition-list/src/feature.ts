import type {
  SupramarkFeature,
  SupramarkDefinitionListNode,
  FeatureConfigWithOptions,
  SupramarkConfig,
} from '@supramark/core';
import { definitionListExamples } from './examples.js';
import { makeFeatureConfigHelpers } from '@supramark/core';

/**
 * Definition List Feature
 *
 * 定义列表语法支持（Term + 多段描述）的规范定义。
 *
 * - 复用 core 中 `definition_list` / `definition_item` AST；
 * - AST v2 中 definition_item 通过 children 承载 definition_term / definition_description；
 * - 解析逻辑由 supramark-markdown AST v2 parser 实现；
 * - 渲染逻辑由 @supramark/rn / @supramark/web 负责。
 *
 * @example
 * ```markdown
 * TODO: 添加 Markdown 示例
 * ```
 *
 * 节点类型说明：
 * - 如果此 Feature 只处理单一节点类型（如 'diagram'），直接使用当前配置即可
 * - 如果此 Feature 需要处理多个节点类型（如 'math_inline' 和 'math_block'），
 *   请参考下面的"多节点类型处理"注释，定义具体的节点接口和 selector
 */
export const definitionListFeature: SupramarkFeature<SupramarkDefinitionListNode> = {
  metadata: {
    id: '@supramark/feature-definition-list',
    name: 'Definition List',
    version: '0.1.0',
    author: 'Supramark Team',
    description: '定义列表语法支持（Term + 多段描述）',
    license: 'Apache-2.0',
    tags: ['definition-list', 'dl', 'term'],
    syntaxFamily: 'main',
  },
  // Definition List - 依赖基础 Markdown（term 和 descriptions 可以包含 inline/block 节点）
  dependencies: ['@supramark/feature-core-markdown'],

  syntax: {
    ast: {
      type: 'definition_list',

      interface: {
        required: ['type', 'children'],
        optional: [],
        fields: {
          type: {
            type: 'string',
            description: '节点类型，固定为 "definition_list"。',
          },
          children: {
            type: 'nodes',
            description: '定义列表条目数组，每个条目为 definition_item 节点。',
          },
        },
      },

      constraints: {
        allowedParents: ['root'],
        allowedChildren: ['definition_item'],
      },

      examples: [
        {
          type: 'definition_list',
          children: [],
        } as SupramarkDefinitionListNode,
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
        // Web 端使用语义 HTML 元素（dl / dt / dd）
        needsClientScript: false,
        // 无需 Worker
        needsWorker: false,
        // 无需缓存
        needsCache: false,
      },

      // 无外部依赖（使用标准 HTML dl 元素）
      dependencies: [],
    },

    // React Native 平台渲染器
    rn: {
      platform: 'rn',

      // 基础设施需求
      infrastructure: {
        // RN 端使用 View + Text 组件渲染
        needsWorker: false,
        // 无需缓存
        needsCache: false,
      },

      // 无外部依赖（使用 View / Text 组件）
      dependencies: [],
    },
  },

  // 使用示例
  examples: definitionListExamples,

  // 测试定义
  testing: {
    // Markdown → AST 语法测试
    syntaxTests: {
      cases: [
        {
          name: '解析简单定义列表',
          input: 'Term\n:   Definition',
          expected: {
            type: 'definition_list',
            children: [],
          } as SupramarkDefinitionListNode,
          options: {
            typeOnly: true,
          },
        },
        {
          name: '解析多个定义的术语',
          input: 'Apple\n:   水果\n:   公司名',
          expected: {
            type: 'definition_list',
          } as SupramarkDefinitionListNode,
          options: {
            typeOnly: true,
          },
        },
        {
          name: '解析多个术语',
          input: 'HTML\nCSS\n:   网页技术',
          expected: {
            type: 'definition_list',
          } as SupramarkDefinitionListNode,
          options: {
            typeOnly: true,
          },
        },
      ],
    },

    // AST → 渲染输出测试
    renderTests: {
      web: [
        {
          name: 'Web 渲染定义列表',
          input: {
            type: 'definition_list',
            children: [
              {
                type: 'definition_item',
                children: [
                  {
                    type: 'definition_term',
                    children: [{ type: 'text', value: 'Term' }],
                  },
                  {
                    type: 'definition_description',
                    children: [
                      {
                        type: 'paragraph',
                        children: [{ type: 'text', value: 'Definition' }],
                      },
                    ],
                  },
                ],
              },
            ],
          } as SupramarkDefinitionListNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
      ],
      rn: [
        {
          name: 'RN 渲染定义列表',
          input: {
            type: 'definition_list',
            children: [
              {
                type: 'definition_item',
                children: [
                  {
                    type: 'definition_term',
                    children: [{ type: 'text', value: 'API' }],
                  },
                  {
                    type: 'definition_description',
                    children: [
                      {
                        type: 'paragraph',
                        children: [{ type: 'text', value: '应用程序接口' }],
                      },
                    ],
                  },
                ],
              },
            ],
          } as SupramarkDefinitionListNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
      ],
    },

    // 端到端集成测试
    integrationTests: {
      cases: [
        {
          name: 'DefinitionList 端到端：单个定义',
          input: 'Markdown\n:   轻量级标记语言',
          validate: result => {
            if (!result || typeof result !== 'object') return false;
            const nodes = (result as any).children || [];
            return nodes.some((n: any) => n.type === 'definition_list');
          },
          platforms: ['web', 'rn'],
        },
        {
          name: 'DefinitionList 端到端：多个定义',
          input: 'TypeScript\n:   强类型 JavaScript\n:   微软开发',
          validate: result => {
            if (!result || typeof result !== 'object') return false;
            const nodes = (result as any).children || [];
            const defList = nodes.find((n: any) => n.type === 'definition_list');
            if (!defList) return false;
            const items = defList.children || [];
            return items.some(
              (item: any) =>
                item.type === 'definition_item' &&
                Array.isArray(item.children) &&
                item.children.some((child: any) => child.type === 'definition_description')
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
# Definition List Feature

为 Supramark 提供定义列表支持。

## 功能

- 术语定义
- 多段描述

## 使用

查看 examples 目录获取更多示例。
    `.trim(),

    api: {
      interfaces: [
        {
          name: 'DefinitionListFeatureOptions',
          description: 'Definition List Feature 的配置选项接口（当前为空，保留用于未来扩展）',
          fields: [],
        },
        {
          name: 'SupramarkDefinitionListNode',
          description: '定义列表 AST 节点接口，用于表示术语及其定义的列表',
          fields: [
            {
              name: 'type',
              type: "'definition_list'",
              description: '节点类型标识，固定为 "definition_list"',
              required: true,
            },
            {
              name: 'children',
              type: 'SupramarkDefinitionItemNode[]',
              description: '定义列表条目数组，每个条目为一个术语及其定义',
              required: true,
            },
          ],
        },
        {
          name: 'SupramarkDefinitionItemNode',
          description: '定义列表项 AST 节点接口，通过 children 包含术语和定义描述',
          fields: [
            {
              name: 'type',
              type: "'definition_item'",
              description: '节点类型标识，固定为 "definition_item"',
              required: true,
            },
            {
              name: 'children',
              type: 'Array<SupramarkDefinitionTermNode | SupramarkDefinitionDescriptionNode>',
              description: '按源码顺序排列的 definition_term 与 definition_description 子节点',
              required: true,
            },
          ],
        },
      ],

      functions: [
        {
          name: 'createDefinitionListFeatureConfig',
          description:
            '创建 Definition List Feature 配置对象，用于在 SupramarkConfig 中启用定义列表支持',
          parameters: [
            {
              name: 'enabled',
              type: 'boolean',
              description: '是否启用 Definition List Feature',
              optional: false,
            },
            {
              name: 'options',
              type: 'DefinitionListFeatureOptions',
              description: 'Definition List Feature 配置选项（当前为空对象）',
              optional: true,
            },
          ],
          returns: 'FeatureConfigWithOptions<DefinitionListFeatureOptions>',
          examples: [
            `import { createDefinitionListFeatureConfig } from '@supramark/feature-definition-list';

const config = {
  features: [
    createDefinitionListFeatureConfig(true),
  ],
};`,
          ],
        },
        {
          name: 'getDefinitionListFeatureOptions',
          description: '从 SupramarkConfig 中提取 Definition List Feature 的配置选项',
          parameters: [
            {
              name: 'config',
              type: 'SupramarkConfig',
              description: 'Supramark 配置对象',
              optional: true,
            },
          ],
          returns: 'DefinitionListFeatureOptions | undefined',
          examples: [
            `import { getDefinitionListFeatureOptions } from '@supramark/feature-definition-list';

const options = getDefinitionListFeatureOptions(config);`,
          ],
        },
      ],

      types: [
        {
          name: 'DefinitionListFeatureConfig',
          description:
            'Definition List Feature 配置类型，是 FeatureConfigWithOptions<DefinitionListFeatureOptions> 的类型别名',
          definition:
            'type DefinitionListFeatureConfig = FeatureConfigWithOptions<DefinitionListFeatureOptions>',
        },
      ],
    },

    bestPractices: [
      '术语单独占一行，定义以 :   开头（冒号后至少 3 个空格或 1 个 tab）',
      '一个术语可以有多个定义，每个定义单独一行并以 :   开头',
      '多个术语可以共享同一个定义',
      '定义内容支持多段落，使用缩进保持结构',
    ],

    faq: [
      {
        question: '定义列表的语法格式是什么？',
        answer:
          '术语单独一行，定义以 :   开头（冒号后至少 3 个空格或 1 个 tab）。例如：Term\\n:   Definition',
      },
      {
        question: '一个术语可以有多个定义吗？',
        answer:
          '可以。每个定义单独一行并以 :   开头即可，例如：Term\\n:   Definition 1\\n:   Definition 2',
      },
      {
        question: '多个术语可以共享定义吗？',
        answer: '可以。连续写多个术语，然后写一个定义，这些术语将共享该定义。',
      },
    ],
  },
};

/**
 * Definition List Feature 的配置项。
 */
export interface DefinitionListFeatureOptions {
  // 当前为空，保留用于未来扩展
}

export type DefinitionListFeatureConfig = FeatureConfigWithOptions<DefinitionListFeatureOptions>;

const definitionListHelpers = makeFeatureConfigHelpers<DefinitionListFeatureOptions>(
  '@supramark/feature-definition-list'
);
export const createDefinitionListFeatureConfig = definitionListHelpers.create;
export const getDefinitionListFeatureOptions = definitionListHelpers.getOptions;
