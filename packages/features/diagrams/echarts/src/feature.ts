import type {
  SupramarkNode,
  SupramarkDiagramNode,
  FeatureConfigWithOptions,
  SupramarkConfig,
  SupramarkFeature,
} from '@supramark/core';
import { FeatureRegistry, makeFeatureConfigHelpers } from '@supramark/core';
import { diagramEchartsExamples } from './examples.js';

/**
 * ECharts diagram feature.
 *
 * - Reuses the generic `diagram` AST node.
 * - Matches diagrams with `engine === 'echarts'`.
 * - Web rendering goes through `@supramark/engines/echarts`. ECharts
 *   itself is a JS chart library (canvas / SVG renderer), so unlike
 *   the *-little engines there is no Rust port today; the RN path is
 *   unsupported in this build.
 *
 * @example
 * ```markdown
 * ```echarts
 * { "title": { "text": "ECharts" }, "series": [{ "type": "bar", "data": [1,2,3] }] }
 * ```
 * ```
 */

const isEchartsDiagram = (node: SupramarkNode): node is SupramarkDiagramNode => {
  return (
    node.type === 'diagram' &&
    typeof (node as SupramarkDiagramNode).engine === 'string' &&
    (node as SupramarkDiagramNode).engine.toLowerCase() === 'echarts'
  );
};

export const diagramEchartsFeature: SupramarkFeature<SupramarkDiagramNode> = {
  metadata: {
    id: '@supramark/feature-diagram-echarts',
    name: 'Diagram (ECharts)',
    version: '0.1.0',
    author: 'Supramark Team',
    description: 'ECharts diagrams rendered to SVG through @supramark/engines + the JS echarts library (Web only).',
    license: 'Apache-2.0',
    tags: ['diagram', 'echarts', 'chart', 'web-only'],
    syntaxFamily: 'fence',
  },

  syntax: {
    ast: {
      type: 'diagram',
      selector: isEchartsDiagram,
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
            description: 'Diagram engine identifier, expected to be "echarts".',
          },
          code: {
            type: 'string',
            description: 'Raw ECharts option JSON from the fenced code block.',
          },
          meta: {
            type: 'object',
            description: 'Optional metadata for ECharts rendering (theme / size / renderer).',
          },
        },
      },
      examples: [
        {
          type: 'diagram',
          engine: 'echarts',
          code: '{ "title": { "text": "ECharts" }, "series": [{ "type": "bar", "data": [1,2,3] }] }',
        } as SupramarkDiagramNode,
      ],
    },
  },

  renderers: {
    rn: {
      platform: 'rn',
      // Web-only feature today. RN path is unsupported in this build:
      // ECharts has no Rust port and the WebView worker has been
      // retired. Replacement is a future @wuba/react-native-echarts
      // integration. Until then, RN renders return "unsupported on RN".
      infrastructure: {
        needsCache: true,
      },
      dependencies: [
        {
          name: 'react-native-svg',
          version: '^13.0.0',
          type: 'npm',
          optional: true,
        },
      ],
    },
    web: {
      platform: 'web',
      infrastructure: {
        needsCache: false,
      },
      dependencies: [
        {
          name: 'echarts',
          version: '^5.0.0',
          type: 'npm',
          optional: false,
        },
      ],
    },
  },

  examples: diagramEchartsExamples,

  testing: {
    syntaxTests: {
      cases: [
        {
          name: 'Parse a ```echarts fence into a diagram node',
          input: ['```echarts', '{ "series": [] }', '```'].join('\n'),
          expected: {
            type: 'diagram',
            engine: 'echarts',
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
          name: 'Web ECharts render (smoke: output exists)',
          input: {
            type: 'diagram',
            engine: 'echarts',
            code: '{ "series": [] }',
          } as SupramarkDiagramNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: false,
        },
      ],
      // RN render path intentionally absent — see infrastructure note.
    },
    integrationTests: {
      cases: [
        {
          name: 'End-to-end: a markdown doc containing a ```echarts fence',
          input: ['# ECharts demo', '', '```echarts', '{ "series": [] }', '```'].join('\n'),
          validate: (result: unknown) => {
            if (!result || typeof result !== 'object') return false;
            const root = result as any;
            const children = Array.isArray(root.children) ? root.children : [];
            return children.some(
              (n: any) => n.type === 'diagram' && String(n.engine).toLowerCase() === 'echarts'
            );
          },
          platforms: ['web'],
        },
      ],
    },
    coverageRequirements: {
      statements: 50,
      branches: 40,
      functions: 40,
      lines: 50,
    },
  },

  documentation: {
    readme: `
# Diagram (ECharts) Feature

Apache ECharts diagrams as fenced code blocks (Web only in this build).

- Syntax: \`\\\`\\\`echarts\` fenced code blocks containing an ECharts
  option JSON.
- AST: parsed into a \`diagram\` node with \`engine = "echarts"\`,
  \`code\` carrying the option string.
- Rendering: on Web, \`@supramark/engines/echarts\` consumes the
  upstream JS \`echarts\` library and produces SVG. On RN, ECharts is
  currently **unsupported** — the WebView worker was retired in
  2026-05; a planned native path uses
  \`@wuba/react-native-echarts\` (mature open-source RN wrapper over
  Skia / SVG).
    `.trim(),

    api: {
      interfaces: [
        {
          name: 'DiagramEchartsFeatureOptions',
          description: 'ECharts feature options (currently empty; reserved).',
          fields: [],
        },
      ],
      functions: [
        {
          name: 'createDiagramEchartsFeatureConfig',
          description: 'Create a feature config entry for the ECharts diagram feature.',
          parameters: [
            {
              name: 'enabled',
              type: 'boolean',
              description: 'Enable / disable the feature.',
              optional: false,
            },
            {
              name: 'options',
              type: 'DiagramEchartsFeatureOptions',
              description: 'Optional feature options.',
              optional: true,
            },
          ],
          returns: 'DiagramEchartsFeatureConfig',
        },
        {
          name: 'getDiagramEchartsFeatureOptions',
          description: 'Read this feature\'s options from the global SupramarkConfig.',
          parameters: [
            {
              name: 'config',
              type: 'SupramarkConfig | undefined',
              description: 'Global supramark config.',
              optional: true,
            },
          ],
          returns: 'DiagramEchartsFeatureOptions | undefined',
        },
      ],
      types: [],
    },

    bestPractices: [
      'Use the same ECharts option shape as your front-end project for shared debugging.',
      'Pass renderer / theme hints through `meta` so engine-side defaults can be overridden per node.',
    ],

    faq: [
      {
        question: 'Why is ECharts Web-only in supramark?',
        answer:
          'ECharts has no Rust port; the engine that produces SVG runs in a JS host. The hidden-WebView worker that previously bridged this on RN was retired in 2026-05. The replacement plan is to wire @wuba/react-native-echarts (a mature RN wrapper over Skia / SVG) in a follow-up.',
      },
    ],
  },
};

FeatureRegistry.register(diagramEchartsFeature);

export interface DiagramEchartsFeatureOptions {
  // Reserved for future options (default renderer, theme, etc.).
}

export type DiagramEchartsFeatureConfig = FeatureConfigWithOptions<DiagramEchartsFeatureOptions>;

const diagramEchartsHelpers = makeFeatureConfigHelpers<DiagramEchartsFeatureOptions>(
  '@supramark/feature-diagram-echarts'
);
export const createDiagramEchartsFeatureConfig = diagramEchartsHelpers.create;
export const getDiagramEchartsFeatureOptions = diagramEchartsHelpers.getOptions;
