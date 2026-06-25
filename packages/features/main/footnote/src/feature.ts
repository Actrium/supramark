import type {
  SupramarkFeature,
  SupramarkNode,
  SupramarkRootNode,
  SupramarkFootnoteReferenceNode,
  SupramarkFootnoteDefinitionNode,
  FeatureConfigWithOptions,
} from '@supramark/core';
import { footnoteExamples } from './examples.js';
import { makeFeatureConfigHelpers } from '@supramark/core';

/**
 * Footnote Feature
 *
 * 脚注语法支持（引用 + 定义）的规范描述：
 *
 * - 复用 core 中已实现的 `footnote_reference` / `footnote_definition` AST；
 * - 不负责实际解析与渲染逻辑；
 * - 主要用于：文档、能力发现、Feature 配置桥梁。
 *
 * @example
 * ```markdown
 * 这是正文[^1]，以及一个内联脚注 ^[内联脚注内容]。
 *
 * [^1]: 这里是脚注定义内容。
 * ```
 *
 * 节点类型说明：
 * - 如果此 Feature 只处理单一节点类型（如 'diagram'），直接使用当前配置即可
 * - 如果此 Feature 需要处理多个节点类型（如 'math_inline' 和 'math_block'），
 *   请参考下面的"多节点类型处理"注释，定义具体的节点接口和 selector
 */
export const footnoteFeature: SupramarkFeature<
  SupramarkFootnoteReferenceNode | SupramarkFootnoteDefinitionNode
> = {
  metadata: {
    id: '@supramark/feature-footnote',
    name: 'Footnote',
    version: '0.1.0',
    author: 'Supramark Team',
    description: '脚注语法支持（引用 + 定义）',
    license: 'Apache-2.0',
    tags: ['footnote', 'reference', 'definition'],
    syntaxFamily: 'main',
  },
  // Footnote - 依赖基础 Markdown（脚注定义可以包含段落等）
  dependencies: ['@supramark/feature-core-markdown'],

  syntax: {
    ast: {
      type: 'footnote_reference',

      selector: (node: SupramarkNode) =>
        node.type === 'footnote_reference' || node.type === 'footnote_definition',

      interface: {
        required: ['type', 'index'],
        optional: ['label'],
        fields: {
          type: {
            type: 'string',
            description:
              '节点类型："footnote_reference"（正文引用）或 "footnote_definition"（文末定义）。',
          },
          index: {
            type: 'number',
            description: '脚注编号（从 1 开始），由解析管线统一分配。',
          },
          label: {
            type: 'string',
            description: '原始 label，例如 [^note] 中的 "note"。',
          },
        },
      },

      constraints: {
        allowedParents: ['root', 'paragraph', 'list_item'],
        allowedChildren: ['paragraph', 'list', 'code', 'blockquote'],
      },

      examples: [
        {
          type: 'footnote_reference',
          index: 1,
          label: '1',
          identifier: '1',
        } as SupramarkFootnoteReferenceNode,
        {
          type: 'footnote_definition',
          index: 1,
          label: '1',
          identifier: '1',
          children: [],
        } as SupramarkFootnoteDefinitionNode,
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
        // Web 端使用锚点链接（<a href="#fn1">）
        needsClientScript: false,
        // 无需 Worker
        needsWorker: false,
        // 无需缓存
        needsCache: false,
      },

      // 无外部依赖（使用标准 HTML 锚点）
      dependencies: [],
    },

    // React Native 平台渲染器
    rn: {
      platform: 'rn',

      // 基础设施需求
      infrastructure: {
        // RN 端使用 ScrollView ref 实现跳转
        needsWorker: false,
        // 无需缓存
        needsCache: false,
      },

      // 无外部依赖（使用 ScrollView ref）
      dependencies: [],
    },
  },

  // 使用示例
  examples: footnoteExamples,

  // 测试定义
  testing: {
    // Markdown → AST 语法测试
    syntaxTests: {
      cases: [
        {
          name: '解析脚注引用',
          input: '文本[^1]',
          expected: {
            type: 'footnote_reference',
            index: 1,
            label: '1',
          } as SupramarkFootnoteReferenceNode,
          options: {
            typeOnly: false,
            ignoreFields: ['position', 'data', 'subId'],
          },
        },
        {
          name: '解析脚注定义',
          input: '[^1]: 脚注内容',
          expected: {
            type: 'footnote_definition',
            index: 1,
            label: '1',
          } as SupramarkFootnoteDefinitionNode,
          options: {
            typeOnly: false,
            ignoreFields: ['children', 'position', 'data'],
          },
        },
        {
          name: '解析多个脚注引用',
          input: '文本[^1]和[^2]',
          expected: [
            {
              type: 'footnote_reference',
              index: 1,
            } as SupramarkFootnoteReferenceNode,
            {
              type: 'footnote_reference',
              index: 2,
            } as SupramarkFootnoteReferenceNode,
          ],
          options: {
            typeOnly: false,
            ignoreFields: ['label', 'position', 'data', 'subId'],
          },
        },
      ],
    },

    // AST → 渲染输出测试
    renderTests: {
      web: [
        {
          name: 'Web 渲染脚注引用',
          input: {
            type: 'footnote_reference',
            index: 1,
            label: '1',
          } as SupramarkFootnoteReferenceNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
        {
          name: 'Web 渲染脚注定义',
          input: {
            type: 'footnote_definition',
            index: 1,
            label: '1',
            children: [{ type: 'paragraph', children: [{ type: 'text', value: '脚注内容' }] }],
          } as SupramarkFootnoteDefinitionNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
      ],
      rn: [
        {
          name: 'RN 渲染脚注引用',
          input: {
            type: 'footnote_reference',
            index: 2,
            label: 'note',
          } as SupramarkFootnoteReferenceNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
      ],
    },

    // 端到端集成测试
    integrationTests: {
      cases: [
        {
          name: 'Footnote 端到端：引用 + 定义',
          input: '正文[^1]\n\n[^1]: 脚注内容',
          validate: result => {
            if (!result || typeof result !== 'object') return false;
            const nodes = (result as SupramarkRootNode).children || [];
            const hasReference = nodes.some(
              (n: SupramarkNode) =>
                n.type === 'paragraph' &&
                n.children?.some((c: SupramarkNode) => c.type === 'footnote_reference')
            );
            const hasDefinition = nodes.some(
              (n: SupramarkNode) => n.type === 'footnote_definition'
            );
            return hasReference && hasDefinition;
          },
          platforms: ['web', 'rn'],
        },
        {
          name: 'Footnote 端到端：多个脚注',
          input: '文本[^1]和[^2]\n\n[^1]: 第一个\n[^2]: 第二个',
          validate: result => {
            if (!result || typeof result !== 'object') return false;
            const nodes = (result as SupramarkRootNode).children || [];
            const references = nodes.reduce((count: number, n: SupramarkNode) => {
              if (n.type === 'paragraph' && Array.isArray(n.children)) {
                return (
                  count +
                  n.children.filter((c: SupramarkNode) => c.type === 'footnote_reference').length
                );
              }
              return count;
            }, 0);
            const definitions = nodes.filter(
              (n: SupramarkNode) => n.type === 'footnote_definition'
            ).length;
            return references >= 2 && definitions >= 2;
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
# Footnote Feature

为 Supramark 提供脚注支持。

## 功能

- 脚注引用
- 脚注定义

## 使用

查看 examples 目录获取更多示例。
    `.trim(),

    api: {
      interfaces: [
        {
          name: 'FootnoteFeatureOptions',
          description: 'Footnote Feature 的配置选项接口（当前为空，保留用于未来扩展）',
          fields: [],
        },
        {
          name: 'SupramarkFootnoteReferenceNode',
          description: '脚注引用 AST 节点接口，用于表示正文中的脚注引用（[^1]）',
          fields: [
            {
              name: 'type',
              type: "'footnote_reference'",
              description: '节点类型标识，固定为 "footnote_reference"',
              required: true,
            },
            {
              name: 'index',
              type: 'number',
              description: '脚注编号（从 1 开始），由解析管线统一分配',
              required: true,
            },
            {
              name: 'label',
              type: 'string',
              description: '原始 label，例如 [^note] 中的 "note"',
              required: false,
            },
            {
              name: 'subId',
              type: 'string',
              description: '子 ID，用于同一脚注的多次引用',
              required: false,
            },
          ],
        },
        {
          name: 'SupramarkFootnoteDefinitionNode',
          description: '脚注定义 AST 节点接口，用于表示文末的脚注定义内容（[^1]: ...）',
          fields: [
            {
              name: 'type',
              type: "'footnote_definition'",
              description: '节点类型标识，固定为 "footnote_definition"',
              required: true,
            },
            {
              name: 'index',
              type: 'number',
              description: '脚注编号（从 1 开始），与引用的 index 对应',
              required: true,
            },
            {
              name: 'label',
              type: 'string',
              description: '原始 label，例如 [^note] 中的 "note"',
              required: false,
            },
            {
              name: 'children',
              type: 'SupramarkNode[]',
              description: '脚注内容节点列表，可包含段落、列表、代码块等',
              required: true,
            },
          ],
        },
      ],

      functions: [
        {
          name: 'createFootnoteFeatureConfig',
          description: '创建 Footnote Feature 配置对象，用于在 SupramarkConfig 中启用脚注支持',
          parameters: [
            {
              name: 'enabled',
              type: 'boolean',
              description: '是否启用 Footnote Feature',
              optional: false,
            },
            {
              name: 'options',
              type: 'FootnoteFeatureOptions',
              description: 'Footnote Feature 配置选项（当前为空对象）',
              optional: true,
            },
          ],
          returns: 'FeatureConfigWithOptions<FootnoteFeatureOptions>',
          examples: [
            `import { createFootnoteFeatureConfig } from '@supramark/feature-footnote';

const config = {
  features: [
    createFootnoteFeatureConfig(true),
  ],
};`,
          ],
        },
        {
          name: 'getFootnoteFeatureOptions',
          description: '从 SupramarkConfig 中提取 Footnote Feature 的配置选项',
          parameters: [
            {
              name: 'config',
              type: 'SupramarkConfig',
              description: 'Supramark 配置对象',
              optional: true,
            },
          ],
          returns: 'FootnoteFeatureOptions | undefined',
          examples: [
            `import { getFootnoteFeatureOptions } from '@supramark/feature-footnote';

const options = getFootnoteFeatureOptions(config);`,
          ],
        },
      ],

      types: [
        {
          name: 'FootnoteFeatureConfig',
          description:
            'Footnote Feature 配置类型，是 FeatureConfigWithOptions<FootnoteFeatureOptions> 的类型别名',
          definition:
            'type FootnoteFeatureConfig = FeatureConfigWithOptions<FootnoteFeatureOptions>',
        },
      ],
    },

    bestPractices: [
      '脚注引用使用 [^label] 格式，label 可以是数字或文字',
      '脚注定义使用 [^label]: 内容 格式，通常放在文档末尾',
      '同一脚注可以被多次引用，系统会自动处理',
      '脚注编号由系统自动分配，按照引用出现的顺序从 1 开始',
    ],

    faq: [
      {
        question: '脚注的语法格式是什么？',
        answer: '引用格式：[^label]，定义格式：[^label]: 脚注内容。label 可以是数字或文字标识符。',
      },
      {
        question: '脚注编号如何确定？',
        answer: '脚注编号由解析器自动分配，按照脚注引用在文档中出现的顺序从 1 开始递增。',
      },
      {
        question: '同一脚注可以被多次引用吗？',
        answer: '可以。多次引用同一 label 的脚注会生成多个引用节点，它们共享同一个定义和编号。',
      },
      {
        question: '脚注定义可以包含哪些内容？',
        answer:
          '脚注定义可以包含段落、列表、代码块、引用块等多种 Markdown 元素，支持复杂的内容结构。',
      },
    ],
  },
};

/**
 * Footnote Feature 的配置项。
 */
export interface FootnoteFeatureOptions {
  // 当前为空，保留用于未来扩展
}

export type FootnoteFeatureConfig = FeatureConfigWithOptions<FootnoteFeatureOptions>;

const footnoteHelpers = makeFeatureConfigHelpers<FootnoteFeatureOptions>(
  '@supramark/feature-footnote'
);
export const createFootnoteFeatureConfig = footnoteHelpers.create;
export const getFootnoteFeatureOptions = footnoteHelpers.getOptions;
