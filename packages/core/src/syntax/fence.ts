import type {
  SupramarkCodeNode,
  SupramarkDiagramNode,
  SupramarkParentNode,
} from '../ast.js';
import { isBuiltInDiagramEngine } from '../ast.js';

export interface SupramarkFenceToken {
  info?: string;
  content?: string;
  [key: string]: unknown;
}

/**
 * 判断 fenced code 的语言是否属于内置 diagram 引擎。
 */
export function isDiagramFenceLanguage(lang?: string | null): boolean {
  if (!lang) return false;
  const engine = lang.toLowerCase();
  return isBuiltInDiagramEngine(engine);
}

/**
 * @deprecated Fence mapping is now implemented by Rust `supramark-markdown`.
 */
export function mapFenceTokenToBlockNode(
  token: SupramarkFenceToken,
  parent: SupramarkParentNode
): void {
  const info = typeof token.info === 'string' ? token.info.trim() : '';
  const [langRaw, ...metaParts] = info.split(/\s+/);
  const lang = langRaw || undefined;
  const meta = metaParts.length > 0 ? metaParts.join(' ') : undefined;
  const content = typeof token.content === 'string' ? token.content : '';

  if (isDiagramFenceLanguage(lang)) {
    const diagram: SupramarkDiagramNode = {
      type: 'diagram',
      engine: lang!.toLowerCase(),
      code: content,
      meta: meta ? { raw: meta } : undefined,
    };
    parent.children.push(diagram);
    return;
  }

  const codeBlock: SupramarkCodeNode = {
    type: 'code',
    value: content,
    lang,
    meta,
  };
  parent.children.push(codeBlock);
}
