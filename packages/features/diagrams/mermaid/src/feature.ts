import type {
  SupramarkNode,
  SupramarkDiagramNode,
  FeatureConfigWithOptions,
  SupramarkConfig,
  SupramarkFeature,
} from '@supramark/core';
import { FeatureRegistry, makeFeatureConfigHelpers } from '@supramark/core';
import { mermaidExamples } from './examples.js';

const isMermaidDiagram = (node: SupramarkNode): node is SupramarkDiagramNode => {
  return (
    node.type === 'diagram' &&
    typeof (node as SupramarkDiagramNode).engine === 'string' &&
    (node as SupramarkDiagramNode).engine.toLowerCase() === 'mermaid'
  );
};

export const mermaidFeature: SupramarkFeature<SupramarkDiagramNode> = {
  metadata: {
    id: '@supramark/feature-mermaid',
    name: 'Diagram (Mermaid)',
    version: '0.1.0',
    author: 'Supramark Team',
    description: 'Support for Mermaid diagrams rendered via the unified diagram pipeline.',
    license: 'Apache-2.0',
    tags: ['diagram', 'mermaid'],
    syntaxFamily: 'fence',
  },

  syntax: {
    ast: {
      type: 'diagram',
      selector: isMermaidDiagram,
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
            description: 'Diagram engine identifier, fixed as "mermaid" for this feature.',
          },
          code: {
            type: 'string',
            description: 'Raw Mermaid source text (between ```mermaid fences).',
          },
          meta: {
            type: 'object',
            description: 'Optional runtime metadata for Mermaid rendering.',
          },
        },
      },
      examples: [
        {
          type: 'diagram',
          engine: 'mermaid',
          code: 'graph TD\n  A --> B',
        } as SupramarkDiagramNode,
      ],
    },
  },

  renderers: {
    rn: {
      platform: 'rn',
      // RN path is unsupported in this build. Tracked for a future
      // mermaid-little RN native FFI binding (mirrors
      // graphviz-anywhere-rn). Until then, RN renders fall back to an
      // "unsupported on RN" message — see
      // crates/mermaid-little/UPSTREAM.md.
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
          name: '@kookyleo/mermaid-little-web',
          version: 'workspace:*',
          type: 'npm',
          optional: false,
        },
      ],
    },
  },

  examples: mermaidExamples,

  testing: {
    syntaxTests: {
      cases: [
        {
          name: 'Parse a ```mermaid fence into a diagram node',
          input: ['```mermaid', 'graph TD', '  A --> B', '```'].join('\n'),
          expected: {
            type: 'diagram',
            engine: 'mermaid',
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
          name: 'Web Mermaid render (smoke: output exists)',
          input: {
            type: 'diagram',
            engine: 'mermaid',
            code: 'graph TD\n  A --> B',
          } as SupramarkDiagramNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: false,
        },
      ],
      // RN render path is intentionally absent in this build — mermaid
      // on RN awaits the mermaid-little native FFI binding; until then
      // DiagramNode returns "unsupported on RN".
    },
    integrationTests: {
      cases: [
        {
          name: 'End-to-end: a markdown doc containing a ```mermaid fence',
          input: ['# Mermaid demo', '', '```mermaid', 'graph TD', '  A --> B', '```'].join('\n'),
          validate: (result: unknown) => {
            if (!result || typeof result !== 'object') return false;
            const root = result as any;
            const children = Array.isArray(root.children) ? root.children : [];
            return children.some(
              (n: any) => n.type === 'diagram' && String(n.engine).toLowerCase() === 'mermaid'
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

    api: {
      interfaces: [
        {
          name: 'MermaidFeatureOptions',
          description: 'Mermaid feature options (currently empty; reserved).',
          fields: [],
        },
      ],
      functions: [
        {
          name: 'createMermaidFeatureConfig',
          description: 'Create a feature config entry for the Mermaid diagram feature.',
          parameters: [
            {
              name: 'enabled',
              type: 'boolean',
              description: 'Enable / disable the feature.',
              optional: false,
            },
            {
              name: 'options',
              type: 'MermaidFeatureOptions',
              description: 'Optional feature options.',
              optional: true,
            },
          ],
          returns: 'MermaidFeatureConfig',
        },
        {
          name: 'getMermaidFeatureOptions',
          description: 'Read this feature\'s options from the global SupramarkConfig.',
          parameters: [
            {
              name: 'config',
              type: 'SupramarkConfig | undefined',
              description: 'Global supramark config.',
              optional: true,
            },
          ],
          returns: 'MermaidFeatureOptions | undefined',
        },
      ],
      types: [],
    },

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
  },
};

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
