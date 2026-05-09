import type {
  SupramarkNode,
  SupramarkDiagramNode,
  FeatureConfigWithOptions,
  SupramarkConfig,
  SupramarkFeature,
} from '@supramark/core';
import { FeatureRegistry, makeFeatureConfigHelpers } from '@supramark/core';
import { diagramVegaLiteExamples } from './examples.js';

/**
 * Vega-Lite diagram feature.
 *
 * - Reuses the generic `diagram` AST node.
 * - Matches diagrams whose engine is one of:
 *   `'vega-lite' | 'vega' | 'chart' | 'chartjs'`.
 * - Web rendering goes through `@supramark/engines/vega-lite` against
 *   the upstream `vega` + `vega-lite` JS packages. RN is unsupported
 *   today; the planned RN path uses
 *   `vega.View(spec, { renderer: 'none' }).toSVG()` (pure JS, no DOM)
 *   piped into react-native-svg.
 *
 * @example
 * ```markdown
 * ```vega-lite
 * {
 *   "mark": "bar",
 *   "encoding": { "x": { "field": "category" }, "y": { "field": "value" } },
 *   "data": { "values": [{ "category": "A", "value": 1 }] }
 * }
 * ```
 * ```
 */

const isVegaLiteDiagram = (node: SupramarkNode): node is SupramarkDiagramNode => {
  return (
    node.type === 'diagram' &&
    typeof (node as SupramarkDiagramNode).engine === 'string' &&
    ['vega-lite', 'vega', 'chart', 'chartjs'].includes(
      (node as SupramarkDiagramNode).engine.toLowerCase()
    )
  );
};

export const diagramVegaLiteFeature: SupramarkFeature<SupramarkDiagramNode> = {
  metadata: {
    id: '@supramark/feature-diagram-vega-lite',
    name: 'Diagram (Vega-Lite)',
    version: '0.1.0',
    author: 'Supramark Team',
    description:
      'Vega / Vega-Lite diagrams rendered through @supramark/engines + the JS vega/vega-lite libraries (Web only).',
    license: 'Apache-2.0',
    tags: ['diagram', 'vega-lite', 'chart', 'web-only'],
    syntaxFamily: 'fence',
  },

  syntax: {
    ast: {
      type: 'diagram',
      selector: isVegaLiteDiagram,
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
            description:
              'Diagram engine identifier. For this feature, expected to be "vega-lite" / "vega" / "chart" / "chartjs".',
          },
          code: {
            type: 'string',
            description: 'Raw Vega-Lite JSON spec encoded in the fenced code block.',
          },
          meta: {
            type: 'object',
            description:
              'Optional metadata (renderer / theme / width / height) passed through to runtime.',
          },
        },
      },
      examples: [
        {
          type: 'diagram',
          engine: 'vega-lite',
          code: '{ "mark": "bar", "encoding": { "x": { "field": "category" }, "y": { "field": "value" } }, "data": { "values": [{ "category": "A", "value": 1 }] } }',
        } as SupramarkDiagramNode,
      ],
    },
  },

  renderers: {
    rn: {
      platform: 'rn',
      // Web-only feature today. RN path is unsupported in this build:
      // the WebView worker has been retired; the planned RN path is a
      // pure-JS pipeline (vega.View(spec, {renderer: 'none'}).toSVG())
      // that produces an SVG string for react-native-svg to display.
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
          name: 'vega',
          version: '^5.0.0',
          type: 'npm',
          optional: false,
        },
        {
          name: 'vega-lite',
          version: '^5.0.0',
          type: 'npm',
          optional: false,
        },
      ],
    },
  },

  examples: diagramVegaLiteExamples,

  testing: {
    syntaxTests: {
      cases: [
        {
          name: 'Parse a ```vega-lite fence into a diagram node',
          input: ['```vega-lite', '{ "mark": "bar", "data": { "values": [] } }', '```'].join('\n'),
          expected: {
            type: 'diagram',
            engine: 'vega-lite',
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
          name: 'Web Vega-Lite render (smoke: output exists)',
          input: {
            type: 'diagram',
            engine: 'vega-lite',
            code: '{ "mark": "bar" }',
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
          name: 'End-to-end: a markdown doc containing a ```vega-lite fence',
          input: ['# Diagram test', '', '```vega-lite', '{ "mark": "bar" }', '```'].join('\n'),
          validate: (result: unknown) => {
            if (!result || typeof result !== 'object') return false;
            const root = result as any;
            const children = Array.isArray(root.children) ? root.children : [];
            return children.some((n: any) => n.type === 'diagram' && typeof n.engine === 'string');
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
# Diagram (Vega-Lite) Feature

Vega / Vega-Lite / ChartJS diagrams as fenced code blocks (Web only in
this build).

- Syntax: \`\\\`\\\`vega-lite\`, \`\\\`\\\`vega\`, \`\\\`\\\`chart\`, or
  \`\\\`\\\`chartjs\` fenced code blocks.
- AST: parsed into a \`diagram\` node with the matching \`engine\`
  identifier; \`code\` is the JSON spec.
- Rendering: on Web, \`@supramark/engines/vega-lite\` consumes the
  upstream JS \`vega\` + \`vega-lite\` packages and produces SVG. On RN,
  this feature is currently **unsupported** — the WebView worker was
  retired in 2026-05; the planned native path runs
  \`vega.View(spec, { renderer: 'none' }).toSVG()\` in pure JS and
  hands the SVG string to react-native-svg.
    `.trim(),

    api: {
      interfaces: [
        {
          name: 'DiagramVegaLiteFeatureOptions',
          description: 'Vega-Lite feature options (currently empty; reserved).',
          fields: [],
        },
      ],
      functions: [
        {
          name: 'createDiagramVegaLiteFeatureConfig',
          description: 'Create a feature config entry for the Vega-Lite diagram feature.',
          parameters: [
            {
              name: 'enabled',
              type: 'boolean',
              description: 'Enable / disable the feature.',
              optional: false,
            },
            {
              name: 'options',
              type: 'DiagramVegaLiteFeatureOptions',
              description: 'Optional feature options.',
              optional: true,
            },
          ],
          returns: 'DiagramVegaLiteFeatureConfig',
        },
        {
          name: 'getDiagramVegaLiteFeatureOptions',
          description: 'Read this feature\'s options from the global SupramarkConfig.',
          parameters: [
            {
              name: 'config',
              type: 'SupramarkConfig | undefined',
              description: 'Global supramark config.',
              optional: true,
            },
          ],
          returns: 'DiagramVegaLiteFeatureOptions | undefined',
        },
      ],
      types: [],
    },

    bestPractices: [
      'Keep the Vega-Lite spec valid JSON for round-trip debugging and reuse.',
      'Express renderer-side options (width, height, theme) via `meta` so the data spec stays portable.',
    ],

    faq: [
      {
        question: 'Why is Vega-Lite Web-only in supramark?',
        answer:
          'Vega and Vega-Lite are JS libraries; the SVG-producing engine runs in a JS host. The hidden-WebView worker that used to bridge this on RN was retired in 2026-05. The replacement plan is a pure-JS path (vega.View(spec, { renderer: "none" }).toSVG()) wired to react-native-svg in a follow-up.',
      },
    ],
  },
};

FeatureRegistry.register(diagramVegaLiteFeature);

export interface DiagramVegaLiteFeatureOptions {
  // Reserved for future options (default renderer, theme, etc.).
}

export type DiagramVegaLiteFeatureConfig = FeatureConfigWithOptions<DiagramVegaLiteFeatureOptions>;

const diagramVegaLiteHelpers = makeFeatureConfigHelpers<DiagramVegaLiteFeatureOptions>(
  '@supramark/feature-diagram-vega-lite'
);
export const createDiagramVegaLiteFeatureConfig = diagramVegaLiteHelpers.create;
export const getDiagramVegaLiteFeatureOptions = diagramVegaLiteHelpers.getOptions;
