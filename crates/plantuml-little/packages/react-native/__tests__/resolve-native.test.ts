import { describe, expect, it, mock } from 'bun:test';

// Capture what importing the package registers with the engine registry.
const registered: Array<{ engine: string; render: (code: string) => Promise<string> }> = [];
mock.module('@supramark/engines/rn', () => ({
  registerNativeEngineAdapter: (adapter: { engine: string; render: (c: string) => Promise<string> }) =>
    registered.push(adapter),
}));

// Fake bridge native module so import-time resolution yields a usable native.
const bridgeCalls: string[] = [];
const bridgeNative = {
  render: async (source: string) => {
    bridgeCalls.push(`render:${source}`);
    return `<svg>${source}</svg>`;
  },
  getVersion: async () => 'plantuml-bridge-1.0',
};
mock.module('react-native', () => ({
  NativeModules: { SupramarkPlantumlNative: bridgeNative },
  Platform: {
    select: (options: Record<string, string | undefined>) => options.default ?? '',
  },
  TurboModuleRegistry: { getEnforcing: () => undefined },
}));

const { resolveNative, getNativeVersion } = await import('../src/index');

const turbo = { render: async () => 'turbo', getVersion: async () => 'turbo' };
const bridge = { render: async () => 'bridge', getVersion: async () => 'bridge' };

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
    expect(() => (resolved as { render: unknown }).render).toThrow(/doesn't seem to be linked/);
    expect(() => (resolved as { getVersion: unknown }).getVersion).toThrow(/rebuilt the app/);
  });
});

describe('engine registration + delegation', () => {
  it('registers exactly one adapter for the "plantuml" engine', () => {
    expect(registered).toHaveLength(1);
    expect(registered[0].engine).toBe('plantuml');
    expect(typeof registered[0].render).toBe('function');
  });

  it('the registered adapter.render delegates to the resolved native render', async () => {
    const out = await registered[0].render('@startuml\nA->B\n@enduml');
    expect(out).toBe('<svg>@startuml\nA->B\n@enduml</svg>');
    expect(bridgeCalls).toContain('render:@startuml\nA->B\n@enduml');
  });

  it('getNativeVersion delegates to the resolved native getVersion', async () => {
    expect(await getNativeVersion()).toBe('plantuml-bridge-1.0');
  });
});
