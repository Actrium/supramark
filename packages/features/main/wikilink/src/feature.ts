import type {
  SupramarkFeature,
  SupramarkWikiLinkNode,
  FeatureConfigWithOptions,
} from '@supramark/core';
import { wikilinkExamples } from './examples.js';
import { makeFeatureConfigHelpers } from '@supramark/core';

/**
 * WikiLink Feature
 *
 * Knowledge-base Markdown (Obsidian, Logseq) syntax support:
 *
 * - `[[target]]` — link to a note
 * - `[[target|label]]` — link with a display label
 * - `[[target#section]]` — link with a heading fragment
 * - `[[#section]]` — same-page fragment link (empty target)
 *
 * Parsing is gated by the Rust parser's `wikilink` option (off by default —
 * `[[...]]` is not CommonMark/GFM syntax). Enabling this feature in
 * `SupramarkConfig.features` turns the parser flag on automatically; hosts
 * can also pass `{ wikilink: true }` to `parse()` explicitly.
 *
 * Resolution to a file path or URL is a host concern: provide a
 * `resolveWikiLink` callback in the feature options to map
 * `{ target, section }` to an href. Without a resolver (or when it returns
 * null/undefined) the wikilink renders as styled but non-navigable text.
 */
export const wikilinkFeature: SupramarkFeature<SupramarkWikiLinkNode> = {
  metadata: {
    id: '@supramark/feature-wikilink',
    name: 'WikiLink',
    version: '0.1.0',
    author: 'Supramark Team',
    description: 'WikiLink syntax support ([[target]], [[target|label]], [[target#section]])',
    license: 'Apache-2.0',
    tags: ['wikilink', 'link', 'knowledge-base'],
    syntaxFamily: 'main',
  },
  // WikiLink runs inside base Markdown inline parsing.
  dependencies: ['@supramark/feature-core-markdown'],

  syntax: {
    ast: {
      type: 'wiki_link',

      selector: (node) => node.type === 'wiki_link',

      interface: {
        required: ['type', 'target'],
        optional: ['section', 'label'],
        fields: {
          type: {
            type: 'string',
            description: 'Node type identifier, always "wiki_link".',
          },
          target: {
            type: 'string',
            description:
              'Raw link target; empty string for same-page fragment links ([[#section]]).',
          },
          section: {
            type: 'string',
            description: 'Heading fragment after the first # in the target part.',
          },
          label: {
            type: 'string',
            description: 'Display text after the first |.',
          },
        },
      },

      constraints: {
        allowedParents: [
          'root',
          'paragraph',
          'heading',
          'list_item',
          'table_cell',
          'blockquote',
          'strong',
          'emphasis',
          'delete',
          'link',
        ],
        allowedChildren: [],
      },

      examples: [
        {
          type: 'wiki_link',
          target: 'Project Plan',
        } as SupramarkWikiLinkNode,
        {
          type: 'wiki_link',
          target: 'Project Plan',
          section: 'Roadmap',
          label: 'the plan',
        } as SupramarkWikiLinkNode,
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

  examples: wikilinkExamples,

  testing: {
    syntaxTests: {
      cases: [
        {
          name: 'parses a plain wikilink',
          input: '[[Project Plan]]',
          expected: {
            type: 'wiki_link',
            target: 'Project Plan',
          } as SupramarkWikiLinkNode,
          options: {
            typeOnly: false,
            ignoreFields: ['position', 'data'],
          },
        },
        {
          name: 'parses a wikilink with label and section',
          input: '[[Project Plan#Roadmap|the plan]]',
          expected: {
            type: 'wiki_link',
            target: 'Project Plan',
            section: 'Roadmap',
            label: 'the plan',
          } as SupramarkWikiLinkNode,
          options: {
            typeOnly: false,
            ignoreFields: ['position', 'data'],
          },
        },
        {
          name: 'parses a same-page fragment wikilink',
          input: '[[#Roadmap]]',
          expected: {
            type: 'wiki_link',
            target: '',
            section: 'Roadmap',
          } as SupramarkWikiLinkNode,
          options: {
            typeOnly: false,
            ignoreFields: ['position', 'data'],
          },
        },
        // Malformed forms ([[]], unclosed [[foo, etc.) degrade to literal
        // text — a different node type, so they are not asserted here.
        // Degradation is covered by the Rust parser tests and core's
        // parser.test.ts (wikilink_degrades_malformed_forms /
        // "degrades malformed forms to literal text with the option on").
      ],
    },

    renderTests: {
      web: [
        {
          name: 'Web renders a wikilink',
          input: {
            type: 'wiki_link',
            target: 'Project Plan',
          } as SupramarkWikiLinkNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
      ],
      rn: [
        {
          name: 'RN renders a wikilink',
          input: {
            type: 'wiki_link',
            target: 'Project Plan',
          } as SupramarkWikiLinkNode,
          expected: (output: unknown) => output !== null && output !== undefined,
          snapshot: true,
        },
      ],
    },

    integrationTests: {
      cases: [
        {
          name: 'WikiLink end-to-end with the parser option on',
          input: 'See [[Project Plan]] and [[Roadmap#Q1|the roadmap]].',
          validate: result => {
            if (!result || typeof result !== 'object') return false;
            const nodes = (result as { children?: unknown[] }).children || [];
            const paragraph = nodes.find(n => (n as { type?: string }).type === 'paragraph');
            const children = ((paragraph as { children?: unknown[] }) || {}).children || [];
            return (
              children.filter(c => (c as { type?: string }).type === 'wiki_link').length === 2
            );
          },
          platforms: ['web', 'rn'],
        },
      ],
    },

    coverageRequirements: {
      statements: 80,
      branches: 75,
      functions: 80,
      lines: 80,
    },
  },

  documentation: {
    readme: `
# WikiLink Feature

Knowledge-base Markdown syntax support for Supramark.

## Syntax

- \`[[target]]\` — link to a note
- \`[[target|label]]\` — link with a display label
- \`[[target#section]]\` — link with a heading fragment
- \`[[#section]]\` — same-page fragment link (empty target)

## Usage

WikiLink parsing is a parser option (off by default). Enable it by adding this
feature to the config (which also enables the parser flag) and pass a resolver:

\`\`\`tsx
import { wikilinkFeature, createWikilinkFeatureConfig } from '@supramark/feature-wikilink';

<Supramark
  config={{
    features: [wikilinkFeature],
    featureConfigs: [
      createWikilinkFeatureConfig(true, {
        resolveWikiLink: ({ target, section }) =>
          \`/notes/\${encodeURIComponent(target)}\${section ? \`#\${section}\` : ''}\`,
      }),
    ],
  }}
/>
\`\`\`
    `.trim(),

    api: {
      interfaces: [
        {
          name: 'WikiLinkFeatureOptions',
          description: 'Configuration options for the WikiLink Feature',
          fields: [
            {
              name: 'resolveWikiLink',
              type: '(node: { target: string; section?: string; label?: string }) => string | null | undefined',
              description:
                'Maps a wiki_link node to an href. Return null/undefined to render the link as non-navigable text.',
              required: false,
            },
          ],
        },
        {
          name: 'SupramarkWikiLinkNode',
          description: 'AST node interface for a WikiLink ([[target]], [[target|label]], [[target#section]])',
          fields: [
            {
              name: 'type',
              type: "'wiki_link'",
              description: 'Node type identifier, always "wiki_link"',
              required: true,
            },
            {
              name: 'target',
              type: 'string',
              description: 'Raw link target; empty string for same-page fragment links',
              required: true,
            },
            {
              name: 'section',
              type: 'string',
              description: 'Heading fragment after the first # in the target part',
              required: false,
            },
            {
              name: 'label',
              type: 'string',
              description: 'Display text after the first |',
              required: false,
            },
          ],
        },
      ],

      functions: [
        {
          name: 'createWikilinkFeatureConfig',
          description: 'Creates a WikiLink Feature config object; enabling the feature also turns on the parser flag',
          parameters: [
            {
              name: 'enabled',
              type: 'boolean',
              description: 'Whether to enable the WikiLink Feature',
              optional: false,
            },
            {
              name: 'options',
              type: 'WikiLinkFeatureOptions',
              description: 'WikiLink Feature configuration options (e.g. resolveWikiLink)',
              optional: true,
            },
          ],
          returns: 'FeatureConfigWithOptions<WikiLinkFeatureOptions>',
          examples: [
            `import { createWikilinkFeatureConfig } from '@supramark/feature-wikilink';

const featureConfig = createWikilinkFeatureConfig(true, {
  resolveWikiLink: ({ target }) => \`/notes/\${encodeURIComponent(target)}\`,
});`,
          ],
        },
        {
          name: 'getWikilinkFeatureOptions',
          description: 'Extracts the WikiLink Feature configuration options from a SupramarkConfig',
          parameters: [
            {
              name: 'config',
              type: 'SupramarkConfig',
              description: 'The Supramark configuration object',
              optional: true,
            },
          ],
          returns: 'WikiLinkFeatureOptions | undefined',
          examples: [
            `import { getWikilinkFeatureOptions } from '@supramark/feature-wikilink';

const options = getWikilinkFeatureOptions(config);`,
          ],
        },
      ],

      types: [
        {
          name: 'WikiLinkFeatureConfig',
          description:
            'WikiLink Feature configuration type, a type alias for FeatureConfigWithOptions<WikiLinkFeatureOptions>',
          definition:
            'type WikiLinkFeatureConfig = FeatureConfigWithOptions<WikiLinkFeatureOptions>',
        },
      ],
    },

    bestPractices: [
      'Enable the feature in SupramarkConfig.features so the parser flag turns on automatically (parse() with { wikilink: true } also works)',
      'Always provide resolveWikiLink — raw targets are workspace-relative names, not URLs',
      'Escape with \\[[ when literal [[...]] text is needed',
      'WikiLink-looking text inside code spans/fences stays literal automatically',
    ],

    faq: [
      {
        question: 'Why is [[...]] plain text by default?',
        answer:
          'WikiLink is not CommonMark/GFM syntax. The parser flag is off by default so parsing stays byte-identical to the CommonMark/GFM profiles; enable the feature (or pass { wikilink: true } to parse()) to turn it on.',
      },
      {
        question: 'What happens to malformed forms like [[foo or [[a[b]]?',
        answer:
          'They degrade to literal text (CommonMark bracket semantics). No diagnostic is emitted because ordinary prose may legitimately contain [[.',
      },
      {
        question: 'Does [[target]] still resolve against a [target]: url reference definition?',
        answer:
          'No — when the option is on, wiki semantics win and the node is a wiki_link. With the option off, CommonMark shortcut-reference semantics apply unchanged.',
      },
      {
        question: 'How do I get clickable links?',
        answer:
          'Provide a resolveWikiLink callback mapping { target, section } to an href. Without a resolver the wikilink renders as styled but non-navigable text.',
      },
    ],
  },
};

/**
 * Configuration options for the WikiLink Feature.
 */
export interface WikiLinkFeatureOptions {
  /**
   * Resolve a wiki_link node to an href.
   *
   * Return `null`/`undefined` to render the wikilink as non-navigable text.
   */
  resolveWikiLink?: (node: {
    target: string;
    section?: string;
    label?: string;
  }) => string | null | undefined;
}

export type WikiLinkFeatureConfig = FeatureConfigWithOptions<WikiLinkFeatureOptions>;

const wikilinkHelpers = makeFeatureConfigHelpers<WikiLinkFeatureOptions>(
  '@supramark/feature-wikilink'
);
export const createWikilinkFeatureConfig = wikilinkHelpers.create;
export const getWikilinkFeatureOptions = wikilinkHelpers.getOptions;
