import type { RenderFn, RenderOptions } from '../types.js';

/**
 * 由 `@supramark/cli` 生成的 spec 形状：
 *
 * - `engines`：engine name → render 函数（host 通过工厂装配）。
 * - `features`：交给 `@supramark/core` 的 parser 配置。
 */
export interface RenderSpec {
  engines: Record<string, RenderFn>;
  features?: Record<string, unknown>;
}

/**
 * 用 spec 生成一个 Markdown → HTML 字符串的纯函数。
 *
 * Phase 1 只提供类型骨架；真正的实现在 Phase 5 迁入
 * （目前走 `@supramark/web` 的 `Supramark` + `renderToString`）。
 *
 * @example
 * ```ts
 * const render = createRender({ engines, features });
 * const html   = await render(markdown);
 * ```
 */
export function createRender(
  _spec: RenderSpec
): (markdown: string, options?: RenderOptions) => Promise<string> {
  // Returned render fn must satisfy the `Promise<string>` contract; the Phase 5
  // stub only throws, so there is nothing to await yet.
  // eslint-disable-next-line @typescript-eslint/require-await
  return async (_markdown, _options) => {
    throw new Error(
      '[@supramark/engines] createRender is not implemented yet (Phase 5). ' +
        'Track ENGINES_AND_CLI_PLAN.md §8 for progress.'
    );
  };
}
