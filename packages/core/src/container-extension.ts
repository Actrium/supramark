export type ContainerParamValue = string | boolean;

export interface ContainerParams {
  raw: string;
  values: Record<string, ContainerParamValue>;
}

/**
 * 解析 :::name 后面的参数字符串。
 *
 * 规则：
 * - 支持多个键："a=1 b=two flag" -> { a: "1", b: "two", flag: true }
 * - 支持引号：title="Hello World" / title='Hello World'
 * - true/false（大小写不敏感）会转换成 boolean
 * - 不做 number coercion（"1" 保持 string）
 */
export function parseContainerParams(raw: string | undefined | null): ContainerParams {
  const text = (raw ?? '').trim();
  const values: Record<string, ContainerParamValue> = {};
  if (!text) return { raw: '', values };

  // 简单 tokenizer：支持双引号/单引号包裹的 value
  const tokens: string[] = [];
  let cur = '';
  let quote: '"' | "'" | null = null;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (quote) {
      if (ch === quote) {
        quote = null;
      } else {
        cur += ch;
      }
      continue;
    }

    if (ch === '"' || ch === "'") {
      quote = ch as '"' | "'";
      continue;
    }

    if (/\s/.test(ch)) {
      if (cur) {
        tokens.push(cur);
        cur = '';
      }
      continue;
    }

    cur += ch;
  }
  if (cur) tokens.push(cur);

  for (const t of tokens) {
    const eq = t.indexOf('=');
    if (eq === -1) {
      values[t] = true;
      continue;
    }
    const k = t.slice(0, eq);
    const v = t.slice(eq + 1);
    const lower = v.toLowerCase();
    if (lower === 'true') values[k] = true;
    else if (lower === 'false') values[k] = false;
    else values[k] = v;
  }

  return { raw: text, values };
}

/**
 * Container 扩展的声明（manifest）。
 *
 * 由 packages/features/container/feature-xxx/src/extension.ts 导出，用于生成 registry。
 */
export interface ContainerExtensionSpec {
  kind: 'container';

  /** feature 包 ID（通常等于包名） */
  featureId: string;

  /** 统一 container 节点名：node.type === 'container' 时，用 node.name === nodeName */
  nodeName: string;

  /** 支持的 :::xxx 名称列表（用于注册 hook / 解析入口） */
  containerNames: string[];

  /** 解析注册函数导出名（src/syntax.ts） */
  parserExport: string;

  /** Web 渲染函数导出名（src/runtime.web.tsx） */
  webRendererExport: string;

  /** RN 渲染函数导出名（src/runtime.rn.tsx） */
  rnRendererExport: string;

  /** 生成器内部填充：feature 目录名 */
  featureDir?: string;
}
