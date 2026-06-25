import type { FeatureConfigWithOptions, SupramarkFeature, SupramarkNode } from '@supramark/core';
import { FeatureRegistry, makeFeatureConfigHelpers } from '@supramark/core';
import { codeHighlightExamples } from './examples.js';

export interface CodeHighlightFeatureOptions {
  /**
   * Reserved for runtime display defaults. Compile-time language/theme assets
   * are controlled by enabled highlight preset/language/theme features.
   */
  // `string & NonNullable<unknown>` keeps the 'light'/'dark' literals as
  // editor autocomplete hints without collapsing the union to a bare `string`.
  theme?: 'light' | 'dark' | (string & NonNullable<unknown>);
}

export type CodeHighlightFeatureConfig = FeatureConfigWithOptions<CodeHighlightFeatureOptions>;

export const codeHighlightFeature: SupramarkFeature<SupramarkNode> = {
  metadata: {
    id: '@supramark/feature-code-highlight',
    name: 'Code Highlight',
    version: '0.1.0',
    author: 'Supramark Team',
    description: 'Optional syntax highlighting framework for Markdown code blocks.',
    license: 'Apache-2.0',
    tags: ['code', 'highlight', 'syntax'],
    syntaxFamily: 'fence',
  },
  syntax: {
    ast: {
      type: 'code-highlight',
      selector: (node: SupramarkNode) => node.type === 'code' || node.type === 'inline_code',
      constraints: {
        allowedParents: ['root', 'paragraph'],
        allowedChildren: [],
      },
      examples: [
        {
          type: 'code',
          lang: 'ts',
          value: 'const message: string = "hello";',
        } as SupramarkNode,
      ],
    },
  },
  renderers: {
    web: {
      platform: 'web',
      infrastructure: {
        needsClientScript: false,
        needsWorker: false,
        needsCache: true,
      },
      dependencies: [],
    },
    rn: {
      platform: 'rn',
      infrastructure: {
        needsWorker: false,
        needsCache: true,
      },
      dependencies: [],
    },
  },
  compile: {
    codeHighlight: {
      runtime: true,
    },
  },
  examples: codeHighlightExamples,
  testing: {
    syntaxTests: {
      cases: [
        {
          name: 'keep code fence language',
          input: ['```ts', 'const value = 1;', '```'].join('\n'),
          expected: {
            type: 'code',
            lang: 'ts',
            value: 'const value = 1;',
          } as SupramarkNode,
          options: {
            ignoreFields: ['meta', 'position', 'data'],
          },
        },
      ],
    },
  },
  documentation: {
    readme: '# Code Highlight\n\nOptional syntax highlighting for standard Markdown code blocks.',
    api: {
      interfaces: [
        {
          name: 'CodeHighlightFeatureOptions',
          description: 'Runtime defaults for code highlighting display.',
          fields: [],
        },
      ],
    },
  },
};

FeatureRegistry.register(codeHighlightFeature);

const helpers = makeFeatureConfigHelpers<CodeHighlightFeatureOptions>(
  '@supramark/feature-code-highlight'
);
export const createCodeHighlightFeatureConfig = helpers.create;
export const getCodeHighlightFeatureOptions = helpers.getOptions;
