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

// Match the opaque node shape returned by the native parser for ::: warning syntax.
const warningAst: SupramarkRootNode = {
  type: 'root',
  children: [
    {
      type: 'container',
      name: 'warning',
      mode: 'opaque',
      params: 'Warning',
      children: [
        {
          type: 'paragraph',
          children: [{ type: 'text', value: 'Do not use test keys directly in production.' }],
        },
      ],
    },
  ],
} as SupramarkRootNode;

// Flatten React Native style arrays so assertions inspect the effective Text layout style.
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

// Render the native-parser AST directly and flush Supramark's asynchronous document effect.
const renderWarning = async (config?: SupramarkConfig): Promise<ReactTestRenderer> => {
  let renderer: ReactTestRenderer | null = null;
  await act(async () => {
    renderer = create(React.createElement(Supramark, { ast: warningAst, config }));
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
  return renderer as unknown as ReactTestRenderer;
};

// Cover every return path in the built-in opaque admonition renderer.
const featureStates: Array<[string, SupramarkConfig | undefined]> = [
  ['enabled', undefined],
  [
    'disabled',
    {
      features: [{ id: '@supramark/feature-admonition', enabled: false }],
    },
  ],
  [
    'kind excluded',
    {
      features: [
        {
          id: '@supramark/feature-admonition',
          enabled: true,
          options: { kinds: ['note'] },
        },
      ],
    },
  ],
];

describe('opaque admonition React Native layout', () => {
  test.each(featureStates)(
    'keeps the warning title height intrinsic when the feature is %s',
    async (_state, config) => {
      const renderer = await renderWarning(config);
      const textNodes = renderer.root.findAllByType('Text');
      const titleNode = textNodes.find(node => flattenText(node.props.children) === 'Warning');
      const bodyNode = textNodes.find(
        node => flattenText(node.props.children) === 'Do not use test keys directly in production.'
      );

      expect(titleNode).toBeDefined();
      expect(bodyNode).toBeDefined();
      const titleStyle = flattenStyle(titleNode?.props.style);
      expect(titleStyle).not.toHaveProperty('flex');
      expect(titleStyle).not.toHaveProperty('flexGrow');
      expect(titleStyle).not.toHaveProperty('flexShrink');
    }
  );
});
