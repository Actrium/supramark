import type { RenderOptions } from '../types.js';
import { DiagramRenderError } from '../types.js';

/** ECharts 引擎的渲染选项。 */
export interface Options extends RenderOptions {
  /** 画布背景色，透明用 `"transparent"`。 */
  backgroundColor?: string;
}

// ECharts 核心对象的鸭子类型（避免硬依赖 `echarts` 包）。
interface EChartsCore {
  init(dom: unknown, theme: unknown, opts: Record<string, unknown>): EChartsInstance;
  use(modules: unknown[]): void;
}
interface EChartsInstance {
  setOption(option: Record<string, unknown>): void;
  renderToSVGString(): string;
  dispose(): void;
}

function isEchartsCore(value: unknown): value is EChartsCore {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as EChartsCore).init === 'function' &&
    typeof (value as EChartsCore).use === 'function'
  );
}

/**
 * ECharts engine 工厂。
 *
 * **Host 必须通过 `modules` 至少传入 ECharts core 实例**
 * （通常来自 `echarts/core`），其余为 chart type / component / renderer
 * 等 ECharts 模块，会被 `core.use(...)` 注册。
 *
 * @example
 * ```ts
 * import * as core       from 'echarts/core';
 * import { SVGRenderer } from 'echarts/renderers';
 * import { LineChart }   from 'echarts/charts';
 * import { GridComponent, TooltipComponent } from 'echarts/components';
 * import echarts         from '@supramark/engines/echarts';
 *
 * const render = echarts([core, SVGRenderer, LineChart, GridComponent, TooltipComponent]);
 * const svg    = await render('{"xAxis":{"type":"category","data":["A","B"]}, ...}');
 * ```
 *
 * 上面的写法让 bundler 的静态分析精确到"只带 LineChart + 三个 component + SVGRenderer"，
 * 其他 chart type 被 tree-shake 砍掉。
 */
export default function echarts(modules?: unknown[]) {
  const items = modules ?? [];
  const core = items.find(isEchartsCore);
  const rest = items.filter(m => m !== core);

  // 模块注册只需一次（echarts.use 幂等），放在工厂外部。
  if (core && rest.length > 0) {
    core.use(rest);
  }

  // RenderFn contract requires a Promise return; echarts SSR is synchronous so
  // there is no await inside, but the async signature must be kept.
  // eslint-disable-next-line @typescript-eslint/require-await
  return async (code: string, options?: Options): Promise<string> => {
    options?.signal?.throwIfAborted();

    if (!core) {
      throw new DiagramRenderError(
        'ECharts core instance missing. Pass `import * as core from "echarts/core"` in modules.',
        { engine: 'echarts', code: 'engine_unavailable' }
      );
    }

    let option: Record<string, unknown>;
    try {
      option = JSON.parse(code) as Record<string, unknown>;
    } catch (e) {
      throw new DiagramRenderError(
        `ECharts option JSON parse error: ${e instanceof Error ? e.message : String(e)}`,
        { engine: 'echarts', code: 'parse_error', input: code.slice(0, 200), cause: e }
      );
    }

    const width = options?.width ?? 600;
    const height = options?.height ?? 400;

    const chart = core.init(null, null, {
      renderer: 'svg',
      ssr: true,
      width,
      height,
      backgroundColor: options?.backgroundColor,
    });

    try {
      chart.setOption(option);
      return chart.renderToSVGString().replace(/pointer-events="visible"/g, '');
    } catch (e) {
      throw new DiagramRenderError(
        `ECharts render failed: ${e instanceof Error ? e.message : String(e)}`,
        { engine: 'echarts', code: 'render_error', input: code.slice(0, 200), cause: e }
      );
    } finally {
      chart.dispose();
    }
  };
}
