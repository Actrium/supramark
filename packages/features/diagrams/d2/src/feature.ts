import type {
  SupramarkNode,
  SupramarkDiagramNode,
  FeatureConfigWithOptions,
  SupramarkConfig,
  SupramarkFeature,
} from '@supramark/core';
import { FeatureRegistry, makeFeatureConfigHelpers } from '@supramark/core';
import { d2Examples } from './examples.js';

/**
 * D2 diagram feature.
 *
 * - Reuses the generic `diagram` AST node.
 * - Matches diagrams with `engine === 'd2'`.
 * - On Web, `@supramark/engines` calls `@kookyleo/d2-little-web`
 *   (Rust → wasm). d2-little ships its own pure-Rust layout engine, so
 *   no Graphviz bridge is needed (unlike plantuml).
 *
 * @example
 * ```markdown
 * ```d2
 * a -> b
 * ```
 * ```
 */

const isD2Diagram = (node: SupramarkNode): node is SupramarkDiagramNode => {
  return (
    node.type === 'diagram' &&
    typeof (node as SupramarkDiagramNode).engine === 'string' &&
    (node as SupramarkDiagramNode).engine.toLowerCase() === 'd2'
  );
};

export const d2Feature: SupramarkFeature<SupramarkDiagramNode> = {
  metadata: {
    id: '@supramark/feature-d2',
    name: 'Diagram (D2)',
    version: '0.1.0',
    author: 'Supramark Team',
    description: 'D2 diagrams rendered to SVG through @supramark/engines + d2-little-web.',
    license: 'Apache-2.0',
    tags: ['diagram', 'd2'],
    syntaxFamily: 'fence',
  },

  syntax: {
    ast: {
      type: 'diagram',
      selector: isD2Diagram,
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
            description: 'Diagram engine identifier, fixed as "d2" for this feature.',
          },
          code: {
            type: 'string',
            description: 'Raw D2 source text (between ```d2 fences).',
          },
          meta: {
            type: 'object',
            description: 'Optional runtime metadata for D2 rendering (e.g. theme, sketch).',
          },
        },
      },
      examples: [
        {
          type: 'diagram',
          engine: 'd2',
          code: 'a -> b',
        } as SupramarkDiagramNode,
      ],
    },
  },

  renderers: {
    rn: {
      platform: 'rn',
      // RN path is unsupported in this build. Awaits a d2-little
      // native FFI binding. See crates/d2-little/UPSTREAM.md.
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
          name: '@kookyleo/d2-little-web',
          version: 'workspace:*',
          type: 'npm',
          optional: false,
        },
      ],
    },
  },

  examples: d2Examples,

  testing: {
    syntaxTests: {
      cases: [
        {
          name: 'Parse a ```d2 fence into a diagram node',
          input: ['```d2', 'a -> b', '```'].join('\n'),
          expected: {
            type: 'diagram',
            engine: 'd2',
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
          name: 'Web D2 render (smoke: output exists)',
          input: {
            type: 'diagram',
            engine: 'd2',
            code: 'a -> b',
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
          name: 'End-to-end: a markdown doc containing a ```d2 fence',
          input: ['# D2 demo', '', '```d2', 'a -> b', '```'].join('\n'),
          validate: (result: unknown) => {
            if (!result || typeof result !== 'object') return false;
            const root = result as any;
            const children = Array.isArray(root.children) ? root.children : [];
            return children.some(
              (n: any) => n.type === 'diagram' && String(n.engine).toLowerCase() === 'd2'
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
# Diagram (D2) Feature

AST modelling + Web rendering for D2 diagrams.

- Syntax: \`\\\`\\\`d2\` fenced code blocks.
- AST: parsed into a \`diagram\` node with \`engine = "d2"\`,
  \`code\` carrying the raw D2 source.
- Rendering: on Web, \`@supramark/engines\` calls
  \`@kookyleo/d2-little-web\` (Rust → wasm; ships its own dagre-style
  layout, no Graphviz bridge required). On RN, d2 is currently
  **unsupported** — replacement is a d2-little native FFI binding
  tracked in \`crates/d2-little/UPSTREAM.md\`.
    `.trim(),

    api: {
      interfaces: [
        {
          name: 'D2FeatureOptions',
          description: 'D2 feature options (currently empty; reserved).',
          fields: [],
        },
      ],
      functions: [
        {
          name: 'createD2FeatureConfig',
          description: 'Create a feature config entry for the D2 diagram feature.',
          parameters: [
            {
              name: 'enabled',
              type: 'boolean',
              description: 'Enable / disable the feature.',
              optional: false,
            },
            {
              name: 'options',
              type: 'D2FeatureOptions',
              description: 'Optional feature options.',
              optional: true,
            },
          ],
          returns: 'D2FeatureConfig',
        },
        {
          name: 'getD2FeatureOptions',
          description: 'Read this feature\'s options from the global SupramarkConfig.',
          parameters: [
            {
              name: 'config',
              type: 'SupramarkConfig | undefined',
              description: 'Global supramark config.',
              optional: true,
            },
          ],
          returns: 'D2FeatureOptions | undefined',
        },
      ],
      types: [],
    },

    bestPractices: [
      'Keep D2 source readable; for complex layouts, use D2 containers `{}` to break the source into modules.',
      'Enable diagram-level caching to skip repeated wasm calls for identical sources.',
    ],

    faq: [
      {
        question: 'How is D2 rendered?',
        answer:
          'On Web, @kookyleo/d2-little-web (Rust → wasm) converts the source to SVG. d2-little ships its own pure-Rust layout engine, so unlike PlantUML there is no need for a Graphviz bridge.',
      },
      {
        question: 'How does D2 differ from mermaid / plantuml?',
        answer:
          'D2 is a more modern declarative diagram DSL with first-class containers, styles, and modern layouts. It complements the others: mermaid leans toward flow / sequence diagrams, plantuml covers the full UML surface, D2 is well suited to software architecture and system diagrams.',
      },
    ],
  },
};

FeatureRegistry.register(d2Feature);

export interface D2FeatureOptions {
  // Reserved for future options (theme, sketch, etc.).
}

export type D2FeatureConfig = FeatureConfigWithOptions<D2FeatureOptions>;

const d2Helpers = makeFeatureConfigHelpers<D2FeatureOptions>('@supramark/feature-d2');
export const createD2FeatureConfig = d2Helpers.create;
export const getD2FeatureOptions = d2Helpers.getOptions;
