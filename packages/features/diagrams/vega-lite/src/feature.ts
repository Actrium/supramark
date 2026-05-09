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
export const diagramVegaLiteFeature = defineDiagramFeature({
  id: '@supramark/feature-diagram-vega-lite',
  engineId: 'vega-lite',
  engineAliases: ['vega', 'chart', 'chartjs'],
  name: 'Diagram (Vega-Lite)',
  description:
    'Vega / Vega-Lite diagrams rendered through @supramark/engines + the JS vega/vega-lite libraries (Web only).',
  tags: ['diagram', 'vega-lite', 'chart', 'web-only'],
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
    // Web-only feature today. RN path is unsupported in this build:
    // the WebView worker has been retired; the planned RN path is a
    // pure-JS pipeline (vega.View(spec, {renderer: 'none'}).toSVG())
    // that produces an SVG string for react-native-svg to display.
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
  examples: diagramVegaLiteExamples,
  exampleCode: '{ "mark": "bar", "data": { "values": [] } }',
  apiPrefix: 'DiagramVegaLite',
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
