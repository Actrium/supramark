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
        name: '@actrium/mermaid-little-web',
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

AST modelling + Web / RN rendering for Mermaid diagrams.

- Syntax: \`\\\`\\\`mermaid\` fenced code blocks.
- AST: parsed into a \`diagram\` node with \`engine = "mermaid"\`.
- Rendering: on Web, \`@supramark/engines\` calls
  \`@actrium/mermaid-little-web\` (Rust → wasm; no DOM, no headless
  browser, no upstream JS Mermaid bundle) and inlines the SVG. On RN,
  hosts import \`@actrium/supramark-mermaid-native-rn\`, which registers
  the mermaid-little native FFI adapter and returns the same SVG contract.
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
      question: 'How does Mermaid render on React Native?',
      answer:
        'Hosts import @actrium/supramark-mermaid-native-rn at startup. The side-effect import registers a native FFI adapter with @supramark/engines/rn, and the renderer receives SVG through the same diagram contract as Web.',
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
