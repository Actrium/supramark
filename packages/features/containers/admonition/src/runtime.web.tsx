/**
 * Admonition Web 渲染器
 *
 * 实现 ContainerWebRenderer 接口
 *
 * @packageDocumentation
 */

import React from 'react';
import type { ContainerWebRenderArgs, FeatureConfig } from '@supramark/core';

/**
 * Web 渲染器 for :::note, :::tip, :::warning 等
 */
export function renderAdmonitionContainerWeb({
  node,
  key,
  classNames,
  config,
  renderChildren,
}: ContainerWebRenderArgs): React.ReactNode {
  const kind = (node?.data?.kind as string | undefined) ?? 'note';
  const title = node?.data?.title as React.ReactNode;

  // Feature enable 检查：如果禁用，退化为普通段落
  const isEnabled =
    !config || !config.features || config.features.length === 0
      ? true
      : (config.features.find((f: FeatureConfig) => f.id === '@supramark/feature-admonition')?.enabled ??
        true);

  if (!isEnabled) {
    return (
      <p key={key} className={classNames.paragraph}>
        {title ? <strong>{title}</strong> : null}
        {title ? ' ' : null}
        {renderChildren(node.children ?? []) as React.ReactNode}
      </p>
    );
  }

  return (
    <div key={key} className={`admonition admonition-${kind} ${classNames.paragraph ?? ''}`.trim()}>
      {title ? (
        <p>
          <strong>{title}</strong>
        </p>
      ) : null}
      <div>{renderChildren(node.children ?? []) as React.ReactNode}</div>
    </div>
  );
}
