import echartsFactory from './echarts';
import type { DiagramRenderFn } from './types';
import vegaLiteFactory from './vega-lite';

/**
 * Shared JS SVG loaders for browser-like hosts.
 *
 * These loaders do not depend on browser DOM rendering. ECharts uses its SVG
 * SSR path and Vega/Vega-Lite use headless SVG export, so Web and RN can share
 * the same output contract: source in, SVG string out.
 */
export async function loadEchartsSvgRender(): Promise<DiagramRenderFn> {
  const [core, renderers, charts, components] = await Promise.all([
    import('echarts/core' as string),
    import('echarts/renderers' as string),
    import('echarts/charts' as string),
    import('echarts/components' as string),
  ]);

  return echartsFactory([
    core,
    renderers.SVGRenderer,
    charts.LineChart,
    charts.BarChart,
    charts.PieChart,
    charts.ScatterChart,
    components.GridComponent,
    components.TooltipComponent,
    components.TitleComponent,
    components.LegendComponent,
  ]) as DiagramRenderFn;
}

export async function loadVegaLiteSvgRender(): Promise<DiagramRenderFn> {
  const [Vega, VegaLite] = await Promise.all([
    import('vega' as string),
    import('vega-lite' as string),
  ]);

  return vegaLiteFactory([Vega, VegaLite]) as DiagramRenderFn;
}
