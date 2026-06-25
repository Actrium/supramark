import { describe, expect, it, mock } from 'bun:test';

// Record the args the native module receives so we can assert delegation
// and the default engine/format applied by the JS wrapper.
const renderCalls: Array<{ dot: string; engine: string; format: string }> = [];
const bridgeNative = {
  renderDot: async (dot: string, engine: string, format: string) => {
    renderCalls.push({ dot, engine, format });
    return `<svg data-engine="${engine}" data-format="${format}">${dot}</svg>`;
  },
  getVersion: async () => '12.2.1',
};
mock.module('react-native', () => ({
  NativeModules: { GraphvizNative: bridgeNative },
  Platform: {
    select: (options: Record<string, string | undefined>) => options.default ?? '',
  },
  TurboModuleRegistry: { getEnforcing: () => undefined },
}));

const { resolveNative, renderDot, getVersion, GraphvizErrorCode } = await import('../src/index');

const turbo = { renderDot: async () => 'turbo', getVersion: async () => 'turbo' };
const bridge = { renderDot: async () => 'bridge', getVersion: async () => 'bridge' };

describe('resolveNative — native module fallback order', () => {
  it('prefers the TurboModule (new arch) over the legacy bridge', () => {
    expect(resolveNative(turbo, bridge)).toBe(turbo);
  });

  it('falls back to the NativeModules bridge when no TurboModule', () => {
    expect(resolveNative(null, bridge)).toBe(bridge);
    expect(resolveNative(undefined, bridge)).toBe(bridge);
  });

  it('returns a Proxy that throws an actionable linking error when unlinked', () => {
    const resolved = resolveNative(null, null);
    // Construction must not throw — the error is deferred to first use.
    expect(() => (resolved as { renderDot: unknown }).renderDot).toThrow(/doesn't seem to be linked/);
    expect(() => (resolved as { getVersion: unknown }).getVersion).toThrow(/rebuilt the app/);
  });
});

describe('renderDot — wrapper delegation and defaults', () => {
  it('defaults engine to "dot" and format to "svg"', async () => {
    const out = await renderDot('digraph { a -> b }');
    expect(out).toBe('<svg data-engine="dot" data-format="svg">digraph { a -> b }</svg>');
    expect(renderCalls.at(-1)).toEqual({ dot: 'digraph { a -> b }', engine: 'dot', format: 'svg' });
  });

  it('passes an explicit engine and format straight through', async () => {
    await renderDot('graph { a -- b }', 'neato', 'json');
    expect(renderCalls.at(-1)).toEqual({ dot: 'graph { a -- b }', engine: 'neato', format: 'json' });
  });

  it('getVersion delegates to the resolved native getVersion', async () => {
    expect(await getVersion()).toBe('12.2.1');
  });
});

describe('GraphvizErrorCode', () => {
  it('exposes the full set of error codes as a frozen-shaped map', () => {
    expect(GraphvizErrorCode.INVALID_DOT).toBe('INVALID_DOT');
    expect(GraphvizErrorCode.OUT_OF_MEMORY).toBe('OUT_OF_MEMORY');
    expect(Object.keys(GraphvizErrorCode)).toContain('UNKNOWN');
  });
});
