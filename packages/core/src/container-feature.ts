/**
 * Container Feature 统一接口
 *
 * 为 :::xxx 容器类型的 Feature 定义精简、实用的接口规范。
 * 合并了原来分散在 feature.ts / extension.ts / syntax.ts 中的定义。
 *
 * ## 设计原则
 * - 每个字段都有明确的消费方
 * - 没有冗余，没有废话
 * - containerNames 全局唯一，由 feature:lint 检查
 *
 * @packageDocumentation
 */

import type { ExampleDefinition, SupramarkConfig } from './feature.js';
import type { SupramarkContainerNode, SupramarkNode } from './ast.js';

// ============================================================================
// ContainerFeature 接口
// ============================================================================

/**
 * Container 类型 Feature 的统一接口
 *
 * 每个 :::xxx 容器 Feature 必须实现此接口。
 *
 * @example
 * ```typescript
 * export const admonitionFeature: ContainerFeature = {
 *   id: '@supramark/feature-admonition',
 *   name: 'Admonition',
 *   version: '0.1.0',
 *   description: '提示框容器（note/tip/warning 等）',
 *   containerNames: ['note', 'tip', 'info', 'warning', 'danger'],
 *   registerParser: () => { ... },
 *   webRendererExport: 'renderAdmonitionContainerWeb',
 *   rnRendererExport: 'renderAdmonitionContainerRN',
 * };
 * ```
 */
export interface ContainerFeature {
  // ============================================================================
  // 元数据（必填）
  // ============================================================================

  /**
   * Feature 唯一标识符
   *
   * 格式: @scope/feature-name
   * 示例: @supramark/feature-admonition
   *
   * 消费方: feature:lint, FeatureRegistry, 配置系统
   */
  id: string;

  /**
   * Feature 显示名称
   *
   * 示例: 'Admonition', 'Weather'
   *
   * 消费方: 文档生成, UI 展示
   */
  name: string;

  /**
   * 版本号（语义化版本）
   *
   * 格式: x.y.z
   * 示例: '0.1.0', '1.0.0'
   *
   * 消费方: 版本检查, 文档
   */
  version: string;

  /**
   * 简短描述（可选）
   *
   * 消费方: 文档生成, package.json description
   */
  description?: string;

  // ============================================================================
  // 容器定义（必填）
  // ============================================================================

  /**
   * 支持的 :::xxx 容器名称列表
   *
   * 示例: ['note', 'tip', 'info', 'warning', 'danger']
   *
   * **重要**: 这些名称必须全局唯一，不能与其他 Feature 冲突。
   * feature:lint 会检查全局唯一性。
   *
   * 消费方: 解析器注册, feature:lint 唯一性检查
   */
  containerNames: string[];

  // ============================================================================
  // 解析器注册（必填）
  // ============================================================================

  /**
   * 注册解析器的函数
   *
   * 调用此函数会注册所有 containerNames 对应的解析 hook。
   * 通常内部调用 registerContainerHook()。
   *
   * 消费方: 生成的 registry 文件
   */
  registerParser: () => void;

  // ============================================================================
  // 渲染器导出（可选，供 registry 生成）
  // ============================================================================

  /**
   * Web 渲染函数的导出名
   *
   * 示例: 'renderAdmonitionContainerWeb'
   *
   * 消费方: feature-sync.ts 生成 web registry
   */
  webRendererExport?: string;

  /**
   * React Native 渲染函数的导出名
   *
   * 示例: 'renderAdmonitionContainerRN'
   *
   * 消费方: feature-sync.ts 生成 rn registry
   */
  rnRendererExport?: string;
}

// ============================================================================
// 渲染器接口
// ============================================================================

/**
 * Container Web 渲染函数的参数
 */
export interface ContainerWebRenderArgs {
  /** AST 节点 */
  node: SupramarkContainerNode;
  /** React key */
  key: number;
  /** CSS 类名映射 */
  classNames: Record<string, string>;
  /** Supramark 配置 */
  config?: SupramarkConfig;
  /** 渲染子节点的函数 */
  renderChildren: (children: SupramarkNode[]) => unknown;
}

/**
 * Container Web 渲染函数类型
 *
 * 每个 runtime.web.tsx 的渲染函数必须符合此签名。
 */
export type ContainerWebRenderer = (args: ContainerWebRenderArgs) => unknown;

/**
 * Container RN 渲染函数的参数
 */
export interface ContainerRNRenderArgs {
  /** AST 节点 */
  node: SupramarkContainerNode;
  /** React key */
  key: number;
  /** RN 样式映射 */
  styles: Record<string, unknown>;
  /** Supramark 配置 */
  config?: SupramarkConfig;
  /** 渲染子节点的函数 */
  renderChildren: (children: SupramarkNode[]) => unknown;
}

/**
 * Container RN 渲染函数类型
 *
 * 每个 runtime.rn.tsx 的渲染函数必须符合此签名。
 */
export type ContainerRNRenderer = (args: ContainerRNRenderArgs) => unknown;

// ============================================================================
// Examples 接口（重导出）
// ============================================================================

/**
 * 示例定义
 *
 * 每个 examples.ts 必须导出 ExampleDefinition[] 类型的数组。
 *
 * 重导出自 feature.ts 以保持兼容。
 */
export type { ExampleDefinition };

// ============================================================================
// 验证函数
// ============================================================================

/**
 * 验证 ContainerFeature 实现的完整性
 *
 * @param feature - Feature 定义
 * @returns 验证结果
 */
export function validateContainerFeature(feature: Partial<ContainerFeature>): {
  valid: boolean;
  errors: Array<{ code: string; message: string }>;
} {
  const errors: Array<{ code: string; message: string }> = [];

  // 必填字段检查
  if (!feature.id) {
    errors.push({ code: 'id-required', message: 'Feature must have an id' });
  } else if (!/^@[\w-]+\/feature-[\w-]+$/.test(feature.id)) {
    errors.push({
      code: 'id-format',
      message: 'Feature id must match @scope/feature-name format',
    });
  }

  if (!feature.name || feature.name.trim().length === 0) {
    errors.push({ code: 'name-required', message: 'Feature must have a name' });
  }

  if (!feature.version) {
    errors.push({ code: 'version-required', message: 'Feature must have a version' });
  } else if (!/^\d+\.\d+\.\d+$/.test(feature.version)) {
    errors.push({
      code: 'version-format',
      message: 'Feature version must be semver format (x.y.z)',
    });
  }

  if (!feature.containerNames || feature.containerNames.length === 0) {
    errors.push({
      code: 'containerNames-required',
      message: 'Feature must define at least one containerName',
    });
  }

  if (typeof feature.registerParser !== 'function') {
    errors.push({
      code: 'registerParser-required',
      message: 'Feature must have a registerParser function',
    });
  }

  return {
    valid: errors.length === 0,
    errors,
  };
}
