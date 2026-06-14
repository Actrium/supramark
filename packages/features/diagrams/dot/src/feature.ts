import type { FeatureConfigWithOptions } from '@supramark/core';
import {
  FeatureRegistry,
  defineDiagramFeature,
  makeFeatureConfigHelpers,
} from '@supramark/core';
import { diagramDotExamples } from './examples.js';

/**
 * DOT / Graphviz diagram feature.
 *
 * - Reuses the generic `diagram` AST node.
 * - Matches diagrams whose engine is `'dot'` or `'graphviz'`.
 * - Rendered through `@supramark/engines` (Web wasm via graphviz-anywhere,
 *   RN via the native graphviz-anywhere binding).
 *
 * @example
 * ```markdown
 * ```dot
 * digraph G { A -> B }
 * ```
 * ```
 */
export const diagramDotFeature = defineDiagramFeature({
  id: '@supramark/feature-diagram-dot',
  engineId: 'dot',
  engineAliases: ['graphviz'],
  name: 'Diagram (DOT / Graphviz)',
  description: 'DOT / Graphviz diagrams rendered to SVG through @supramark/engines.',
  tags: ['diagram', 'dot', 'graphviz'],
  web: {
    infrastructure: { needsClientScript: false },
  },
  rn: {
    infrastructure: { needsWorker: false, needsCache: false },
  },
  examples: diagramDotExamples,
  exampleCode: 'digraph G { A -> B }',
  apiPrefix: 'DiagramDot',
  engineFieldDescription: 'Diagram engine identifier, "dot" or "graphviz".',
  codeFieldDescription: 'Raw DOT source text from the fenced code block.',
  metaFieldDescription:
    'Optional metadata reserved for future Graphviz integration (layout engine, options, etc.).',
  rnRenderTest: true,
  integrationTestPlatforms: ['web', 'rn'],
  coverageRequirements: { statements: 40, branches: 30, functions: 30, lines: 40 },
  readme: `
# Diagram (DOT / Graphviz) Feature

AST modelling + RN / Web rendering for DOT / Graphviz diagrams via the
@supramark/engines pipeline.

- Syntax: \`\\\`\\\`dot\` or \`\\\`\\\`graphviz\` fenced code blocks.
- AST: parsed into a \`diagram\` node with \`engine = "dot"\` or
  \`"graphviz"\`, \`code\` carrying the raw DOT source.
- Rendering: \`@supramark/engines\` produces SVG on both Web (wasm via
  graphviz-anywhere-web) and RN (native graphviz-anywhere-rn).
  `.trim(),
  bestPractices: [
    'Keep DOT source intact in the AST and let @supramark/engines emit SVG uniformly across platforms.',
  ],
  faq: [
    {
      question: 'How is DOT / Graphviz rendered?',
      answer:
        'Supramark parses ```dot / ```graphviz fences into diagram nodes; @supramark/engines then produces SVG via wasm on Web and via the native Graphviz module on RN.',
    },
  ],
});

FeatureRegistry.register(diagramDotFeature);

export interface DiagramDotFeatureOptions {
  // Reserved: defaults for layout engine / attribute injection, etc.
}

export type DiagramDotFeatureConfig = FeatureConfigWithOptions<DiagramDotFeatureOptions>;

const diagramDotHelpers = makeFeatureConfigHelpers<DiagramDotFeatureOptions>(
  '@supramark/feature-diagram-dot'
);
export const createDiagramDotFeatureConfig = diagramDotHelpers.create;
export const getDiagramDotFeatureOptions = diagramDotHelpers.getOptions;
