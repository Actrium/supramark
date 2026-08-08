import { describe, expect, test } from 'bun:test';
import React from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { SupramarkConfig, SupramarkRootNode } from '@supramark/core';

import './support/mock-react-native';
import './support/mock-renderer';

// React's test renderer requires this flag before effects can be flushed through act().
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT =
  true;

const { Supramark } = await import('../src/Supramark');

// Build the same two-entry definition list shape produced by the native Markdown parser.
const definitionListAst: SupramarkRootNode = {
  type: 'root',
  children: [
    {
      type: 'definition_list',
      children: [
        {
          type: 'definition_item',
          children: [
            {
              type: 'definition_term',
              children: [{ type: 'text', value: 'HTTP' }],
            },
            {
              type: 'definition_description',
              children: [
                {
                  type: 'paragraph',
                  children: [{ type: 'text', value: 'Hypertext protocol' }],
                },
              ],
            },
          ],
        },
        {
          type: 'definition_item',
          children: [
            {
              type: 'definition_term',
              children: [{ type: 'text', value: 'HTTPS' }],
            },
            {
              type: 'definition_description',
              children: [
                {
                  type: 'paragraph',
                  children: [{ type: 'text', value: 'Encrypted hypertext protocol' }],
                },
              ],
            },
          ],
        },
      ],
    },
  ],
} as SupramarkRootNode;

// Flatten React Native style arrays so layout assertions inspect the effective Text style.
const flattenStyle = (style: unknown): Record<string, unknown> =>
  (Array.isArray(style) ? style : [style]).reduce<Record<string, unknown>>(
    (result, item) =>
      item && typeof item === 'object'
        ? { ...result, ...(item as Record<string, unknown>) }
        : result,
    {}
  );

// Read string leaves from a host Text node without depending on native text measurement.
const flattenText = (node: React.ReactNode): string => {
  if (node == null || typeof node === 'boolean') return '';
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(flattenText).join('');
  if (typeof node === 'object' && 'props' in node) {
    return flattenText((node as React.ReactElement).props.children);
  }
  return '';
};

// Render a completed AST and flush Supramark's asynchronous document effect.
const renderDefinitionList = async (config?: SupramarkConfig): Promise<ReactTestRenderer> => {
  let renderer: ReactTestRenderer | null = null;
  await act(async () => {
    renderer = create(React.createElement(Supramark, { ast: definitionListAst, config }));
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
  return renderer as unknown as ReactTestRenderer;
};

// Both feature branches must keep terms out of the ordinary list text's flex layout.
const featureStates: Array<[string, SupramarkConfig | undefined]> = [
  ['enabled', undefined],
  [
    'disabled',
    {
      features: [{ id: '@supramark/feature-definition-list', enabled: false }],
    },
  ],
];

describe('definition list React Native layout', () => {
  test.each(featureStates)(
    'keeps term height intrinsic when the feature is %s',
    async (_state, config) => {
      const renderer = await renderDefinitionList(config);
      const termNodes = renderer.root
        .findAllByType('Text')
        .filter(node => ['HTTP', 'HTTPS'].includes(flattenText(node.props.children)));

      expect(termNodes).toHaveLength(2);
      for (const termNode of termNodes) {
        const termStyle = flattenStyle(termNode.props.style);
        expect(termStyle).not.toHaveProperty('flex');
        expect(termStyle).not.toHaveProperty('flexGrow');
        expect(termStyle).not.toHaveProperty('flexShrink');
        expect(termStyle.fontWeight).toBe('600');
      }
    }
  );

  test('keeps the non-compact spacer out of list flex layout', async () => {
    const renderer = await renderDefinitionList({
      features: [
        {
          id: '@supramark/feature-definition-list',
          enabled: true,
          options: { compact: false },
        },
      ],
    });
    const spacerNode = renderer.root
      .findAllByType('Text')
      .find(node => flattenText(node.props.children) === '\n');

    expect(spacerNode).toBeDefined();
    const spacerStyle = flattenStyle(spacerNode?.props.style);
    expect(spacerStyle).not.toHaveProperty('flex');
    expect(spacerStyle).not.toHaveProperty('flexGrow');
    expect(spacerStyle).not.toHaveProperty('flexShrink');
  });
});
