import type {
  SupramarkFeature,
  SupramarkNode,
  SupramarkRootNode,
  SupramarkListItemNode,
  FeatureConfigWithOptions,
} from '@supramark/core';
import { gfmExamples } from './examples.js';
import { makeFeatureConfigHelpers } from '@supramark/core';

/**
 * GFM Feature
 *
 * GitHub Flavored Markdown 扩展（删除线 / 任务列表 / 表格）
 *
 * @example
 * ```markdown
 * - [ ] 未完成任务
 * - [x] 已完成任务
 *
 * ~~删除线文本~~
 *
 * | 列 1 | 列 2 |
 * | ---- | ---- |
 * |  1   |  2   |
 * ```
 *
 * 节点类型说明：
 * - 如果此 Feature 只处理单一节点类型（如 'diagram'），直接使用当前配置即可
 * - 如果此 Feature 需要处理多个节点类型（如 'math_inline' 和 'math_block'），
 *   请参考下面的"多节点类型处理"注释，定义具体的节点接口和 selector
 */
export const gfmFeature: SupramarkFeature<SupramarkNode> = {
  metadata: {
    id: '@supramark/feature-gfm',
    name: 'GFM',
    version: '0.1.0',
    author: 'Supramark Team',
    description: 'GitHub Flavored Markdown 扩展（删除线 / 任务列表 / 表格）',
    license: 'Apache-2.0',
    tags: ['gfm', 'table', 'task-list', 'strikethrough'],
    syntaxFamily: 'main',
  },
  // GFM 扩展 - 依赖基础 Markdown（删除线、任务列表、表格的 children 都需要 core）
  dependencies: ['@supramark/feature-core-markdown'],

  syntax: {
    ast: {
      /**
       * 由于 GFM 涵盖多种节点（delete / 任务列表 / 表格），
       * 本 Feature 使用一个“虚拟”入口节点类型，并通过 selector 在运行时匹配：
       *
       * - 删除线：`node.type === 'delete'`
       * - 任务列表项：`node.type === 'list_item' && node.checked !== undefined`
       * - 表格相关：`node.type === 'table' | 'table_row' | 'table_cell'`
       *
       * 这里的 type 仅用于标识 Feature 归属，不直接等同于某个 AST 类型。
       */
      type: 'gfm',
      selector: (node: SupramarkNode) => {
        if (node.type === 'delete') return true;
        if (node.type === 'list_item' && 'checked' in node && node.checked !== undefined)
          return true;
        if (node.type === 'table' || node.type === 'table_row' || node.type === 'table_cell') {
          return true;
        }
        return false;
      },

      /**
       * 注意：GFM 是虚拟节点，不对应单一的 AST 节点类型
       * 它通过 selector 匹配多种实际节点（delete, table, task-list）
       * 因此不定义具体的 interface
       */
      // interface: undefined (虚拟节点不需要 interface)

      constraints: {
        allowedParents: ['root'],
        allowedChildren: [],
      },

      examples: [
        // 删除线节点示例
        {
          type: 'delete',
          children: [
            {
              type: 'text',
              value: '删除的文本',
            },
          ],
        } as SupramarkNode,
        // 表格节点示例
        {
          type: 'table',
          children: [
            {
              type: 'table_row',
              children: [
                {
                  type: 'table_cell',
                  children: [],
                },
              ],
            },
          ],
        } as SupramarkNode,
        // 任务列表项示例
        {
          type: 'list_item',
          checked: true,
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
        // Web 端使用基础 HTML 渲染（table / del / checkbox）
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
        // RN 端使用基础组件渲染
        needsWorker: false,
        // 无需缓存
        needsCache: false,
      },

      // 无外部依赖（使用 View / Text 组件）
      dependencies: [],
    },
  },

  // 使用示例
  examples: gfmExamples,

  // 测试定义
  testing: {
    // Markdown → AST 语法测试
    syntaxTests: {
      cases: [
        {
          name: '解析删除线',
          input: '这是 ~~删除的文本~~ 内容',
          expected: {
            type: 'delete',
            children: [{ type: 'text', value: '删除的文本' }],
          } as SupramarkNode,
          options: {
            typeOnly: false,
            ignoreFields: ['position', 'data'],
          },
        },
        {
          name: '解析任务列表',
          input: '- [x] 已完成任务\n- [ ] 未完成任务',
          expected: [
            {
              type: 'list_item',
              checked: true,
            } as SupramarkNode,
            {
              type: 'list_item',
              checked: false,
            } as SupramarkNode,
          ],
          options: {
            typeOnly: false,
            ignoreFields: ['children', 'position', 'data'],
          },
        },
        {
          name: '解析表格',
          input: '| 列1 | 列2 |\n| --- | --- |\n| A | B |',
          expected: {
            type: 'table',
            children: [],
          } as SupramarkNode,
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
          name: 'Web 渲染删除线',
          input: {
            type: 'delete',
            children: [{ type: 'text', value: '删除内容' }],
          } as SupramarkNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
        {
          name: 'Web 渲染任务列表',
          input: {
            type: 'list_item',
            checked: true,
            children: [{ type: 'text', value: '任务' }],
          } as SupramarkNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
      ],
      rn: [
        {
          name: 'RN 渲染表格',
          input: {
            type: 'table',
            children: [
              {
                type: 'table_row',
                children: [
                  { type: 'table_cell', header: true, children: [{ type: 'text', value: '标题' }] },
                ],
              },
            ],
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
          name: 'GFM 端到端：删除线 + 任务列表',
          input: '~~删除~~ 文本\n\n- [x] 任务1\n- [ ] 任务2',
          validate: result => {
            if (!result || typeof result !== 'object') return false;
            const nodes = (result as SupramarkRootNode).children || [];
            const hasDelete = nodes.some(
              (n: SupramarkNode) =>
                n.type === 'paragraph' &&
                n.children?.some((c: SupramarkNode) => c.type === 'delete')
            );
            const hasTaskList = nodes.some(
              (n: SupramarkNode) =>
                n.type === 'list' &&
                n.children?.some(
                  (item: SupramarkNode) =>
                    (item as SupramarkListItemNode).checked !== undefined
                )
            );
            return hasDelete && hasTaskList;
          },
          platforms: ['web', 'rn'],
        },
        {
          name: 'GFM 端到端：完整表格',
          input: '| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |',
          validate: result => {
            if (!result || typeof result !== 'object') return false;
            const nodes = (result as SupramarkRootNode).children || [];
            const hasTable = nodes.some((n: SupramarkNode) => n.type === 'table');
            return hasTable;
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
# GFM Feature

为 Supramark 提供 GitHub Flavored Markdown 扩展支持。

## 功能

- 删除线
- 任务列表
- 表格

## 使用

查看 examples 目录获取更多示例。
    `.trim(),

    api: {
      interfaces: [
        {
          name: 'GFMFeatureOptions',
          description: 'GFM Feature 的配置选项接口（当前为空，保留用于未来扩展）',
          fields: [],
        },
        {
          name: 'SupramarkDeleteNode',
          description: '删除线 AST 节点接口，用于表示被删除的文本（~~...~~）',
          fields: [
            {
              name: 'type',
              type: "'delete'",
              description: '节点类型标识，固定为 "delete"',
              required: true,
            },
            {
              name: 'children',
              type: 'SupramarkNode[]',
              description: '删除线内部的子节点（通常包含 text 节点）',
              required: true,
            },
          ],
        },
        {
          name: 'SupramarkTableNode',
          description: '表格 AST 节点接口，用于表示 Markdown 表格',
          fields: [
            {
              name: 'type',
              type: "'table'",
              description: '节点类型标识，固定为 "table"',
              required: true,
            },
            {
              name: 'children',
              type: 'SupramarkTableRowNode[]',
              description: '表格行节点数组',
              required: true,
            },
            {
              name: 'align',
              type: "Array<'left' | 'right' | 'center' | null>",
              description: '每列的对齐方式配置',
              required: false,
            },
          ],
        },
        {
          name: 'SupramarkTableRowNode',
          description: '表格行 AST 节点接口',
          fields: [
            {
              name: 'type',
              type: "'table_row'",
              description: '节点类型标识，固定为 "table_row"',
              required: true,
            },
            {
              name: 'children',
              type: 'SupramarkTableCellNode[]',
              description: '表格单元格节点数组',
              required: true,
            },
          ],
        },
        {
          name: 'SupramarkTableCellNode',
          description: '表格单元格 AST 节点接口',
          fields: [
            {
              name: 'type',
              type: "'table_cell'",
              description: '节点类型标识，固定为 "table_cell"',
              required: true,
            },
            {
              name: 'children',
              type: 'SupramarkNode[]',
              description: '单元格内部的子节点',
              required: true,
            },
            {
              name: 'header',
              type: 'boolean',
              description: '是否为表头单元格',
              required: false,
            },
          ],
        },
        {
          name: 'SupramarkListItemNode (with task list)',
          description: '任务列表项 AST 节点接口，扩展自标准列表项，增加了 checked 属性',
          fields: [
            {
              name: 'type',
              type: "'list_item'",
              description: '节点类型标识，固定为 "list_item"',
              required: true,
            },
            {
              name: 'checked',
              type: 'boolean | null',
              description:
                '任务完成状态：true（已完成）、false（未完成）、null/undefined（普通列表项）',
              required: false,
            },
            {
              name: 'children',
              type: 'SupramarkNode[]',
              description: '列表项内部的子节点',
              required: true,
            },
          ],
        },
      ],

      functions: [
        {
          name: 'createGFMFeatureConfig',
          description:
            '创建 GFM Feature 配置对象，用于在 SupramarkConfig 中启用 GitHub Flavored Markdown 扩展',
          parameters: [
            {
              name: 'enabled',
              type: 'boolean',
              description: '是否启用 GFM Feature',
              optional: false,
            },
            {
              name: 'options',
              type: 'GFMFeatureOptions',
              description: 'GFM Feature 配置选项（当前为空对象）',
              optional: true,
            },
          ],
          returns: 'FeatureConfigWithOptions<GFMFeatureOptions>',
          examples: [
            `import { createGFMFeatureConfig } from '@supramark/feature-gfm';

const config = {
  features: [
    createGFMFeatureConfig(true),
  ],
};`,
          ],
        },
        {
          name: 'getGFMFeatureOptions',
          description: '从 SupramarkConfig 中提取 GFM Feature 的配置选项',
          parameters: [
            {
              name: 'config',
              type: 'SupramarkConfig',
              description: 'Supramark 配置对象',
              optional: true,
            },
          ],
          returns: 'GFMFeatureOptions | undefined',
          examples: [
            `import { getGFMFeatureOptions } from '@supramark/feature-gfm';

const options = getGFMFeatureOptions(config);`,
          ],
        },
      ],

      types: [
        {
          name: 'GFMFeatureConfig',
          description:
            'GFM Feature 配置类型，是 FeatureConfigWithOptions<GFMFeatureOptions> 的类型别名',
          definition: 'type GFMFeatureConfig = FeatureConfigWithOptions<GFMFeatureOptions>',
        },
      ],
    },

    bestPractices: [
      '使用 ~~ 包裹需要删除的文本',
      '任务列表使用 - [ ] 表示未完成，- [x] 表示已完成',
      '表格使用 | 分隔列，使用 --- 定义表头分隔线',
      '表格对齐使用 :--- (左对齐)、:---: (居中)、---: (右对齐)',
    ],

    faq: [
      {
        question: 'GFM Feature 包含哪些功能？',
        answer: 'GFM Feature 包含删除线（~~text~~）、任务列表（- [ ] / - [x]）和表格三个核心功能。',
      },
      {
        question: '如何创建任务列表？',
        answer: '使用 - [ ] 创建未完成任务，使用 - [x] 创建已完成任务。注意方括号内的空格。',
      },
      {
        question: '表格如何设置对齐方式？',
        answer: '在表头分隔线中使用冒号设置对齐：:--- 左对齐，:---: 居中，---: 右对齐。',
      },
    ],
  },
};

/**
 * GFM Feature 的配置项。
 */
export interface GFMFeatureOptions {
  // 当前为空，保留用于未来扩展
}

export type GFMFeatureConfig = FeatureConfigWithOptions<GFMFeatureOptions>;

const gfmHelpers = makeFeatureConfigHelpers<GFMFeatureOptions>('@supramark/feature-gfm');
export const createGFMFeatureConfig = gfmHelpers.create;
export const getGFMFeatureOptions = gfmHelpers.getOptions;

// Backward/usage compatibility: alias with lower-case 'fm' to match examples
export function createGfmFeatureConfig(
  enabled = true,
  options?: GFMFeatureOptions
): GFMFeatureConfig {
  return createGFMFeatureConfig(enabled, options);
}
