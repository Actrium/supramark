import type { ContainerHookContext } from '@supramark/core';
import { registerContainerHook } from '@supramark/core';
import type { SupramarkContainerNode } from '@supramark/core';
import type { VisonContainerData, VisonSpec } from './feature.js';

function extractInnerText(ctx: ContainerHookContext): string {
  const { token, sourceLines } = ctx;
  if (!token.map || token.map.length !== 2) return '';
  const [start, end] = token.map;
  const innerStart = start + 1;
  const innerEnd = end - 1 > innerStart ? end - 1 : end;
  return sourceLines.slice(innerStart, innerEnd).join('\n');
}

function tryParseSpec(source: string): { spec?: VisonSpec; parseError?: string } {
  const trimmed = source.trim();
  if (!trimmed) {
    return { parseError: 'empty body' };
  }
  try {
    const parsed = JSON.parse(trimmed) as VisonSpec;
    if (!parsed || typeof parsed !== 'object') {
      return { parseError: 'parsed value is not a JSON object' };
    }
    return { spec: parsed };
  } catch (err) {
    return { parseError: err instanceof Error ? err.message : String(err) };
  }
}

// Register the :::vison container hook. Body is parsed as JSON; result
// is attached as `data.spec`. We keep the original source on
// `data.source` so the host can re-render it as a fallback when
// parsing fails (or expose a "view source" affordance).
registerContainerHook({
  name: 'vison',
  opaque: true,
  onOpen(ctx: ContainerHookContext) {
    const source = extractInnerText(ctx);
    const { spec, parseError } = tryParseSpec(source);
    const data: VisonContainerData = parseError ? { source, parseError } : { source, spec };
    const node: SupramarkContainerNode = {
      type: 'container',
      name: 'vison',
      data: data as Record<string, unknown>,
      children: [],
    };
    const parent = ctx.stack[ctx.stack.length - 1];
    parent.children.push(node);
  },
});
