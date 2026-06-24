import type {
  SupramarkContainerNode,
  SupramarkNode,
  FeatureConfigWithOptions,
  SupramarkConfig,
  SupramarkFeature,
} from '@supramark/core';
import { FeatureRegistry, makeFeatureConfigHelpers } from '@supramark/core';
import { visonExamples } from './examples.js';

/**
 * Vison Feature
 *
 * Adds support for `:::vison` container blocks. The body of the
 * container is parsed as a Vison JSON spec
 * (see https://github.com/kookyleo/vison) and exposed on the AST node
 * as `data.spec`. Hosts pair this feature with the `vison`
 * containerRenderer entry from `runtime.web` / `runtime.rn` to render
 * the spec as a card.
 *
 * @example
 * ```markdown
 * :::vison
 * { "version": "1", "type": "container",
 *   "style": { "padding": 12, "backgroundColor": "#F5F5F5", "borderRadius": 8 },
 *   "children": [
 *     { "type": "text",
 *       "props": { "text": "Hello Vison" },
 *       "style": { "fontSize": 16, "fontWeight": "bold" } }
 *   ]
 * }
 * :::
 * ```
 */

/** Vison spec subset surfaced on the AST node. Mirrors `VisonComponent`
 *  from `@actrium/vison-web` so consumers don't have to import
 *  the renderer package just to read the spec. */
export interface VisonSpec {
  version?: string;
  type: string;
  props?: Record<string, unknown>;
  style?: Record<string, unknown>;
  children?: VisonSpec[];
}

export interface VisonContainerData extends Record<string, unknown> {
  /** Successfully-parsed Vison spec, or `undefined` when the body
   *  failed to parse (in which case `parseError` carries the reason). */
  spec?: VisonSpec;
  /** Raw body string (always available — useful for debugging or
   *  re-rendering the spec source in a fallback view). */
  source: string;
  /** Set when the body could not be parsed as JSON. */
  parseError?: string;
}

export type SupramarkVisonContainerNode = SupramarkContainerNode & {
  name: 'vison';
  data: VisonContainerData;
};

const isVisonContainer = (node: SupramarkNode): node is SupramarkVisonContainerNode => {
  return node.type === 'container' && (node as SupramarkContainerNode).name === 'vison';
};

export const visonFeature: SupramarkFeature<SupramarkVisonContainerNode> = {
  metadata: {
    id: '@supramark/feature-card-vison',
    name: 'Card (Vison)',
    version: '0.1.0',
    author: 'Supramark Team',
    description:
      'Render :::vison container blocks as Vison cards — a JSON visual description spec for AI chat UIs.',
    license: 'Apache-2.0',
    tags: ['card', 'vison', 'ai-chat', 'container'],
    syntaxFamily: 'container',
  },

  syntax: {
    ast: {
      type: 'container',
      selector: isVisonContainer,
      interface: {
        required: ['type', 'name', 'data', 'children'],
        optional: ['params'],
        fields: {
          type: {
            type: 'string',
            description: 'Always "container".',
          },
          name: {
            type: 'string',
            description: 'Always "vison".',
          },
          data: {
            type: 'object',
            description: '{ spec?: VisonSpec; source: string; parseError?: string }',
          },
          children: {
            type: 'array',
            description: 'Always empty (the body is consumed as JSON).',
          },
        },
      },
      examples: [
        {
          type: 'container',
          name: 'vison',
          data: {
            source: '{ "version": "1", "type": "text" }',
            spec: {
              version: '1',
              type: 'text',
            },
          },
          children: [],
        } as SupramarkVisonContainerNode,
      ],
    },
  },

  renderers: {
    web: {
      platform: 'web',
      infrastructure: {
        needsClientScript: false,
        needsWorker: false,
        needsCache: false,
      },
      dependencies: [],
    },
    rn: {
      platform: 'rn',
      infrastructure: {
        needsWorker: false,
        needsCache: false,
      },
      dependencies: [],
    },
  },

  examples: visonExamples,

  testing: {
    syntaxTests: {
      cases: [
        {
          name: 'Vison container with valid JSON body',
          input: ':::vison\n{ "version": "1", "type": "text", "props": { "text": "hi" } }\n:::',
          expected: {
            type: 'container',
            name: 'vison',
          } as SupramarkVisonContainerNode,
          options: { typeOnly: true },
        },
      ],
    },
    integrationTests: {
      cases: [
        {
          name: 'Vison container produces a vison-named container node',
          input: ':::vison\n{ "version": "1", "type": "text" }\n:::',
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          validate: (result: any) => result?.children?.[0]?.name === 'vison',
          platforms: ['web', 'rn'],
        },
      ],
    },
    coverageRequirements: {
      statements: 80,
      branches: 80,
      functions: 80,
      lines: 80,
    },
  },

  documentation: {
    readme: `
# Card (Vison) Feature

Renders \`:::vison\` container blocks as Vison cards. The body of the
container is parsed as Vison JSON (see
[vison spec](https://github.com/kookyleo/vison)) and exposed on the
AST node as \`data.spec\`.

## Wiring

\`\`\`tsx
import { visonFeature } from '@supramark/feature-card-vison';
import { renderVisonContainerWeb } from '@supramark/feature-card-vison/runtime.web';
import { Supramark } from '@supramark/web';

<Supramark
  markdown={md}
  config={{ features: [{ id: '@supramark/feature-card-vison', enabled: true }] }}
  containerRenderers={{ vison: renderVisonContainerWeb }}
/>
\`\`\`

The RN wiring is the same with \`renderVisonContainerRN\` from
\`@supramark/feature-card-vison/runtime.rn\`.
`.trim(),

    api: {
      interfaces: [
        {
          name: 'VisonFeatureOptions',
          description: 'Vison feature options (currently empty; reserved).',
          fields: [],
        },
      ],
      functions: [
        {
          name: 'createVisonFeatureConfig',
          description: 'Create a feature config entry for the Vison card feature.',
          parameters: [
            { name: 'enabled', type: 'boolean', description: 'Enable / disable.' },
            {
              name: 'options',
              type: 'VisonFeatureOptions',
              description: 'Reserved.',
              optional: true,
            },
          ],
          returns: 'VisonFeatureConfig',
        },
      ],
      types: [],
    },

    bestPractices: [
      'Keep the JSON body small — Vison is intentionally a thin visual layer.',
      'Validate / pre-render server-side when possible; the client-side renderer is intentionally strict and returns a fallback view on parse errors.',
    ],

    faq: [
      {
        question: 'Why a container instead of a fence?',
        answer:
          'Containers naturally pair with renderer-side wiring (`containerRenderers={{ vison }}`), reuse the existing `:::name` parser, and let supramark deliver the parsed spec straight to the host renderer without going through the diagram → SVG path.',
      },
    ],
  },
};

FeatureRegistry.register(visonFeature);

export interface VisonFeatureOptions {
  // Reserved for future use (theme overrides, max depth, etc.).
}

export type VisonFeatureConfig = FeatureConfigWithOptions<VisonFeatureOptions>;

const visonHelpers = makeFeatureConfigHelpers<VisonFeatureOptions>('@supramark/feature-card-vison');
export const createVisonFeatureConfig = visonHelpers.create;
export const getVisonFeatureOptions = visonHelpers.getOptions;
