import { describe, expect, it, mock } from 'bun:test';

// The injected-`loadAdapter` test in `engine-loader-retry.test.ts` bypasses
// `createWebGraphvizAdapterLoader()`, which is precisely the production path
// that defeated the #161 fix: the default loader caches its own
// `adapterPromise` with no clear-on-rejection, so once `Graphviz.load()`
// rejects the engine stays bricked for life even after `engine.ts`'s outer
// cache is cleared. This file exercises the DEFAULT wiring
// (`createWebDiagramEngine({})` → `createWebGraphvizAdapterLoader` →
// `loadWebGraphvizAdapter` → `@actrium/graphviz-anywhere-web`'s
// `Graphviz.load()`) with a module mock that counts real load attempts.

let loadCalls = 0;

mock.module('@actrium/graphviz-anywhere-web', () => ({
  __esModule: true,
  Graphviz: {
    load: () => {
      loadCalls += 1;
      if (loadCalls === 1) {
        return Promise.reject(new Error('transient wasm load failure'));
      }
      return Promise.resolve({
        layout: () => '<svg>gv</svg>',
        version: () => 'mock-1.0',
      });
    },
  },
}));

const { createWebDiagramEngine } = await import('../src/web');

describe('createWebGraphvizAdapterLoader retry on rejection (#161, default wiring)', () => {
  it('retries Graphviz.load on the next render after a transient failure', async () => {
    // No injected `loadAdapter` — exercise the production default wiring.
    const engine = createWebDiagramEngine();
    const code = 'digraph { a -> b }';

    const first = await engine.render({ engine: 'dot', code });
    expect(first.success).toBe(false);
    expect(loadCalls).toBe(1);

    // Before the inner-cache fix, the rejected `adapterPromise` was pinned
    // inside `createWebGraphvizAdapterLoader` and the second render re-awaited
    // the same rejected promise (loadCalls stayed at 1).
    const second = await engine.render({ engine: 'dot', code });
    expect(loadCalls).toBe(2);
    expect(second.success).toBe(true);
    expect(second.payload).toBe('<svg>gv</svg>');
  });
});
