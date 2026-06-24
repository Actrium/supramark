import type { FeatureConfigWithOptions } from '@supramark/core';
import {
  FeatureRegistry,
  defineDiagramFeature,
  makeFeatureConfigHelpers,
} from '@supramark/core';
import { d2Examples } from './examples.js';

/**
 * D2 diagram feature.
 *
 * - Reuses the generic `diagram` AST node.
 * - Matches diagrams with `engine === 'd2'`.
 * - On Web, `@supramark/engines` calls `@actrium/d2-little-web`
 *   (Rust → wasm). On RN, hosts import the d2-little native package,
 *   which registers the same SVG contract through `@supramark/engines/rn`.
 *
 * @example
 * ```markdown
 * ```d2
 * a -> b
 * ```
 * ```
 */
export const d2Feature = defineDiagramFeature({
  id: '@supramark/feature-d2',
  engineId: 'd2',
  name: 'Diagram (D2)',
  description: 'D2 diagrams rendered to SVG through @supramark/engines.',
  tags: ['diagram', 'd2'],
  web: {
    dependencies: [
      {
        name: '@actrium/d2-little-web',
        version: 'workspace:*',
        type: 'npm',
        optional: false,
      },
    ],
  },
  rn: {
    dependencies: [
      {
        name: 'react-native-svg',
        version: '^13.0.0',
        type: 'npm',
        optional: true,
      },
    ],
  },
  examples: d2Examples,
  exampleCode: 'a -> b',
  apiPrefix: 'D2',
  engineFieldDescription: 'Diagram engine identifier, fixed as "d2" for this feature.',
  codeFieldDescription: 'Raw D2 source text (between ```d2 fences).',
  metaFieldDescription: 'Optional runtime metadata for D2 rendering (e.g. theme, sketch).',
  readme: `
# Diagram (D2) Feature

AST modelling + SVG rendering for D2 diagrams.

- Syntax: \`\\\`\\\`d2\` fenced code blocks.
- AST: parsed into a \`diagram\` node with \`engine = "d2"\`,
  \`code\` carrying the raw D2 source.
- Rendering: \`@supramark/engines\` returns SVG. Web uses
  \`@actrium/d2-little-web\` (Rust → wasm; ships its own dagre-style
  layout, no Graphviz bridge required). RN hosts import
  \`@actrium/supramark-d2-native-rn\` at startup, which registers the
  d2-little native FFI adapter with \`@supramark/engines/rn\`.
  `.trim(),
  bestPractices: [
    'Keep D2 source readable; for complex layouts, use D2 containers `{}` to break the source into modules.',
    'Enable diagram-level caching to skip repeated wasm calls for identical sources.',
  ],
  faq: [
    {
      question: 'How is D2 rendered?',
      answer:
        'D2 is rendered through @supramark/engines and always returns SVG. Web uses @actrium/d2-little-web; RN hosts import @actrium/supramark-d2-native-rn to register the native FFI adapter.',
    },
    {
      question: 'How does D2 differ from mermaid / plantuml?',
      answer:
        'D2 is a more modern declarative diagram DSL with first-class containers, styles, and modern layouts. It complements the others: mermaid leans toward flow / sequence diagrams, plantuml covers the full UML surface, D2 is well suited to software architecture and system diagrams.',
    },
  ],
});

FeatureRegistry.register(d2Feature);

export interface D2FeatureOptions {
  // Reserved for future options (theme, sketch, etc.).
}

export type D2FeatureConfig = FeatureConfigWithOptions<D2FeatureOptions>;

const d2Helpers = makeFeatureConfigHelpers<D2FeatureOptions>('@supramark/feature-d2');
export const createD2FeatureConfig = d2Helpers.create;
export const getD2FeatureOptions = d2Helpers.getOptions;
