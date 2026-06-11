/**
 * Admonition React Native 渲染器
 *
 * 实现 ContainerRNRenderer 接口
 *
 * @packageDocumentation
 */

import React from 'react';
import { View, Text, StyleSheet, Image, ImageSourcePropType } from 'react-native';
import type { ContainerRNRenderArgs } from '@supramark/core';

import noteIcon from './icons/note.png';
import warningIcon from './icons/warning.png';

type AdmonitionKind = 'note' | 'tip' | 'info' | 'warning' | 'danger';

const kindTheme: Record<
  AdmonitionKind,
  { border: string; bg: string; color: string; icon: ImageSourcePropType; fallbackTitle: string }
> = {
  note: {
    border: '#448aff',
    bg: '#E9F0FC',
    color: '#597EF7',
    icon: noteIcon,
    fallbackTitle: '提示',
  },
  tip: {
    border: '#00c853',
    bg: '#e8f5e9',
    color: '#2e7d32',
    icon: warningIcon,
    fallbackTitle: '建议',
  },
  info: {
    border: '#448aff',
    bg: '#e3f2fd',
    color: '#1565c0',
    icon: noteIcon,
    fallbackTitle: '信息',
  },
  warning: {
    border: '#ff9100',
    bg: '#F2BF4433',
    color: '#F2BF44',
    icon: warningIcon,
    fallbackTitle: '警告',
  },
  danger: {
    border: '#ff1744',
    bg: '#fce4ec',
    color: '#b71c1c',
    icon: warningIcon,
    fallbackTitle: '危险',
  },
};

/** 深色主题下每种 admonition 类型对应的卡片色板。 */
const darkKindTheme: Record<
  AdmonitionKind,
  { border: string; bg: string; color: string; icon: ImageSourcePropType; fallbackTitle: string }
> = {
  note: {
    border: '#58a6ff',
    bg: '#10223a',
    color: '#79c0ff',
    icon: noteIcon,
    fallbackTitle: '提示',
  },
  tip: {
    border: '#3fb950',
    bg: '#0f2a1a',
    color: '#7ee787',
    icon: warningIcon,
    fallbackTitle: '建议',
  },
  info: {
    border: '#58a6ff',
    bg: '#10223a',
    color: '#79c0ff',
    icon: noteIcon,
    fallbackTitle: '信息',
  },
  warning: {
    border: '#d29922',
    bg: '#2d230d',
    color: '#f2cc60',
    icon: warningIcon,
    fallbackTitle: '警告',
  },
  danger: {
    border: '#f85149',
    bg: '#2d1517',
    color: '#ff7b72',
    icon: warningIcon,
    fallbackTitle: '危险',
  },
};

function resolveKind(node: any): AdmonitionKind {
  const raw = String(node?.data?.kind ?? '').toLowerCase();
  if (raw === 'note' || raw === 'tip' || raw === 'info' || raw === 'warning' || raw === 'danger') {
    return raw;
  }
  return 'note';
}

function normalizeNode(node: React.ReactNode, keyPrefix: string): React.ReactNode {
  if (typeof node === 'string' || typeof node === 'number') {
    return <Text key={`${keyPrefix}_txt`}>{String(node)}</Text>;
  }
  if (Array.isArray(node)) {
    return (
      <React.Fragment key={`${keyPrefix}_arr`}>
        {node.map((item, idx) => normalizeNode(item, `${keyPrefix}_${idx}`))}
      </React.Fragment>
    );
  }
  if (React.isValidElement(node)) {
    return <React.Fragment key={`${keyPrefix}_el`}>{node}</React.Fragment>;
  }
  return null;
}

function normalizeChildren(children: React.ReactNode[] | undefined): React.ReactNode[] {
  const nodes = children ?? [];
  return nodes.map((child, index) => normalizeNode(child, `node_${index}`));
}

/**
 * RN 渲染器 for :::note, :::tip, :::warning 等
 */
export function renderAdmonitionContainerRN({
  node,
  key,
  styles,
  theme: themeName = 'light',
  config,
  renderChildren,
}: ContainerRNRenderArgs): React.ReactNode {
  const kind = resolveKind(node);
  const theme = themeName === 'dark' ? darkKindTheme[kind] : kindTheme[kind];
  const title = String(node?.data?.title ?? '').trim() || theme.fallbackTitle;

  const renderedChildren = normalizeChildren(renderChildren(node.children ?? []));

  // Feature enable 检查：如果禁用，退化为普通块
  const isEnabled =
    !config || !config.features || config.features.length === 0
      ? true
      : (config.features.find((f: any) => f.id === '@supramark/feature-admonition')?.enabled ??
        true);

  if (!isEnabled) {
    return (
      <View key={key} style={localStyles.fallbackBlock}>
        {title ? (
          <Text style={[styles?.listItemText, localStyles.fallbackTitle]}>{title}</Text>
        ) : null}
        <View style={localStyles.content}>{renderedChildren}</View>
      </View>
    );
  }

  return (
    <View
      key={key}
      style={[
        localStyles.card,
        {
          backgroundColor: theme.bg,
          borderLeftColor: theme.border,
        },
      ]}
    >
      <View style={localStyles.header}>
        <Image source={theme.icon} style={localStyles.icon} resizeMode="contain" />
        <Text style={[localStyles.title, { color: theme.color }]}>{title}</Text>
      </View>
      <View style={localStyles.content}>{renderedChildren}</View>
    </View>
  );
}

const localStyles = StyleSheet.create({
  card: {
    width: '100%',
    borderLeftWidth: 4,
    borderRadius: 14,
    paddingHorizontal: 12,
    paddingVertical: 12,
    marginBottom: 12,
    minWidth: 288,
  },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    marginBottom: 8,
  },
  title: {
    fontSize: 16,
    fontWeight: '600',
  },
  content: {
    width: '100%',
  },
  icon: {
    width: 18,
    height: 18,
    marginRight: 4,
  },
  fallbackBlock: {
    width: '100%',
    marginBottom: 8,
  },
  fallbackTitle: {
    fontSize: 16,
    fontWeight: '600',
    marginBottom: 4,
  },
});
