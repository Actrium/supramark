import type { RenderOptions } from '../types.js';
import { DiagramRenderError } from '../types.js';

/** Vega-Lite 引擎的渲染选项。 */
export interface Options extends RenderOptions {
  /**
   * 输入类型：'vega' 或 'vega-lite'。
   * 默认 'vega-lite'；若设为 'vega' 则跳过 vegaLite.compile()，直接渲染 vega spec。
   */
  dialect?: 'vega' | 'vega-lite';
}

// Vega/VegaLite 的鸭子类型（零硬依赖）。
interface VegaRuntime {
  parse: (spec: Record<string, unknown>) => unknown;
  View: new (runtime: unknown, opts: Record<string, unknown>) => VegaView;
}
interface VegaView {
  toSVG: () => Promise<string>;
  finalize: () => void;
}
interface VegaLiteCompiler {
  compile: (spec: Record<string, unknown>) => { spec: Record<string, unknown> };
}

function isVegaRuntime(value: unknown): value is VegaRuntime {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as VegaRuntime).parse === 'function' &&
    typeof (value as VegaRuntime).View === 'function'
  );
}
function isVegaLiteCompiler(value: unknown): value is VegaLiteCompiler {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as VegaLiteCompiler).compile === 'function'
  );
}

/**
 * Vega-Lite engine 工厂。
 *
 * Host 通过 `modules` 注入两个运行时模块：
 * - `vega`（含 `.parse` / `.View`）—— 必需
 * - `vega-lite`（含 `.compile`）—— 如果要渲染 vega-lite spec 则必需；纯 vega spec 可省
 *
 * @example
 * ```ts
 * import * as Vega     from 'vega';
 * import * as VegaLite from 'vega-lite';
 * import vegaLite      from '@supramark/engines/vega-lite';
 *
 * const render = vegaLite([Vega, VegaLite]);
 * const svg    = await render('{"mark":"bar","encoding":{...},"data":{...}}');
 * ```
 */
export default function vegaLite(modules?: unknown[]) {
  const items = modules ?? [];
  const vega = items.find(isVegaRuntime);
  const compiler = items.find(isVegaLiteCompiler);

  return async (code: string, options?: Options): Promise<string> => {
    options?.signal?.throwIfAborted();

    if (!vega) {
      throw new DiagramRenderError(
        'Vega runtime missing. Pass `import * as Vega from "vega"` in modules.',
        { engine: 'vega-lite', code: 'engine_unavailable' }
      );
    }

    let spec: Record<string, unknown>;
    try {
      spec = JSON.parse(code) as Record<string, unknown>;
    } catch (e) {
      throw new DiagramRenderError(
        `Spec JSON parse error: ${e instanceof Error ? e.message : String(e)}`,
        { engine: 'vega-lite', code: 'parse_error', input: code.slice(0, 200), cause: e }
      );
    }

    const dialect = options?.dialect ?? 'vega-lite';
    let vegaSpec: Record<string, unknown>;
    if (dialect === 'vega') {
      vegaSpec = spec;
    } else {
      if (!compiler) {
        throw new DiagramRenderError(
          'Vega-Lite compiler missing. Pass `import * as VegaLite from "vega-lite"` in modules, or set options.dialect = "vega".',
          { engine: 'vega-lite', code: 'engine_unavailable' }
        );
      }
      vegaSpec = compiler.compile(spec).spec;
    }

    const view = new vega.View(vega.parse(vegaSpec), { renderer: 'none' });
    try {
      return await view.toSVG();
    } catch (e) {
      throw new DiagramRenderError(
        `Vega render failed: ${e instanceof Error ? e.message : String(e)}`,
        { engine: 'vega-lite', code: 'render_error', input: code.slice(0, 200), cause: e }
      );
    } finally {
      view.finalize();
    }
  };
}
