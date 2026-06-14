import type { SupramarkParentNode } from '../ast.js';
import type { SupramarkConfig } from '../feature.js';

export interface SupramarkInputToken {
  type?: string;
  info?: string;
  map?: [number, number] | number[] | null;
  meta?: Record<string, unknown>;
  [key: string]: unknown;
}

/**
 * Input 语法处理上下文。
 *
 * AST v2 的 %%% input 扫描已迁移到 Rust `supramark-markdown`。该上下文只保留给
 * 旧 feature runtime 编译和后处理迁移使用。
 */
export interface InputProcessorContext {
  config?: SupramarkConfig;
  sourceLines: string[];
  stack: SupramarkParentNode[];
}

export interface InputHookContext extends InputProcessorContext {
  token: SupramarkInputToken;
  name: string;
  phase: 'open' | 'close';
}

export interface InputHook {
  /** Input 块名称，对应 %%%name 中的 name。 */
  name: string;

  /** 历史字段；AST v2 使用节点上的 `mode` 表达透明/不透明。 */
  opaque?: boolean;

  onOpen: (ctx: InputHookContext) => void;
  onClose?: (ctx: InputHookContext) => void;
}

const customInputHooks: InputHook[] = [];

export function registerInputHook(hook: InputHook): void {
  const existingIndex = customInputHooks.findIndex(existing => existing.name === hook.name);
  if (existingIndex >= 0) {
    customInputHooks[existingIndex] = hook;
    return;
  }
  customInputHooks.push(hook);
}

export function getRegisteredInputHooks(): readonly InputHook[] {
  return customInputHooks;
}

/**
 * @deprecated Markdown token registration was removed in AST v2.
 */
export function registerInputSyntax(_parser: unknown, _config?: SupramarkConfig): void {
  // AST v2 inputs are scanned by supramark-markdown.
}

/**
 * @deprecated Token processors are no longer part of the public parser path.
 */
export function createInputProcessor(
  _context: InputProcessorContext
): (_token: SupramarkInputToken) => boolean {
  return () => false;
}

/**
 * 从历史 token.map 信息中提取 input 内部原始文本。
 */
export function extractInputInnerText(token: SupramarkInputToken, sourceLines: string[]): string {
  if (!Array.isArray(token.map) || token.map.length !== 2) {
    return '';
  }
  const [start, end] = token.map;
  if (typeof start !== 'number' || typeof end !== 'number') {
    return '';
  }
  const innerStart = start + 1;
  const innerEnd = end - 1 > innerStart ? end - 1 : end;
  return sourceLines.slice(innerStart, innerEnd).join('\n');
}
