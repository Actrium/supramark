import type { FeatureConfigWithOptions } from '@supramark/core';
import {
  FeatureRegistry,
  defineDiagramFeature,
  makeFeatureConfigHelpers,
} from '@supramark/core';
import { diagramVegaLiteExamples } from './examples.js';

/**
 * Vega-Lite diagram feature.
 *
 * - Reuses the generic `diagram` AST node.
 * - Matches diagrams whose engine is one of:
 *   `'vega-lite' | 'vega' | 'chart' | 'chartjs'`.
 * - Rendering goes through `@supramark/engines/vega-lite` against the
 *   upstream `vega` + `vega-lite` JS packages. Web and RN share the
 *   same headless `vega.View(...).toSVG()` output path.
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
export const diagramVegaLiteFeature = defineDiagramFeature({
  id: '@supramark/feature-diagram-vega-lite',
  engineId: 'vega-lite',
  engineAliases: ['vega', 'chart', 'chartjs'],
  name: 'Diagram (Vega-Lite)',
  description:
    'Vega / Vega-Lite diagrams rendered through @supramark/engines + the JS vega/vega-lite libraries.',
  tags: ['diagram', 'vega-lite', 'chart', 'svg'],
  web: {
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
  rn: {
    infrastructure: { needsCache: true },
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
      {
        name: 'react-native-svg',
        version: '^13.0.0',
        type: 'npm',
        optional: false,
      },
    ],
  },
  examples: diagramVegaLiteExamples,
  exampleCode: '{ "mark": "bar", "data": { "values": [] } }',
  apiPrefix: 'DiagramVegaLite',
  readme: `
# Diagram (Vega-Lite) Feature

Vega / Vega-Lite / ChartJS diagrams as fenced code blocks.

- Syntax: \`\\\`\\\`vega-lite\`, \`\\\`\\\`vega\`, \`\\\`\\\`chart\`, or
  \`\\\`\\\`chartjs\` fenced code blocks.
- AST: parsed into a \`diagram\` node with the matching \`engine\`
  identifier; \`code\` is the JSON spec.
- Rendering: on Web and RN, \`@supramark/engines/vega-lite\` consumes the
  upstream JS \`vega\` + \`vega-lite\` packages, runs
  \`vega.View(..., { renderer: "none" }).toSVG()\`, and hands the SVG
  string to the platform renderer.
  `.trim(),
  bestPractices: [
    'Keep the Vega-Lite spec valid JSON for round-trip debugging and reuse.',
    'Express renderer-side options (width, height, theme) via `meta` so the data spec stays portable.',
  ],
  faq: [
    {
      question: 'How does Vega-Lite render on RN?',
      answer:
        'Supramark runs Vega in headless SVG export mode and hands the SVG string to react-native-svg.',
    },
  ],
});

FeatureRegistry.register(diagramVegaLiteFeature);

export interface DiagramVegaLiteFeatureOptions {
  // Reserved for future options (default renderer, theme, etc.).
}

export type DiagramVegaLiteFeatureConfig =
  FeatureConfigWithOptions<DiagramVegaLiteFeatureOptions>;

const diagramVegaLiteHelpers = makeFeatureConfigHelpers<DiagramVegaLiteFeatureOptions>(
  '@supramark/feature-diagram-vega-lite'
);
export const createDiagramVegaLiteFeatureConfig = diagramVegaLiteHelpers.create;
export const getDiagramVegaLiteFeatureOptions = diagramVegaLiteHelpers.getOptions;
