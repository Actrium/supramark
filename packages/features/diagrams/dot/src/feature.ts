import type {
  SupramarkNode,
  SupramarkDiagramNode,
  FeatureConfigWithOptions,
  SupramarkConfig,
  SupramarkFeature,
} from '@supramark/core';
import { FeatureRegistry, makeFeatureConfigHelpers } from '@supramark/core';
import { diagramDotExamples } from './examples.js';

/**
 * DOT / Graphviz 图表 Feature
 *
 * - 复用通用 `diagram` AST 节点；
 * - 只关心 engine 为 'dot' 或 'graphviz' 的 diagram；
 * - 由 `@supramark/engines` 在 RN / Web 侧将 DOT 源码转换为 SVG。
 *
 * @example
 * ```markdown
 * ```dot
 * digraph G { A -> B }
 * ```
 * ```
 */

const isDotDiagram = (node: SupramarkNode): node is SupramarkDiagramNode => {
  return (
    node.type === 'diagram' &&
    typeof (node as SupramarkDiagramNode).engine === 'string' &&
    ['dot', 'graphviz'].includes((node as SupramarkDiagramNode).engine.toLowerCase())
  );
};

export const diagramDotFeature: SupramarkFeature<SupramarkDiagramNode> = {
  metadata: {
    id: '@supramark/feature-diagram-dot',
    name: 'Diagram (DOT / Graphviz)',
    version: '0.1.0',
    author: 'Supramark Team',
    description: 'DOT / Graphviz diagrams rendered to SVG through @supramark/engines.',
    license: 'Apache-2.0',
    tags: ['diagram', 'dot', 'graphviz'],
    syntaxFamily: 'fence',
  },

  syntax: {
    ast: {
      type: 'diagram',
      selector: isDotDiagram,
      interface: {
        required: ['type', 'engine', 'code'],
        optional: ['meta'],
        fields: {
          type: {
            type: 'string',
            description: 'Node type identifier, always "diagram".',
          },
          engine: {
            type: 'string',
            description: 'Diagram engine identifier, "dot" or "graphviz".',
          },
          code: {
            type: 'string',
            description: 'Raw DOT source text from the fenced code block.',
          },
          meta: {
            type: 'object',
            description:
              'Optional metadata reserved for future Graphviz integration (layout engine, options, etc.).',
          },
        },
      },
      examples: [
        {
          type: 'diagram',
          engine: 'dot',
          code: 'digraph G { A -> B }',
        } as SupramarkDiagramNode,
      ],
    },
  },

  renderers: {
    rn: {
      platform: 'rn',
      infrastructure: {
        needsWorker: false,
        needsCache: false,
      },
    },
    web: {
      platform: 'web',
      infrastructure: {
        needsClientScript: false,
      },
    },
  },

  examples: diagramDotExamples,

  testing: {
    syntaxTests: {
      cases: [
        {
          name: '解析 dot 围栏为 diagram 节点',
          input: ['```dot', 'digraph G { A -> B }', '```'].join('\n'),
          expected: {
            type: 'diagram',
            engine: 'dot',
          } as unknown as SupramarkDiagramNode,
          options: {
            typeOnly: true,
          },
        },
      ],
    },
    renderTests: {
      web: [
        {
          name: 'Web 渲染 DOT diagram',
          input: {
            type: 'diagram',
            engine: 'dot',
            code: 'digraph G { A -> B }',
          } as SupramarkDiagramNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: false,
        },
      ],
      rn: [
        {
          name: 'RN 渲染 DOT diagram',
          input: {
            type: 'diagram',
            engine: 'dot',
            code: 'digraph G { A -> B }',
          } as SupramarkDiagramNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: false,
        },
      ],
    },
    integrationTests: {
      cases: [
        {
          name: '端到端：markdown 中包含 ```dot 围栏',
          input: ['# DOT demo', '', '```dot', 'digraph G { A -> B }', '```'].join('\n'),
          validate: (result: unknown) => {
            if (!result || typeof result !== 'object') return false;
            const root = result as any;
            const children = Array.isArray(root.children) ? root.children : [];
            return children.some(
              (n: any) =>
                n.type === 'diagram' &&
                (String(n.engine).toLowerCase() === 'dot' ||
                  String(n.engine).toLowerCase() === 'graphviz')
            );
          },
          platforms: ['web', 'rn'],
        },
      ],
    },
    coverageRequirements: {
      statements: 40,
      branches: 30,
      functions: 30,
      lines: 40,
    },
  },

  documentation: {
    readme: `
# Diagram (DOT / Graphviz) Feature

为 supramark 提供 DOT / Graphviz 围栏代码块的 AST 建模，并在 RN / Web 端通过 Graphviz 渲染为 SVG。

- 语法：使用 \`\\\`\\\`dot\` 或 \`\\\`\\\`graphviz\` 围栏；
- AST：解析为 \`diagram\` 节点，engine = "dot" 或 "graphviz"，code 为 DOT 源码；
- 渲染：由 \`@supramark/engines\` 基于 Graphviz 渲染为 SVG。
    `.trim(),

    api: {
      interfaces: [
        {
          name: 'DiagramDotFeatureOptions',
          description: 'DOT / Graphviz Feature 的配置选项。',
          fields: [],
        },
      ],
      functions: [
        {
          name: 'createDiagramDotFeatureConfig',
          description: '创建 Diagram (DOT / Graphviz) Feature 的配置对象。',
          parameters: [
            {
              name: 'enabled',
              type: 'boolean',
              description: '是否启用该 Feature',
              optional: false,
            },
            {
              name: 'options',
              type: 'DiagramDotFeatureOptions',
              description: '可选配置项',
              optional: true,
            },
          ],
          returns: 'DiagramDotFeatureConfig',
        },
        {
          name: 'getDiagramDotFeatureOptions',
          description: '从 SupramarkConfig 中读取 Diagram (DOT / Graphviz) 的 options。',
          parameters: [
            {
              name: 'config',
              type: 'SupramarkConfig | undefined',
              description: '全局 supramark 配置',
              optional: true,
            },
          ],
          returns: 'DiagramDotFeatureOptions | undefined',
        },
      ],
      types: [],
    },

    bestPractices: ['在 AST 层保持 DOT 源码完整，并通过 diagram-engine 在各平台统一输出 SVG。'],

    faq: [
      {
        question: 'DOT / Graphviz 是如何渲染的？',
        answer:
          'Supramark 会把 ```dot / ```graphviz 围栏解析为 diagram 节点，再由 @supramark/engines 在 Web 侧通过 Wasm、在 RN 侧通过原生 Graphviz 模块输出 SVG。',
      },
    ],
  },
};

FeatureRegistry.register(diagramDotFeature);

export interface DiagramDotFeatureOptions {
  // 预留：未来可加入默认布局引擎、属性注入等选项
}

export type DiagramDotFeatureConfig = FeatureConfigWithOptions<DiagramDotFeatureOptions>;

const diagramDotHelpers = makeFeatureConfigHelpers<DiagramDotFeatureOptions>(
  '@supramark/feature-diagram-dot'
);
export const createDiagramDotFeatureConfig = diagramDotHelpers.create;
export const getDiagramDotFeatureOptions = diagramDotHelpers.getOptions;
