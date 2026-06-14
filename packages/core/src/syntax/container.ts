import type { SupramarkParentNode } from '../ast.js';
import type { SupramarkConfig } from '../feature.js';

export interface SupramarkContainerToken {
  type?: string;
  info?: string;
  map?: [number, number] | number[] | null;
  meta?: Record<string, unknown>;
  [key: string]: unknown;
}

/**
 * 容器语法处理上下文。
 *
 * AST v2 的容器扫描已迁移到 Rust `supramark-markdown`。该上下文只保留给
 * 旧 feature runtime 编译和后处理迁移使用。
 */
export interface ContainerProcessorContext {
  config?: SupramarkConfig;
  sourceLines: string[];
  stack: SupramarkParentNode[];
}

export interface ContainerHookContext extends ContainerProcessorContext {
  token: SupramarkContainerToken;
  name: string;
  phase: 'open' | 'close';
}

export interface ContainerHook {
  /** 容器名称，对应 :::name 中的 name。 */
  name: string;

  /** 历史字段；AST v2 使用节点上的 `mode` 表达透明/不透明。 */
  opaque?: boolean;

  onOpen: (ctx: ContainerHookContext) => void;
  onClose?: (ctx: ContainerHookContext) => void;
}

const customContainerHooks: ContainerHook[] = [];

export function registerContainerHook(hook: ContainerHook): void {
  const existingIndex = customContainerHooks.findIndex(existing => existing.name === hook.name);
  if (existingIndex >= 0) {
    customContainerHooks[existingIndex] = hook;
    return;
  }
  customContainerHooks.push(hook);
}

export function getRegisteredContainerHooks(): readonly ContainerHook[] {
  return customContainerHooks;
}

/**
 * @deprecated Markdown token registration was removed in AST v2.
 */
export function registerContainerSyntax(_parser: unknown, _config?: SupramarkConfig): void {
  // AST v2 containers are scanned by supramark-markdown.
}

/**
 * @deprecated Token processors are no longer part of the public parser path.
 */
export function createContainerTokenProcessor(
  _context: ContainerProcessorContext
): (_token: SupramarkContainerToken) => boolean {
  return () => false;
}

/**
 * 从历史 token.map 信息中提取容器内部原始文本。
 */
export function extractContainerInnerText(
  token: SupramarkContainerToken,
  sourceLines: string[]
): string {
  if (!Array.isArray(token.map) || token.map.length !== 2) {
    return '';
  }
  const [start, end] = token.map;
  if (typeof start !== 'number' || typeof end !== 'number') {
    return '';
  }
  return sourceLines.slice(start + 1, end).join('\n');
}
