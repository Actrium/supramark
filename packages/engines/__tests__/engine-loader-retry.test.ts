import { describe, expect, it } from 'bun:test';

import { createWebDiagramEngine } from '../src/web';

/**
 * A rejected loader promise must not be cached forever — otherwise a single
 * transient load failure (chunk 404, wasm init error) permanently bricks the
 * engine for the app lifetime, since `defaultDiagramEngine` is a module-level
 * singleton. The cached promise is dropped on rejection so the next render
 * retries the load. See #161.
 */
describe('LocalDiagramEngine loader retry on rejection (#161)', () => {
  it('retries loadRender after a rejection instead of re-awaiting the cached rejected promise', async () => {
    let loadCalls = 0;
    const loadRender = async () => {
      loadCalls += 1;
      if (loadCalls === 1) {
        throw new Error('transient chunk failure');
      }
      return async () => '<svg>ok</svg>';
    };

    const engine = createWebDiagramEngine({ echarts: { loadRender } });

    const first = await engine.render({ engine: 'echarts', code: 'graph' });
    expect(first.success).toBe(false);
    expect(loadCalls).toBe(1);

    // Before the fix the rejected promise was cached and re-awaited forever:
    // the second render would NOT call loadRender again and stayed failed.
    const second = await engine.render({ engine: 'echarts', code: 'graph' });
    expect(loadCalls).toBe(2);
    expect(second.success).toBe(true);
    expect(second.payload).toBe('<svg>ok</svg>');
  });

  it('caches a successful loadRender promise across renders (no re-load on hit)', async () => {
    let loadCalls = 0;
    const loadRender = async () => {
      loadCalls += 1;
      return async () => '<svg>ok</svg>';
    };

    const engine = createWebDiagramEngine({ echarts: { loadRender } });

    await engine.render({ engine: 'echarts', code: 'a' });
    await engine.render({ engine: 'echarts', code: 'b' });

    expect(loadCalls).toBe(1);
  });

  it('retries a rejected graphviz loadAdapter on the next render', async () => {
    let loadCalls = 0;
    const loadAdapter = async () => {
      loadCalls += 1;
      if (loadCalls === 1) throw new Error('transient adapter failure');
      return { renderToSvg: async () => '<svg>gv</svg>' };
    };

    const engine = createWebDiagramEngine({ graphviz: { loadAdapter } });

    const first = await engine.render({ engine: 'dot', code: 'digraph { a -> b }' });
    expect(first.success).toBe(false);
    expect(loadCalls).toBe(1);

    const second = await engine.render({ engine: 'dot', code: 'digraph { a -> b }' });
    expect(loadCalls).toBe(2);
    expect(second.success).toBe(true);
    expect(second.payload).toBe('<svg>gv</svg>');
  });

  // vega-lite / plantuml / d2 share the same cacheRetryableLoad invariant as
  // echarts above; exercise each so the contract is locked in per-loader rather
  // than only on the one we happened to test first. See #161.
  const cases: Array<{ engine: 'vega-lite' | 'plantuml' | 'd2'; optionKey: 'vegaLite' | 'plantuml' | 'd2' }> = [
    { engine: 'vega-lite', optionKey: 'vegaLite' },
    { engine: 'plantuml', optionKey: 'plantuml' },
    { engine: 'd2', optionKey: 'd2' },
  ];
  for (const { engine: engineName, optionKey } of cases) {
    it(`retries a rejected ${engineName} loadRender on the next render`, async () => {
      let loadCalls = 0;
      const loadRender = async () => {
        loadCalls += 1;
        if (loadCalls === 1) throw new Error('transient chunk failure');
        return async () => '<svg>ok</svg>';
      };

      const engine = createWebDiagramEngine({ [optionKey]: { loadRender } });

      const first = await engine.render({ engine: engineName, code: 'x' });
      expect(first.success).toBe(false);
      expect(loadCalls).toBe(1);

      const second = await engine.render({ engine: engineName, code: 'x' });
      expect(loadCalls).toBe(2);
      expect(second.success).toBe(true);
      expect(second.payload).toBe('<svg>ok</svg>');
    });
  }
});
