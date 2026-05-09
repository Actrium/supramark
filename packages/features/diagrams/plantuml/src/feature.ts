import type {
  SupramarkNode,
  SupramarkDiagramNode,
  FeatureConfigWithOptions,
  SupramarkConfig,
  SupramarkFeature,
} from '@supramark/core';
import { FeatureRegistry, makeFeatureConfigHelpers } from '@supramark/core';
import { plantumlExamples } from './examples.js';

/**
 * PlantUML diagram feature.
 *
 * - Reuses the generic `diagram` AST node.
 * - Matches diagrams with `engine === 'plantuml'`.
 * - On Web, `@supramark/engines` calls `@kookyleo/plantuml-little-web`
 *   (Rust → wasm) to turn `@startuml … @enduml` source into SVG.
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

const isPlantumlDiagram = (node: SupramarkNode): node is SupramarkDiagramNode => {
  return (
    node.type === 'diagram' &&
    typeof (node as SupramarkDiagramNode).engine === 'string' &&
    (node as SupramarkDiagramNode).engine.toLowerCase() === 'plantuml'
  );
};

export const plantumlFeature: SupramarkFeature<SupramarkDiagramNode> = {
  metadata: {
    id: '@supramark/feature-plantuml',
    name: 'Diagram (PlantUML)',
    version: '0.1.0',
    author: 'Supramark Team',
    description: 'PlantUML UML diagrams rendered to SVG through @supramark/engines + plantuml-little-web.',
    license: 'Apache-2.0',
    tags: ['diagram', 'plantuml', 'uml'],
    syntaxFamily: 'fence',
  },

  syntax: {
    ast: {
      type: 'diagram',
      selector: isPlantumlDiagram,
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
            description: 'Diagram engine identifier, fixed as "plantuml" for this feature.',
          },
          code: {
            type: 'string',
            description: 'Raw PlantUML source text (between ```plantuml fences, typically wrapped with @startuml / @enduml).',
          },
          meta: {
            type: 'object',
            description: 'Optional runtime metadata for PlantUML rendering (e.g. skin params).',
          },
        },
      },
      examples: [
        {
          type: 'diagram',
          engine: 'plantuml',
          code: '@startuml\nBob -> Alice : hello\n@enduml',
        } as SupramarkDiagramNode,
      ],
    },
  },

  renderers: {
    rn: {
      platform: 'rn',
      // RN path is unsupported in this build. Awaits a plantuml-little
      // native FFI binding (modelled on graphviz-anywhere-rn). See
      // crates/plantuml-little/UPSTREAM.md.
      infrastructure: {
        needsCache: false,
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
          name: '@kookyleo/plantuml-little-web',
          version: 'workspace:*',
          type: 'npm',
          optional: false,
        },
        {
          name: '@kookyleo/graphviz-anywhere-web',
          version: 'workspace:*',
          type: 'npm',
          optional: false,
        },
      ],
    },
  },

  examples: plantumlExamples,

  testing: {
    syntaxTests: {
      cases: [
        {
          name: 'Parse a ```plantuml fence into a diagram node',
          input: ['```plantuml', '@startuml', 'Bob -> Alice : hello', '@enduml', '```'].join('\n'),
          expected: {
            type: 'diagram',
            engine: 'plantuml',
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
          name: 'Web PlantUML render (smoke: output exists)',
          input: {
            type: 'diagram',
            engine: 'plantuml',
            code: '@startuml\nBob -> Alice : hello\n@enduml',
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
          name: 'End-to-end: a markdown doc containing a ```plantuml fence',
          input: [
            '# PlantUML demo',
            '',
            '```plantuml',
            '@startuml',
            'Bob -> Alice : hello',
            '@enduml',
            '```',
          ].join('\n'),
          validate: (result: unknown) => {
            if (!result || typeof result !== 'object') return false;
            const root = result as any;
            const children = Array.isArray(root.children) ? root.children : [];
            return children.some(
              (n: any) => n.type === 'diagram' && String(n.engine).toLowerCase() === 'plantuml'
            );
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
# Diagram (PlantUML) Feature

AST modelling + Web rendering for PlantUML diagrams.

- Syntax: \`\\\`\\\`plantuml\` fenced code blocks.
- AST: parsed into a \`diagram\` node with \`engine = "plantuml"\`,
  \`code\` carrying the raw PlantUML source.
- Rendering: on Web, \`@supramark/engines\` calls
  \`@kookyleo/plantuml-little-web\` (Rust → wasm). Graphviz layout for
  the diagram families that need it is served by
  \`@kookyleo/graphviz-anywhere-web\` through a host-installed
  \`globalThis.__graphviz_anywhere_render\` bridge. On RN, plantuml
  is currently **unsupported** — the legacy WebView worker was
  retired in 2026-05; replacement is a plantuml-little native FFI
  binding tracked in \`crates/plantuml-little/UPSTREAM.md\`.
    `.trim(),

    api: {
      interfaces: [
        {
          name: 'PlantumlFeatureOptions',
          description: 'PlantUML feature options (currently empty; reserved).',
          fields: [],
        },
      ],
      functions: [
        {
          name: 'createPlantumlFeatureConfig',
          description: 'Create a feature config entry for the PlantUML diagram feature.',
          parameters: [
            {
              name: 'enabled',
              type: 'boolean',
              description: 'Enable / disable the feature.',
              optional: false,
            },
            {
              name: 'options',
              type: 'PlantumlFeatureOptions',
              description: 'Optional feature options.',
              optional: true,
            },
          ],
          returns: 'PlantumlFeatureConfig',
        },
        {
          name: 'getPlantumlFeatureOptions',
          description: 'Read this feature\'s options from the global SupramarkConfig.',
          parameters: [
            {
              name: 'config',
              type: 'SupramarkConfig | undefined',
              description: 'Global supramark config.',
              optional: true,
            },
          ],
          returns: 'PlantumlFeatureOptions | undefined',
        },
      ],
      types: [],
    },

    bestPractices: [
      'Wrap source in @startuml / @enduml so the same fence renders consistently across hosts.',
      'For large diagrams, enable caching via the unified diagram config so identical sources skip the wasm call.',
    ],

    faq: [
      {
        question: 'How is PlantUML rendered?',
        answer:
          'On Web, @kookyleo/plantuml-little-web (Rust → wasm) converts the source to SVG. Graphviz-backed layout is bridged through @kookyleo/graphviz-anywhere-web via a globalThis.__graphviz_anywhere_render bridge installed by the engine loader.',
      },
      {
        question: 'Why do you need a Graphviz bridge?',
        answer:
          'PlantUML\'s component / use-case / state diagram families delegate layout to Graphviz. The default loader therefore preloads graphviz-anywhere-web, installs the bridge function on globalThis, and then loads plantuml-little-web — which calls back into Graphviz when layout is needed.',
      },
    ],
  },
};

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
