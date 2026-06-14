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
 * - Rendering goes through `@supramark/engines/echarts`. ECharts itself is
 *   a JS chart library; Supramark uses its SVG SSR path so Web and RN share
 *   the same source -> SVG-string contract.
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
    'ECharts diagrams rendered to SVG through @supramark/engines + the JS echarts library.',
  tags: ['diagram', 'echarts', 'chart', 'svg'],
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
    infrastructure: { needsCache: true },
    dependencies: [
      {
        name: 'echarts',
        version: '^5.0.0',
        type: 'npm',
        optional: false,
      },
      {
        name: 'react-native-svg',
        version: '^13.0.0',
        type: 'npm',
        optional: false,
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

Apache ECharts diagrams as fenced code blocks.

- Syntax: \`\\\`\\\`echarts\` fenced code blocks containing an ECharts
  option JSON.
- AST: parsed into a \`diagram\` node with \`engine = "echarts"\`,
  \`code\` carrying the option string.
- Rendering: on Web and RN, \`@supramark/engines/echarts\` consumes the
  upstream JS \`echarts\` library through its SVG SSR mode and produces
  SVG markup for the platform renderer.
  `.trim(),
  bestPractices: [
    'Use the same ECharts option shape as your front-end project for shared debugging.',
    'Pass renderer / theme hints through `meta` so engine-side defaults can be overridden per node.',
  ],
  faq: [
    {
      question: 'How does ECharts render on RN?',
      answer:
        'Supramark uses ECharts SVG SSR mode and hands the resulting SVG string to react-native-svg.',
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
