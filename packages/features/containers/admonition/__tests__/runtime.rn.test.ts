import React from 'react';
import { describe, expect, mock, test } from 'bun:test';

// Replace native host components with inspectable element names for this renderer unit test.
mock.module('react-native', () => ({
  StyleSheet: {
    create: <T>(styles: T) => styles,
  },
  View: 'View',
  Text: 'Text',
}));

const { renderAdmonitionContainerRN } = await import('../src/runtime.rn');

// Flatten the renderer's style arrays without depending on React Native StyleSheet.
const flattenStyle = (style: unknown): Record<string, unknown> =>
  (Array.isArray(style) ? style : [style]).reduce<Record<string, unknown>>(
    (result, item) =>
      item && typeof item === 'object'
        ? { ...result, ...(item as Record<string, unknown>) }
        : result,
    {}
  );

// Build one renderer result with deliberately incompatible list styles to expose accidental reuse.
const renderFixture = (config: unknown) => {
  const renderedChild = React.createElement('RenderedChild', {
    key: 'child',
  });
  const listItemStyle = { flexDirection: 'row', flex: 1 };
  const listItemTextStyle = { flex: 1 };
  const result = renderAdmonitionContainerRN({
    node: {
      type: 'container',
      data: { title: 'Warning' },
      children: [{ type: 'paragraph', children: [] }],
    },
    key: 'admonition',
    styles: {
      listItem: listItemStyle,
      listItemText: listItemTextStyle,
    },
    config,
    renderChildren: () => [renderedChild],
  } as never) as React.ReactElement;

  return { result, renderedChild, listItemStyle, listItemTextStyle };
};

// Exercise both feature branches because disabled rendering must preserve the same layout contract.
const featureStates = [
  ['enabled', undefined],
  [
    'disabled',
    {
      features: [{ id: '@supramark/feature-admonition', enabled: false }],
    },
  ],
] as const;

describe('Admonition React Native renderer', () => {
  test.each(featureStates)(
    'uses natural column sizing when the feature is %s',
    (_state, config) => {
      const { result } = renderFixture(config);
      const containerStyle = flattenStyle(result.props.style);
      expect(result.type).toBe('View');
      expect(containerStyle).toMatchObject({
        flexDirection: 'column',
        alignSelf: 'flex-start',
        maxWidth: '100%',
        minWidth: 0,
      });
      expect(containerStyle).not.toHaveProperty('flex');
      expect(containerStyle).not.toHaveProperty('flexGrow');
      expect(containerStyle).not.toHaveProperty('flexShrink');
    }
  );

  test.each(featureStates)(
    'does not reuse list flex styles when the feature is %s',
    (_state, config) => {
      const { result, listItemStyle, listItemTextStyle } = renderFixture(config);
      const containerStyle = flattenStyle(result.props.style);
      const children = React.Children.toArray(result.props.children) as React.ReactElement[];
      const title = children[0];

      expect(containerStyle).not.toMatchObject(listItemStyle);
      expect(title.type).toBe('Text');
      expect(flattenStyle(title.props.style)).not.toMatchObject(listItemTextStyle);
      expect(flattenStyle(title.props.style)).not.toHaveProperty('flex');
      expect(flattenStyle(title.props.style)).not.toHaveProperty('flexGrow');
      expect(flattenStyle(title.props.style)).not.toHaveProperty('flexShrink');
    }
  );

  test.each(featureStates)(
    'renders block children directly inside a View when the feature is %s',
    (_state, config) => {
      const { result } = renderFixture(config);
      const children = React.Children.toArray(result.props.children) as React.ReactElement[];
      const content = children[1];
      const contentStyle = flattenStyle(content.props.style);

      expect(content.type).toBe('View');
      expect(contentStyle).not.toHaveProperty('flex');
      expect(contentStyle).not.toHaveProperty('flexGrow');
      expect(contentStyle).not.toHaveProperty('flexShrink');
      expect(React.Children.count(content.props.children)).toBe(1);
      expect(React.Children.toArray(content.props.children)[0]).toMatchObject({
        type: 'RenderedChild',
      });
    }
  );
});
