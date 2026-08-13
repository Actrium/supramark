import { describe, expect, it } from 'bun:test';

import './support/mock-react-native';
import {
  stableSerialize,
  AsyncRendererCache,
  clearReactNativeRendererCaches,
  clearSupramarkRenderCache,
  getRendererCache,
  resolveRendererCachePolicy,
  type RendererCachePolicy,
} from '../src/renderCache';

/**
 * Tests for stableSerialize's cache-key behavior.
 *
 * Contract: different option values must produce different keys, otherwise
 * the wrong cached SVG could be returned; a circular reference must not
 * overflow the stack (the useMemo in the render path must not throw a
 * RangeError). Regression coverage for the issue (raised in review) where
 * Date/Map/Set all serialized to {} and collided on the same key.
 */
describe('stableSerialize', () => {
  it('distinguishes options containing different Date values (no longer collide on the same key)', () => {
    const a = { since: new Date('2026-01-01'), label: 'x' };
    const b = { since: new Date('2020-06-15'), label: 'x' };
    expect(stableSerialize(a)).not.toBe(stableSerialize(b));
  });

  it('distinguishes options containing different Map values', () => {
    const a = { tags: new Map([['k', 'v1']]) };
    const b = { tags: new Map([['k', 'v2']]) };
    expect(stableSerialize(a)).not.toBe(stableSerialize(b));
  });

  it('distinguishes options containing different Set values', () => {
    const a = { set: new Set([1, 2, 3]) };
    const b = { set: new Set([4, 5, 6]) };
    expect(stableSerialize(a)).not.toBe(stableSerialize(b));
  });

  it('serializing the same non-plain instance multiple times yields the same key (identity-stable)', () => {
    const date = new Date('2026-01-01');
    expect(stableSerialize({ d: date })).toBe(stableSerialize({ d: date }));
  });

  it('a circular reference does not throw a RangeError', () => {
    const cyclic: Record<string, unknown> = { a: 1 };
    cyclic.self = cyclic;
    expect(() => stableSerialize(cyclic)).not.toThrow();
  });

  it('a plain object is still serialized by structure + sorted keys', () => {
    expect(stableSerialize({ b: 2, a: 1 })).toBe(stableSerialize({ a: 1, b: 2 }));
    expect(stableSerialize({ a: 1 })).not.toBe(stableSerialize({ a: 2 }));
  });

  it('number and string do not collide on the same key', () => {
    expect(stableSerialize(1)).not.toBe(stableSerialize('1'));
  });

  it('null and undefined are each independent', () => {
    expect(stableSerialize(null)).toBe('null');
    expect(stableSerialize(undefined)).toBe('undefined');
    expect(stableSerialize(null)).not.toBe(stableSerialize(undefined));
  });

  it('nested arrays are serialized by structure', () => {
    expect(stableSerialize([1, [2, 3]])).toBe(stableSerialize([1, [2, 3]]));
    expect(stableSerialize([1, [2, 3]])).not.toBe(stableSerialize([1, [2, 4]]));
  });

  // #124 nits: equal-valued non-plain values must hit the SAME key (value, not
  // identity) so freshly-constructed but equal options reuse cached work.
  it('equal-valued Dates (distinct instances) hit the same key', () => {
    const a = { since: new Date('2026-01-01') };
    const b = { since: new Date('2026-01-01') };
    expect(stableSerialize(a)).toBe(stableSerialize(b));
  });

  it('equal-valued Maps (distinct instances, same entries) hit the same key', () => {
    const a = { tags: new Map([['k', 'v']]) };
    const b = { tags: new Map([['k', 'v']]) };
    expect(stableSerialize(a)).toBe(stableSerialize(b));
  });

  it('equal-valued Sets (distinct instances) hit the same key', () => {
    const a = { ids: new Set([1, 2, 3]) };
    const b = { ids: new Set([3, 2, 1]) }; // different insertion order
    expect(stableSerialize(a)).toBe(stableSerialize(b));
  });

  it('equal RegExps (distinct instances) hit the same key', () => {
    const a = { pat: /foo/gi };
    const b = { pat: /foo/gi };
    expect(stableSerialize(a)).toBe(stableSerialize(b));
    expect(stableSerialize({ pat: /foo/gi })).not.toBe(stableSerialize({ pat: /foo/g }));
  });

  // Two functions/symbols with identical source/description must NOT collide.
  it('distinct functions with identical source do not collide', () => {
    const a = { fn: () => 1 };
    const b = { fn: () => 1 };
    expect(stableSerialize(a)).not.toBe(stableSerialize(b));
  });

  it('distinct symbols with the same description do not collide', () => {
    const sym = Symbol('theme');
    const a: Record<symbol, unknown> = { [sym]: 1 };
    const b: Record<symbol, unknown> = { [Symbol('theme')]: 1 };
    expect(stableSerialize(a)).not.toBe(stableSerialize(b));
    // Same symbol identity does hit the same key (and the prop is not dropped).
    const c: Record<symbol, unknown> = { [sym]: 1 };
    expect(stableSerialize(a)).toBe(stableSerialize(c));
  });

  it('symbol-keyed properties are not dropped', () => {
    const sym = Symbol('hidden');
    const a: Record<symbol, unknown> = { [sym]: 'x' };
    const b: Record<symbol, unknown> = { [sym]: 'y' };
    expect(stableSerialize(a)).not.toBe(stableSerialize(b));
  });

  // #124 nit: O(depth) backtracking — a 2000-level chain must be fast, not O(depth²).
  it('a deep chain serializes without quadratic blowup', () => {
    let node: unknown = 'leaf';
    for (let i = 0; i < 2000; i++) {
      node = { next: node };
    }
    const start = Date.now();
    stableSerialize(node);
    const elapsed = Date.now() - start;
    // Generous bound; the pre-#124 quadratic version took 400ms+ on a 2000-chain.
    expect(elapsed).toBeLessThan(200);
  });
});

describe('AsyncRendererCache', () => {
  const basePolicy: RendererCachePolicy = {
    enabled: true,
    maxSize: 10,
    ttl: undefined,
    maxBytes: undefined,
  };

  it('OR-joins shouldRetain across deduped in-flight callers (#124 #4)', async () => {
    const cache = new AsyncRendererCache<string>(basePolicy, () => 1);
    let resolveFactory: (value: string) => void = () => undefined;
    const factory = () =>
      new Promise<string>(resolve => {
        resolveFactory = resolve;
      });

    // Caller 1 does NOT want to retain; caller 2 (joining in-flight) does.
    const p1 = cache.getOrCreate('shared', factory, () => false);
    const p2 = cache.getOrCreate('shared', factory, () => true);

    resolveFactory('result');
    await Promise.all([p1, p2]);

    // Because caller 2 wanted retention, the value must be cached.
    expect(cache.get('shared')).toBe('result');
  });

  it('does not retain when no joiner wants retention', async () => {
    const cache = new AsyncRendererCache<string>(basePolicy, () => 1);
    let resolveFactory: (value: string) => void = () => undefined;
    const factory = () =>
      new Promise<string>(resolve => {
        resolveFactory = resolve;
      });

    const p1 = cache.getOrCreate('shared', factory, () => false);
    const p2 = cache.getOrCreate('shared', factory, () => false);
    resolveFactory('result');
    await Promise.all([p1, p2]);

    expect(cache.get('shared')).toBeUndefined();
  });

  it('does not retain rejected work (#91)', async () => {
    const cache = new AsyncRendererCache<string>(basePolicy, () => 1);
    const factory = () => Promise.reject(new Error('boom'));

    await expect(cache.getOrCreate('shared', factory)).rejects.toThrow('boom');
    expect(cache.get('shared')).toBeUndefined();
  });
});

describe('getRendererCache policy reconfiguration (#124 #3)', () => {
  it('keys by namespace alone — a policy change reconfigures, not strands', () => {
    clearReactNativeRendererCaches();
    const policyA = resolveRendererCachePolicy({ enabled: true, maxSize: 5 });
    const policyB = resolveRendererCachePolicy({ enabled: true, maxSize: 20 });

    const cacheA = getRendererCache<string>('diagram', policyA, () => 1);
    const cacheB = getRendererCache<string>('diagram', policyB, () => 1);

    expect(cacheA).toBe(cacheB); // same instance, reconfigured in place
    expect(cacheA!.getStats().maxSize).toBe(20); // reconfigured to policy B
  });

  it('reconfiguring to a smaller maxSize evicts oldest entries', async () => {
    clearReactNativeRendererCaches();
    const policy = resolveRendererCachePolicy({ enabled: true, maxSize: 10 });
    const cache = getRendererCache<string>('diagram', policy, () => 1)!;

    // Fill via getOrCreate with synchronous-ish factories.
    for (let i = 0; i < 5; i++) {
      await cache.getOrCreate(`k${i}`, async () => `v${i}`);
    }
    expect(cache.getStats().size).toBe(5);

    cache.reconfigure(resolveRendererCachePolicy({ enabled: true, maxSize: 2 }));
    expect(cache.getStats().maxSize).toBe(2);
    expect(cache.getStats().size).toBe(2); // evicted to fit
  });

  it('distinct namespaces keep distinct caches', () => {
    clearReactNativeRendererCaches();
    const policy = resolveRendererCachePolicy({ enabled: true, maxSize: 5 });
    const diagram = getRendererCache<string>('diagram', policy, () => 1);
    const document = getRendererCache<string>('parsed-document', policy, () => 1);
    expect(diagram).not.toBe(document);
  });

  it('keeps different diagram-engine policies isolated', async () => {
    clearReactNativeRendererCaches();
    const mermaid = getRendererCache<string>(
      'diagram:mermaid',
      resolveRendererCachePolicy({ enabled: true, maxSize: 5 }),
      () => 1
    )!;
    await mermaid.getOrCreate('mermaid-svg', async () => '<svg>mermaid</svg>');

    const dot = getRendererCache<string>(
      'diagram:dot',
      resolveRendererCachePolicy({ enabled: true, maxSize: 1 }),
      () => 1
    )!;
    await dot.getOrCreate('dot-svg', async () => '<svg>dot</svg>');

    expect(dot).not.toBe(mermaid);
    expect(dot.getStats().maxSize).toBe(1);
    expect(mermaid.getStats().maxSize).toBe(5);
    expect(mermaid.get('mermaid-svg')).toBe('<svg>mermaid</svg>');
  });

  it('clearSupramarkRenderCache drops all namespaces (public API alias)', () => {
    clearReactNativeRendererCaches();
    const policy = resolveRendererCachePolicy({ enabled: true, maxSize: 5 });
    const before = getRendererCache<string>('diagram', policy, () => 1);
    expect(before).toBeDefined();
    clearSupramarkRenderCache();
    const after = getRendererCache<string>('diagram', policy, () => 1);
    expect(after).not.toBe(before); // new instance after clear
  });
});

describe('resolveRendererCachePolicy maxBytes resolution (#124 follow-up)', () => {
  // Pre-fix, an explicit maxBytes: 0 fell through to the 8 MB default
  // (indistinguishable from "unset"), so a host could not opt out of the byte
  // cap. Now 0 (or any non-positive / non-finite value) means "no byte bound",
  // matching ttl and LRUCache.reconfigure.
  it('treats an explicit maxBytes: 0 as "no byte cap"', () => {
    expect(resolveRendererCachePolicy({ enabled: true, maxBytes: 0 }).maxBytes).toBeUndefined();
  });

  it('treats a negative / non-finite maxBytes as "no byte cap"', () => {
    expect(resolveRendererCachePolicy({ enabled: true, maxBytes: -1 }).maxBytes).toBeUndefined();
    expect(resolveRendererCachePolicy({ enabled: true, maxBytes: Infinity }).maxBytes).toBeUndefined();
    expect(resolveRendererCachePolicy({ enabled: true, maxBytes: Number.NaN }).maxBytes).toBeUndefined();
  });

  it('keeps an explicit positive maxBytes and applies the default when it is unset', () => {
    expect(resolveRendererCachePolicy({ enabled: true, maxBytes: 1024 }).maxBytes).toBe(1024);
    // override wins over a fallback that does set it
    expect(
      resolveRendererCachePolicy({ enabled: true, maxBytes: 0 }, { maxBytes: 2048 }).maxBytes
    ).toBeUndefined();
    // unset → default
    expect(resolveRendererCachePolicy({ enabled: true }).maxBytes).toBe(8_000_000);
  });
});
