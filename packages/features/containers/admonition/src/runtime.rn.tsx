/**
 * Admonition React Native renderer
 *
 * Implements the ContainerRNRenderer interface
 *
 * @packageDocumentation
 */

import React from 'react';
import { StyleSheet, View, Text } from 'react-native';
import type { StyleProp, TextStyle } from 'react-native';
import type { ContainerRNRenderArgs, FeatureConfig } from '@supramark/core';

// React Native's bundled @types/react differs from the workspace React types
// (e.g. on bigint in ReactNode); derive the node type from Text's own children.
type RNNode = React.ComponentProps<typeof Text>['children'];
type RNViewNode = React.ComponentProps<typeof View>['children'];

/**
 * RN renderer for :::note, :::tip, :::warning etc.
 */
export function renderAdmonitionContainerRN({
  node,
  key,
  styles,
  config,
  renderChildren,
}: ContainerRNRenderArgs): React.ReactNode {
  const title = node?.data?.title as RNNode;
  const textStyle = styles.paragraph as StyleProp<TextStyle>;

  // Feature enable check: fall back to plain styling when disabled
  const isEnabled =
    !config || !config.features || config.features.length === 0
      ? true
      : (config.features.find((f: FeatureConfig) => f.id === '@supramark/feature-admonition')
          ?.enabled ?? true);

  return (
    <View key={key} style={localStyles.container}>
      {title ? (
        <Text style={[textStyle, localStyles.title, isEnabled && localStyles.titleEnabled]}>
          {title}
        </Text>
      ) : null}
      <View style={localStyles.content}>{renderChildren(node.children ?? []) as RNViewNode}</View>
    </View>
  );
}

const localStyles = StyleSheet.create({
  // Admonitions are block containers: their width follows content up to the host's available width.
  container: {
    flexDirection: 'column',
    alignSelf: 'flex-start',
    maxWidth: '100%',
    minWidth: 0,
  },
  // The title keeps host typography without inheriting list-item flex behavior.
  title: {
    maxWidth: '100%',
  },
  titleEnabled: {
    fontWeight: '600',
  },
  // Render block children inside a View so paragraphs, lists, and diagrams remain valid RN children.
  content: {
    alignSelf: 'flex-start',
    maxWidth: '100%',
    minWidth: 0,
  },
});
