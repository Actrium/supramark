import type { SupramarkNode, SupramarkRootNode } from '@supramark/core';

/**
 * The real canonical parser, loaded by file path on purpose: bun's module mocks
 * are process-wide, and the renderer package's `SupramarkCache.test.tsx` stubs
 * the `@supramark/markdown-web` specifier for the whole run — so when the root
 * suite runs these tests, a specifier import would return that stub. Going
 * through the dist entry bypasses it. The node entry (not the browser one) is
 * what core's loader actually picks under bun: it instantiates its own wasm
 * build, whereas the browser entry expects a bundler to hand it an instance.
 */
const realParser = (await import(
  '../../../../../../crates/supramark-markdown/packages/web/dist/node.js'
)) as {
  parse: (markdown: string) => Promise<SupramarkRootNode>;
};

/**
 * Mirror `expandOpaqueContainers` with the real parser: containers parsed by
 * the canonical parser carry their body in `value` with empty children, and the
 * renderer fills children in before rendering. Doing it here keeps the AST real
 * without routing through the stubbed specifier.
 */
export async function expandContainers(node: SupramarkNode): Promise<void> {
  const children = (node as { children?: SupramarkNode[] }).children;
  if (!Array.isArray(children)) return;
  for (const child of children) {
    const container = child as {
      type?: string;
      mode?: string;
      data?: unknown;
      value?: string;
      children?: SupramarkNode[];
    };
    if (
      container.type === 'container' &&
      container.mode === 'opaque' &&
      container.data == null &&
      typeof container.value === 'string' &&
      container.value.length > 0
    ) {
      const sub = await realParser.parse(container.value);
      container.children = sub.children;
      container.value = undefined;
    }
    await expandContainers(child);
  }
}

/** Parse real Markdown and fill container children — the renderer's own pipeline. */
export async function parseRealMarkdown(markdown: string): Promise<SupramarkRootNode> {
  const ast = await realParser.parse(markdown);
  await expandContainers(ast);
  return ast;
}