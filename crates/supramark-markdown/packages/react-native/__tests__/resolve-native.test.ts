import { describe, expect, it, mock } from 'bun:test';

// `resolveNative` is a pure selection function, so we can exercise all
// three fallback branches (turbo / bridge / unlinked) with plain fakes —
// no reliance on import-time module resolution, which the bun test runner
// caches per-process and cannot re-evaluate per file.
mock.module('react-native', () => ({
  NativeModules: {},
  Platform: {
    select: (options: Record<string, string | undefined>) => options.default ?? '',
  },
  TurboModuleRegistry: {
    getEnforcing: () => undefined,
  },
}));

const { resolveNative } = await import('../src/index');

const turbo = {
  parseJson: async () => JSON.stringify({ via: 'turbo' }),
  getVersion: async () => 'turbo',
};
const bridge = {
  parseJson: async () => JSON.stringify({ via: 'bridge' }),
  getVersion: async () => 'bridge',
};

describe('resolveNative — native module fallback order', () => {
  it('prefers the TurboModule (new arch) over the legacy bridge', () => {
    const resolved = resolveNative(turbo, bridge);
    expect(resolved).toBe(turbo);
  });

  it('falls back to the NativeModules bridge when no TurboModule', () => {
    expect(resolveNative(null, bridge)).toBe(bridge);
    expect(resolveNative(undefined, bridge)).toBe(bridge);
  });

  it('returns a Proxy that throws an actionable linking error when unlinked', () => {
    const resolved = resolveNative(null, null);
    // Construction itself must not throw — the error is deferred to use.
    expect(() => (resolved as { parseJson: unknown }).parseJson).toThrow(
      /doesn't seem to be linked/
    );
    expect(() => (resolved as { getVersion: unknown }).getVersion).toThrow(/rebuilt the app/);
  });
});
