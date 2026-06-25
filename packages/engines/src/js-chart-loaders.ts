import echartsFactory from './echarts';
import type { DiagramRenderFn } from './types';
import vegaLiteFactory from './vega-lite';

/**
 * Minimal surfaces of the dynamically-imported `echarts/*` subpath modules.
 *
 * Only the named exports the factory wiring touches are typed; each is an
 * opaque echarts module token consumed by `core.use(...)`, so `unknown` is the
 * right element type (the factory receives `unknown[]`).
 */
interface EchartsCoreModule {
  [key: string]: unknown;
}
interface EchartsRenderersModule {
  SVGRenderer: unknown;
}
interface EchartsChartsModule {
  LineChart: unknown;
  BarChart: unknown;
  PieChart: unknown;
  ScatterChart: unknown;
}
interface EchartsComponentsModule {
  GridComponent: unknown;
  TooltipComponent: unknown;
  TitleComponent: unknown;
  LegendComponent: unknown;
}

/** The two vega runtime modules are passed opaquely into the factory. */
type VegaModule = Record<string, unknown>;

/**
 * Shared JS SVG loaders for browser-like hosts.
 *
 * These loaders do not depend on browser DOM rendering. ECharts uses its SVG
 * SSR path and Vega/Vega-Lite use headless SVG export, so Web and RN can share
 * the same output contract: source in, SVG string out.
 */
export async function loadEchartsSvgRender(): Promise<DiagramRenderFn> {
  // `spec: string` keeps these as unresolved specifiers so TS does not require
  // the optional `echarts` peer dependency to be installed at type-check time.
  const coreSpec: string = 'echarts/core';
  const renderersSpec: string = 'echarts/renderers';
  const chartsSpec: string = 'echarts/charts';
  const componentsSpec: string = 'echarts/components';

  const [core, renderers, charts, components] = await Promise.all([
    import(coreSpec) as Promise<EchartsCoreModule>,
    import(renderersSpec) as Promise<EchartsRenderersModule>,
    import(chartsSpec) as Promise<EchartsChartsModule>,
    import(componentsSpec) as Promise<EchartsComponentsModule>,
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
  const vegaSpec: string = 'vega';
  const vegaLiteSpec: string = 'vega-lite';

  const [Vega, VegaLite] = await Promise.all([
    import(vegaSpec) as Promise<VegaModule>,
    import(vegaLiteSpec) as Promise<VegaModule>,
  ]);

  return vegaLiteFactory([Vega, VegaLite]) as DiagramRenderFn;
}
