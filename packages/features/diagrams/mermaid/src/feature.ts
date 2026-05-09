import type { FeatureConfigWithOptions } from '@supramark/core';
import {
  FeatureRegistry,
  defineDiagramFeature,
  makeFeatureConfigHelpers,
} from '@supramark/core';
import { mermaidExamples } from './examples.js';

export const mermaidFeature = defineDiagramFeature({
  id: '@supramark/feature-mermaid',
  engineId: 'mermaid',
  name: 'Diagram (Mermaid)',
  description: 'Support for Mermaid diagrams rendered via the unified diagram pipeline.',
  tags: ['diagram', 'mermaid'],
  web: {
    dependencies: [
      {
        name: '@kookyleo/mermaid-little-web',
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
  examples: mermaidExamples,
  exampleCode: 'graph TD\n  A --> B',
  apiPrefix: 'Mermaid',
  engineFieldDescription: 'Diagram engine identifier, fixed as "mermaid" for this feature.',
  codeFieldDescription: 'Raw Mermaid source text (between ```mermaid fences).',
  metaFieldDescription: 'Optional runtime metadata for Mermaid rendering.',
  readme: `
# Mermaid Feature

AST modelling + Web rendering for Mermaid diagrams.

- Syntax: \`\\\`\\\`mermaid\` fenced code blocks.
- AST: parsed into a \`diagram\` node with \`engine = "mermaid"\`.
- Rendering: on Web, \`@supramark/engines\` calls
  \`@kookyleo/mermaid-little-web\` (Rust → wasm; no DOM, no headless
  browser, no upstream JS Mermaid bundle) and inlines the SVG. On RN,
  mermaid is currently **unsupported** — the legacy WebView worker was
  retired in 2026-05; replacement is a mermaid-little native FFI
  binding tracked in \`crates/mermaid-little/UPSTREAM.md\`.
  `.trim(),
  bestPractices: [
    'Keep Mermaid source small and reusable so the same fence can be shared across Web hosts.',
    'Prefer the unified diagram config (theme / layout) over inlining options inside the Markdown source.',
  ],
  faq: [
    {
      question: 'Why a dedicated feature package for Mermaid?',
      answer:
        'Parser, renderer wiring, and feature gating all need to be aligned per engine. A standalone feature lets Mermaid participate in the same capability-discovery, config, and documentation flow as every other diagram.',
    },
    {
      question: 'Does React Native still need a headless WebView?',
      answer:
        'No — and Mermaid is also not yet usable on RN. The hidden-WebView worker (@supramark/rn-diagram-worker) was retired in the 2026-05 cleanup. Mermaid on RN will return when the mermaid-little native FFI binding lands; tracked in crates/mermaid-little/UPSTREAM.md.',
    },
  ],
});

FeatureRegistry.register(mermaidFeature);

export interface MermaidFeatureOptions {
  // reserved for future options
}

export type MermaidFeatureConfig = FeatureConfigWithOptions<MermaidFeatureOptions>;

const mermaidHelpers = makeFeatureConfigHelpers<MermaidFeatureOptions>(
  '@supramark/feature-mermaid'
);
export const createMermaidFeatureConfig = mermaidHelpers.create;
export const getMermaidFeatureOptions = mermaidHelpers.getOptions;
