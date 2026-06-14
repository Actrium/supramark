import { describe, expect, it } from 'bun:test';
import { createDiagramEngine } from '../src/engine';
import { createReactNativeDiagramEngine } from '../src/rn';

const ECHARTS_CODE = JSON.stringify({
  xAxis: { type: 'category', data: ['A', 'B'] },
  yAxis: { type: 'value' },
  series: [{ type: 'bar', data: [3, 5] }],
});

const VEGA_LITE_CODE = JSON.stringify({
  data: {
    values: [
      { label: 'A', value: 3 },
      { label: 'B', value: 5 },
    ],
  },
  mark: 'bar',
  encoding: {
    x: { field: 'label', type: 'nominal' },
    y: { field: 'value', type: 'quantitative' },
  },
});

const VEGA_CODE = JSON.stringify({
  $schema: 'https://vega.github.io/schema/vega/v5.json',
  width: 100,
  height: 60,
  marks: [
    {
      type: 'rect',
      encode: {
        enter: {
          x: { value: 0 },
          y: { value: 0 },
          width: { value: 80 },
          height: { value: 40 },
          fill: { value: '#4f46e5' },
        },
      },
    },
  ],
});

describe('JS SVG diagram engines', () => {
  it('routes chart aliases through the Vega-Lite renderer', async () => {
    const calls: string[] = [];
    const engine = createDiagramEngine({
      vegaLite: {
        render: async (_code, options) => {
          calls.push(String(options?.dialect ?? 'vega-lite'));
          return '<svg data-engine="vega-lite"></svg>';
        },
      },
    });

    for (const alias of ['chart', 'chartjs']) {
      const result = await engine.render({ engine: alias, code: VEGA_LITE_CODE });
      expect(result.success).toBe(true);
      expect(result.format).toBe('svg');
      expect(result.payload).toContain('<svg');
    }

    expect(calls).toEqual(['vega-lite', 'vega-lite']);
  });

  it('renders ECharts and Vega-family diagrams through the default RN SVG route', async () => {
    const engine = createReactNativeDiagramEngine();
    const cases = [
      ['echarts', ECHARTS_CODE],
      ['vega-lite', VEGA_LITE_CODE],
      ['vega', VEGA_CODE],
      ['chart', VEGA_LITE_CODE],
      ['chartjs', VEGA_LITE_CODE],
    ] as const;

    for (const [name, code] of cases) {
      const result = await engine.render({ engine: name, code });
      expect(result.success).toBe(true);
      expect(result.format).toBe('svg');
      expect(result.payload.trimStart().startsWith('<svg')).toBe(true);
    }
  });
});
