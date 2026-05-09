import type { FeatureConfigWithOptions } from '@supramark/core';
import {
  FeatureRegistry,
  defineDiagramFeature,
  makeFeatureConfigHelpers,
} from '@supramark/core';
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
export const diagramEchartsFeature = defineDiagramFeature({
  id: '@supramark/feature-diagram-echarts',
  engineId: 'echarts',
  name: 'Diagram (ECharts)',
  description:
    'ECharts diagrams rendered to SVG through @supramark/engines + the JS echarts library (Web only).',
  tags: ['diagram', 'echarts', 'chart', 'web-only'],
  web: {
    dependencies: [
      {
        name: 'echarts',
        version: '^5.0.0',
        type: 'npm',
        optional: false,
      },
    ],
  },
  rn: {
    // Web-only feature today. RN path is unsupported in this build:
    // ECharts has no Rust port and the WebView worker has been
    // retired. Replacement is a future @wuba/react-native-echarts
    // integration. Until then, RN renders return "unsupported on RN".
    infrastructure: { needsCache: true },
    dependencies: [
      {
        name: 'react-native-svg',
        version: '^13.0.0',
        type: 'npm',
        optional: true,
      },
    ],
  },
  examples: diagramEchartsExamples,
  exampleCode: '{ "series": [] }',
  apiPrefix: 'DiagramEcharts',
  engineFieldDescription: 'Diagram engine identifier, expected to be "echarts".',
  codeFieldDescription: 'Raw ECharts option JSON from the fenced code block.',
  metaFieldDescription: 'Optional metadata for ECharts rendering (theme / size / renderer).',
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
});

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
