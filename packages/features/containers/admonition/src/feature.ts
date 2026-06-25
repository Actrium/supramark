/**
 * Admonition Feature 定义
 *
 * 实现 ContainerFeature 接口，合并了元数据、容器定义和解析器注册。
 *
 * @example
 * ```markdown
 * :::note 提示标题
 * 这是一个提示框内容
 * :::
 *
 * :::warning 警告
 * 请注意这个警告信息
 * :::
 * ```
 *
 * @packageDocumentation
 */

import {
  registerContainerHook,
  type ContainerFeature,
  type ContainerHook,
  type ContainerHookContext,
  type SupramarkContainerNode,
} from '@supramark/core';

type ContainerTokenLike = {
  info?: string;
};

// ============================================================================
// 容器名称定义（唯一事实来源）
// ============================================================================

/**
 * Admonition 支持的容器名称
 *
 * 全局唯一，不能与其他 Feature 冲突。
 */
export const ADMONITION_CONTAINER_NAMES = [
  'note',
  'tip',
  'info',
  'warning',
  'danger',
] as const;

export type AdmonitionKind = (typeof ADMONITION_CONTAINER_NAMES)[number];

// ============================================================================
// 解析逻辑
// ============================================================================

function parseTitle(token: ContainerTokenLike, _kind: string): string | undefined {
  const info = (token.info || '').trim();
  // info 形如 "note 标题..."，第一个单词是容器名(kind)
  const parts = info.split(/\s+/).filter(Boolean);
  const titleParts = parts.length > 1 ? parts.slice(1) : [];
  return titleParts.length > 0 ? titleParts.join(' ') : undefined;
}

function createAdmonitionContainerHook(kind: string): ContainerHook {
  return {
    name: kind,
    opaque: false,
    onOpen(ctx: ContainerHookContext) {
      const title = parseTitle(ctx.token, kind);
      const node: SupramarkContainerNode = {
        type: 'container' as const,
        name: 'admonition',
        params: ctx.token.info ? String(ctx.token.info) : undefined,
        data: {
          kind,
          title,
        },
        children: [],
      };
      const parent = ctx.stack[ctx.stack.length - 1];
      parent.children.push(node);
      ctx.stack.push(node);
    },
    onClose(ctx: ContainerHookContext) {
      const top = ctx.stack[ctx.stack.length - 1] as SupramarkContainerNode;
      if (top && top.type === 'container' && top.name === 'admonition') {
        ctx.stack.pop();
      }
    },
  };
}

/**
 * 注册 Admonition 解析器
 *
 * 为所有 containerNames 注册解析 hook。
 */
function registerAdmonitionParser(): void {
  for (const kind of ADMONITION_CONTAINER_NAMES) {
    registerContainerHook(createAdmonitionContainerHook(kind));
  }
}

// ============================================================================
// Feature 定义（实现 ContainerFeature 接口）
// ============================================================================

/**
 * Admonition Feature
 *
 * 提示框容器块语法支持（note/tip/warning 等）
 */
export const admonitionFeature: ContainerFeature = {
  // 元数据
  id: '@supramark/feature-admonition',
  name: 'Admonition',
  version: '0.1.0',
  description: '提示框容器块语法支持（note/tip/warning 等）',

  // 容器定义
  containerNames: [...ADMONITION_CONTAINER_NAMES],

  // 解析器注册
  registerParser: registerAdmonitionParser,

  // 渲染器导出名
  webRendererExport: 'renderAdmonitionContainerWeb',
  rnRendererExport: 'renderAdmonitionContainerRN',
};

// ============================================================================
// 兼容性导出（保持向后兼容）
// ============================================================================

/**
 * @deprecated 使用 admonitionFeature.registerParser() 代替
 */
export const registerAdmonitionContainer = registerAdmonitionParser;
