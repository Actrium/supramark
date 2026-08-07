import { beforeEach, describe, expect, it } from 'bun:test';

import type { DiagramRenderResult } from '@supramark/engines';
import {
  clearWebRendererCaches,
  getRendererCache,
  resolveDiagramCachePolicy,
  resolveRendererCachePolicy,
  stableSerialize,
} from '../src/renderCache';
import type { AsyncRendererCache } from '../src/renderCache';

/**
 * Tests for the web renderer cache. The AsyncRendererCache / policy resolvers
 * are a direct port of the RN renderer's cache (issue #163), so these tests
 * mirror packages/renderers/rn/__tests__/renderCache.test.ts.
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

  it('a function value uses identity, not source text', () => {
    const f1 = () => 1;
    const f2 = () => 1;
    expect(stableSerialize({ fn: f1 })).not.toBe(stableSerialize({ fn: f2 }));
    expect(stableSerialize({ fn: f1 })).toBe(stableSerialize({ fn: f1 }));
  });
});

describe('resolveRendererCachePolicy', () => {
  it('defaults to disabled when neither override nor fallback enable it', () => {
    expect(resolveRendererCachePolicy(undefined, undefined).enabled).toBe(false);
  });

  it('falls back to global cache when no engine policy is set', () => {
    expect(resolveRendererCachePolicy(undefined, { enabled: true }).enabled).toBe(true);
  });

  it('engine override wins over fallback', () => {
    const policy = resolveRendererCachePolicy({ enabled: false, maxSize: 5 }, { enabled: true });
    expect(policy.enabled).toBe(false);
    expect(policy.maxSize).toBe(5);
  });

  it('clamps a non-finite maxSize to the default', () => {
    const policy = resolveRendererCachePolicy(
      { enabled: true, maxSize: Number.POSITIVE_INFINITY },
      undefined
    );
    expect(policy.maxSize).toBe(100);
  });

  it('drops a non-positive ttl', () => {
    expect(resolveRendererCachePolicy({ enabled: true, ttl: 0 }).ttl).toBeUndefined();
    expect(resolveRendererCachePolicy({ enabled: true, ttl: -5 }).ttl).toBeUndefined();
  });
});

describe('resolveDiagramCachePolicy', () => {
  it('resolves engine > diagram default > global precedence', () => {
    const policy = resolveDiagramCachePolicy(
      { enabled: true, maxSize: 7 },
      { enabled: true, maxSize: 50 },
      false
    );
    expect(policy.enabled).toBe(true);
    expect(policy.maxSize).toBe(7);
  });

  it('global cache enables when no explicit policy is set', () => {
    const policy = resolveDiagramCachePolicy(undefined, undefined, true);
    expect(policy.enabled).toBe(true);
  });

  it('explicitly disabled engine cache wins even when global is true', () => {
    const policy = resolveDiagramCachePolicy({ enabled: false }, undefined, true);
    expect(policy.enabled).toBe(false);
  });
});

describe('getRendererCache', () => {
  beforeEach(() => {
    clearWebRendererCaches();
  });

  it('returns undefined when caching is disabled', () => {
    expect(getRendererCache<DiagramRenderResult>('diagram', resolveRendererCachePolicy(undefined, undefined))).toBeUndefined();
  });

  it('returns a shared cache for an equivalent policy key', () => {
    const policy = resolveRendererCachePolicy({ enabled: true, maxSize: 10 }, undefined);
    const a = getRendererCache<DiagramRenderResult>('diagram', policy);
    const b = getRendererCache<DiagramRenderResult>('diagram', policy);
    expect(a).toBe(b);
  });

  it('returns a distinct cache when maxSize differs', () => {
    const p1 = getRendererCache<DiagramRenderResult>('diagram', resolveRendererCachePolicy({ enabled: true, maxSize: 10 }, undefined));
    const p2 = getRendererCache<DiagramRenderResult>('diagram', resolveRendererCachePolicy({ enabled: true, maxSize: 20 }, undefined));
    expect(p1).not.toBe(p2);
  });
});

describe('AsyncRendererCache', () => {
  beforeEach(() => {
    clearWebRendererCaches();
  });

  function makeCache(): AsyncRendererCache<DiagramRenderResult> {
    const policy = resolveRendererCachePolicy({ enabled: true, maxSize: 10 }, undefined);
    return getRendererCache<DiagramRenderResult>('diagram', policy)!;
  }

  it('shares in-flight work for the same key', async () => {
    const cache = makeCache();
    let calls = 0;
    const factory = () => {
      calls += 1;
      return new Promise<DiagramRenderResult>(resolve => {
        setTimeout(() => {
          calls += 0;
          resolve(successResult());
        }, 0);
      });
    };
    const [a, b] = await Promise.all([
      cache.getOrCreate('k', factory, r => r.success),
      cache.getOrCreate('k', factory, r => r.success),
    ]);
    expect(calls).toBe(1);
    expect(a).toBe(b);
  });

  it('serves a completed value on a later getOrCreate', async () => {
    const cache = makeCache();
    let calls = 0;
    const factory = () => {
      calls += 1;
      return Promise.resolve(successResult());
    };
    await cache.getOrCreate('k', factory, r => r.success);
    const second = await cache.getOrCreate('k', factory, r => r.success);
    expect(calls).toBe(1);
    expect(second.success).toBe(true);
  });

  it('does not retain an unsuccessful result so the next call retries', async () => {
    const cache = makeCache();
    let calls = 0;
    const factory = () => {
      calls += 1;
      return Promise.resolve(errorResult());
    };
    await cache.getOrCreate('k', factory, r => r.success);
    await cache.getOrCreate('k', factory, r => r.success);
    expect(calls).toBe(2);
  });

  it('returns the cached value synchronously from get', async () => {
    const cache = makeCache();
    await cache.getOrCreate('k', () => Promise.resolve(successResult()), r => r.success);
    expect(cache.get('k')?.success).toBe(true);
  });
});

function successResult(): DiagramRenderResult {
  return {
    id: 'test',
    engine: 'mermaid',
    success: true,
    format: 'svg',
    payload: '<svg></svg>',
  };
}

function errorResult(): DiagramRenderResult {
  return {
    id: 'test',
    engine: 'mermaid',
    success: false,
    format: 'error',
    payload: 'boom',
    error: { code: 'render_error', message: 'boom' },
  };
}
