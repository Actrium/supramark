import type { FeatureConfigWithOptions } from '@supramark/core';
import {
  FeatureRegistry,
  defineDiagramFeature,
  makeFeatureConfigHelpers,
} from '@supramark/core';
import { plantumlExamples } from './examples.js';

/**
 * PlantUML diagram feature.
 *
 * - Reuses the generic `diagram` AST node.
 * - Matches diagrams with `engine === 'plantuml'`.
 * - On Web, `@supramark/engines` calls `@actrium/plantuml-little-web`
 *   (Rust → wasm) to turn `@startuml ... @enduml` source into SVG.
 * - On RN, hosts import `@actrium/supramark-plantuml-native-rn`,
 *   which registers the plantuml-little native FFI adapter.
 *
 * @example
 * ```markdown
 * ```plantuml
 * @startuml
 * Bob -> Alice : hello
 * @enduml
 * ```
 * ```
 */
export const plantumlFeature = defineDiagramFeature({
  id: '@supramark/feature-plantuml',
  engineId: 'plantuml',
  name: 'Diagram (PlantUML)',
  description:
    'PlantUML UML diagrams rendered to SVG through @supramark/engines + plantuml-little.',
  tags: ['diagram', 'plantuml', 'uml'],
  web: {
    dependencies: [
      {
        name: '@actrium/plantuml-little-web',
        version: 'workspace:*',
        type: 'npm',
        optional: false,
      },
      {
        name: '@actrium/graphviz-anywhere-web',
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
  examples: plantumlExamples,
  exampleCode: '@startuml\nBob -> Alice : hello\n@enduml',
  apiPrefix: 'Plantuml',
  engineFieldDescription: 'Diagram engine identifier, fixed as "plantuml" for this feature.',
  codeFieldDescription:
    'Raw PlantUML source text (between ```plantuml fences, typically wrapped with @startuml / @enduml).',
  metaFieldDescription: 'Optional runtime metadata for PlantUML rendering (e.g. skin params).',
  readme: `
# Diagram (PlantUML) Feature

AST modelling + Web / RN rendering for PlantUML diagrams.

- Syntax: \`\\\`\\\`plantuml\` fenced code blocks.
- AST: parsed into a \`diagram\` node with \`engine = "plantuml"\`,
  \`code\` carrying the raw PlantUML source.
- Rendering: on Web, \`@supramark/engines\` calls
  \`@actrium/plantuml-little-web\` (Rust → wasm). Graphviz layout for
  the diagram families that need it is served by
  \`@actrium/graphviz-anywhere-web\` through a host-installed
  \`globalThis.__graphviz_anywhere_render\` bridge. On RN, hosts import
  \`@actrium/supramark-plantuml-native-rn\`, which registers the
  plantuml-little native FFI adapter and returns the same SVG contract.
  `.trim(),
  bestPractices: [
    'Wrap source in @startuml / @enduml so the same fence renders consistently across hosts.',
    'For large diagrams, enable caching via the unified diagram config so identical sources skip the wasm call.',
  ],
  faq: [
    {
      question: 'How is PlantUML rendered?',
      answer:
        'On Web, @actrium/plantuml-little-web (Rust → wasm) converts the source to SVG. Graphviz-backed layout is bridged through @actrium/graphviz-anywhere-web via a globalThis.__graphviz_anywhere_render bridge installed by the engine loader. On RN, @actrium/supramark-plantuml-native-rn registers the native FFI adapter with @supramark/engines/rn.',
    },
    {
      question: 'Why do you need a Graphviz bridge?',
      answer:
        "PlantUML's component / use-case / state diagram families delegate layout to Graphviz. The default loader therefore preloads graphviz-anywhere-web, installs the bridge function on globalThis, and then loads plantuml-little-web — which calls back into Graphviz when layout is needed.",
    },
  ],
});

FeatureRegistry.register(plantumlFeature);

export interface PlantumlFeatureOptions {
  // Reserved for future options (skin params, default theme, etc.).
}

export type PlantumlFeatureConfig = FeatureConfigWithOptions<PlantumlFeatureOptions>;

const plantumlHelpers = makeFeatureConfigHelpers<PlantumlFeatureOptions>(
  '@supramark/feature-plantuml'
);
export const createPlantumlFeatureConfig = plantumlHelpers.create;
export const getPlantumlFeatureOptions = plantumlHelpers.getOptions;
