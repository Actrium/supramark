import { LRUCache } from '@supramark/core';

/** Runtime cache policy resolved from the host's Supramark configuration. */
export interface RendererCachePolicy {
  enabled: boolean;
  maxSize: number;
  ttl?: number;
  /**
   * Optional total-byte cap. When set, the cache evicts oldest entries until
   * the summed `sizeCalculator` output fits, in addition to the entry-count
   * `maxSize` bound. `undefined` disables the byte bound.
   */
  maxBytes?: number;
}

/** Cache policy input accepted by existing Supramark diagram configuration. */
export interface RendererCachePolicyInput {
  enabled?: boolean;
  maxSize?: number;
  ttl?: number;
  maxBytes?: number;
}

// Default bounds apply only when a host explicitly enables caching without
// limits. maxSize caps the entry count; maxBytes bounds the resident byte
// footprint so a chat host scrolling hundreds of large SVGs cannot keep them
// alive for the process lifetime. Both are intentionally conservative — a host
// that wants more can raise either via diagram.defaultCache / engine cache.
const DEFAULT_CACHE_MAX_SIZE = 50;
const DEFAULT_CACHE_MAX_BYTES = 8_000_000;

/**
 * Estimates the byte footprint of one cached value for the byte-aware cap.
 * Returning a fixed 1 (as the pre-#124 code did) makes maxBytes a no-op, so
 * each namespace picks a cheap estimator proportional to real payload size.
 * This runs only on `set()` (one cache miss), never on `get()`.
 */
export type CacheSizeCalculator<T> = (value: T) => number;

/**
 * Keeps resolved values in the core LRU cache and shares equivalent in-flight work.
 * Rejected work is never retained, so a later mount can retry normally.
 */
export class AsyncRendererCache<T> {
  private readonly values: LRUCache<T>;
  private readonly pending = new Map<
    string,
    { promise: Promise<T>; retain: (value: T) => boolean }
  >();
  private readonly sizeCalculator: CacheSizeCalculator<T>;

  constructor(
    policy: RendererCachePolicy,
    sizeCalculator: CacheSizeCalculator<T>
  ) {
    this.sizeCalculator = sizeCalculator;
    this.values = new LRUCache<T>({
      maxSize: policy.maxSize,
      ttl: policy.ttl,
      maxBytes: policy.maxBytes,
      sizeCalculator: (value: unknown) => this.sizeCalculator(value as T),
    });
  }

  /** Returns a resolved cache value synchronously for first-render restoration. */
  get(key: string): T | undefined {
    return this.values.get(key);
  }

  /** Returns cache statistics for monitoring (size, bounds, total estimated bytes). */
  getStats(): ReturnType<LRUCache<T>['getStats']> {
    return this.values.getStats();
  }

  /**
   * Returns cached work or starts one shared asynchronous computation.
   *
   * `shouldRetain` is OR-joined across every caller that dedups onto the same
   * in-flight promise: if any caller wants the result retained, it is retained.
   * This fixes the pre-#124 behavior where the first caller's predicate
   * silently overrode every later joiner (a caller passing `() => true` still
   * got nothing cached when it joined a `() => false` in-flight row).
   */
  getOrCreate(
    key: string,
    factory: () => Promise<T>,
    shouldRetain: (value: T) => boolean = () => true
  ): Promise<T> {
    const cached = this.values.get(key);
    if (cached !== undefined) {
      return Promise.resolve(cached);
    }

    const existing = this.pending.get(key);
    if (existing) {
      const previousRetain = existing.retain;
      const incomingRetain = shouldRetain;
      existing.retain = value => previousRetain(value) || incomingRetain(value);
      return existing.promise;
    }

    const entry = {
      promise: undefined as unknown as Promise<T>,
      retain: shouldRetain,
    };
    const promise = factory()
      .then(value => {
        if (entry.retain(value)) {
          this.values.set(key, value);
        }
        return value;
      })
      .finally(() => {
        if (this.pending.get(key) === entry) {
          this.pending.delete(key);
        }
      });
    entry.promise = promise;
    this.pending.set(key, entry);
    return promise;
  }

  /**
   * Reconfigure bounds in place (used when a policy changes for the same
   * namespace, so the cache is not stranded — see #124). Pending work is
   * untouched; only completed entries are evicted to fit the new bounds.
   */
  reconfigure(policy: RendererCachePolicy): void {
    this.values.reconfigure({
      maxSize: policy.maxSize,
      ttl: policy.ttl,
      maxBytes: policy.maxBytes,
    });
  }
}

// Renderer-level caches are keyed by namespace ALONE. Keying by
// `namespace:maxSize:ttl` (pre-#124) stranded a whole AsyncRendererCache
// whenever the resolved policy changed — and `resolveDocumentCachePolicy`
// derives maxSize from the enabled engine subset, so a host enabling
// different engines per message produced a new key without changing code,
// leaking every stranded cache's entries + pending map forever.
//
// Keying by namespace and reconfiguring in place removes the leak: a policy
// change shrinks/grows the existing cache's bounds and evicts to fit. The
// sizeCalculator is fixed at first construction per namespace; a later
// policy change with a different byte footprint still uses the original
// estimator (it is proportional to payload, not to the bound).
const rendererCaches = new Map<string, AsyncRendererCache<unknown>>();

/** Resolves an engine override on top of the diagram-wide cache policy. */
export function resolveRendererCachePolicy(
  override: RendererCachePolicyInput | undefined,
  fallback: RendererCachePolicyInput | undefined
): RendererCachePolicy {
  const enabled = override?.enabled ?? fallback?.enabled ?? false;
  const configuredMaxSize = override?.maxSize ?? fallback?.maxSize ?? DEFAULT_CACHE_MAX_SIZE;
  const configuredTtl = override?.ttl ?? fallback?.ttl;
  const configuredMaxBytes = override?.maxBytes ?? fallback?.maxBytes ?? DEFAULT_CACHE_MAX_BYTES;

  return {
    enabled,
    maxSize: Number.isFinite(configuredMaxSize)
      ? Math.max(0, Math.floor(configuredMaxSize))
      : DEFAULT_CACHE_MAX_SIZE,
    ttl:
      configuredTtl !== undefined && Number.isFinite(configuredTtl) && configuredTtl > 0
        ? configuredTtl
        : undefined,
    maxBytes:
      configuredMaxBytes !== undefined &&
      Number.isFinite(configuredMaxBytes) &&
      configuredMaxBytes > 0
        ? configuredMaxBytes
        : DEFAULT_CACHE_MAX_BYTES,
  };
}

/** Resolves engine > diagram default > global cache precedence. */
export function resolveDiagramCachePolicy(
  enginePolicy: RendererCachePolicyInput | undefined,
  diagramPolicy: RendererCachePolicyInput | undefined,
  globalCache: boolean | undefined
): RendererCachePolicy {
  const diagramFallback = resolveRendererCachePolicy(
    diagramPolicy,
    globalCache === undefined ? undefined : { enabled: globalCache }
  );
  return resolveRendererCachePolicy(enginePolicy, diagramFallback);
}

/**
 * Returns the renderer-level cache for one namespace and resolved policy.
 *
 * The cache is keyed by namespace only; a policy change reconfigures the
 * existing cache in place rather than stranding it (see #124). The
 * `sizeCalculator` is applied only when the cache is first created for a
 * namespace; later calls for the same namespace reuse the existing cache
 * (and its original estimator), since per-namespace value shapes are stable.
 */
export function getRendererCache<T>(
  namespace: string,
  policy: RendererCachePolicy,
  sizeCalculator: CacheSizeCalculator<T> = () => 1
): AsyncRendererCache<T> | undefined {
  // maxSize:0 means "zero capacity", i.e. nothing can be retained, so caching is
  // disabled even when policy.enabled is true. Note this can surprise a host that
  // sets options.cache:true together with diagram.defaultCache.maxSize:0: the
  // explicit zero capacity wins and documents are not cached. The combination is
  // self-consistent (0 capacity stores nothing) but uncommon — if both signals
  // appear, raise the maxSize rather than relying on the global toggle here.
  if (!policy.enabled || policy.maxSize === 0) {
    return undefined;
  }

  const existing = rendererCaches.get(namespace);
  if (existing) {
    existing.reconfigure(policy);
    return existing as AsyncRendererCache<T>;
  }

  const cache = new AsyncRendererCache<T>(policy, sizeCalculator);
  rendererCaches.set(namespace, cache as AsyncRendererCache<unknown>);
  return cache;
}

/**
 * Deterministically serializes JSON-like render options for cache keys.
 *
 * Plain objects/arrays are serialized structurally with sorted keys. Values
 * with canonical content (Date, RegExp, Map, Set) are compared by VALUE so
 * two freshly-constructed but equal-valued options hit the same cache key
 * (pre-#124 they were identity-keyed and each miss evicted a good entry).
 * Other non-plain values (class instances, functions, symbols) cannot be
 * compared by content cheaply or safely, so each distinct instance gets a
 * stable process-local identity id via a WeakMap. A cyclic config object
 * short-circuits re-entrant visits to the identity id rather than overflowing
 * the stack.
 *
 * The visited-set is backtracked (add on enter, delete on leave) so the
 * serializer is O(depth) rather than the pre-#124 O(depth²) that allocated a
 * new Set per node.
 */
// Functions are objects (WeakMap-keyable). Symbols are primitives, so they
// cannot key a WeakMap on older TS lib targets — keep a separate Map for
// them. Config symbol values are rare, so the unbounded Map is acceptable.
const nonPlainIdentities = new WeakMap<object, number>();
const symbolIdentities = new Map<symbol, number>();
let nextNonPlainId = 1;

function isPlainObject(value: object): boolean {
  // Object.getPrototypeOf is typed `any` in the ES5 lib, so assert the return
  // to keep the type-aware lint (no-unsafe-assignment) happy.
  const proto = Object.getPrototypeOf(value) as object | null;
  return proto === null || proto === Object.prototype;
}

function nonPlainIdentity(value: object | symbol): string {
  if (typeof value === 'symbol') {
    const existing = symbolIdentities.get(value);
    if (existing !== undefined) {
      return `obj:${existing}`;
    }
    const next = nextNonPlainId++;
    symbolIdentities.set(value, next);
    return `obj:${next}`;
  }
  const existing = nonPlainIdentities.get(value);
  if (existing !== undefined) {
    return `obj:${existing}`;
  }
  const next = nextNonPlainId++;
  nonPlainIdentities.set(value, next);
  return `obj:${next}`;
}

export function stableSerialize(value: unknown, seen = new Set<object | symbol>()): string {
  if (value === null) {
    return 'null';
  }
  if (value === undefined) {
    return 'undefined';
  }
  if (typeof value === 'string') {
    return `string:${JSON.stringify(value)}`;
  }
  if (typeof value === 'number' || typeof value === 'boolean') {
    return `${typeof value}:${String(value)}`;
  }
  if (typeof value === 'function' || typeof value === 'symbol') {
    // Functions are objects (WeakMap-keyable); symbols are WeakMap-keyable in
    // modern engines. Two closures with identical source or two symbols with
    // the same description are distinct values, so identity (not source/desc)
    // avoids colliding them onto the same cache key.
    return nonPlainIdentity(value as object | symbol);
  }
  if (Array.isArray(value)) {
    if (seen.has(value)) {
      return nonPlainIdentity(value);
    }
    seen.add(value);
    const body = value.map(item => stableSerialize(item, seen)).join(',');
    seen.delete(value);
    return `[${body}]`;
  }
  if (typeof value === 'object') {
    if (seen.has(value)) {
      return nonPlainIdentity(value);
    }
    if (value instanceof Date) {
      // Equal-valued Dates (different instances) must hit the same key.
      return `date:${value.getTime()}`;
    }
    if (value instanceof RegExp) {
      return `regexp:${value.source}/${value.flags}`;
    }
    if (value instanceof Map) {
      seen.add(value);
      // Canonicalize by sorted serialized key so insertion order does not
      // split equal-valued maps. Entries with object keys recurse normally.
      const entries = [...value.entries()]
        .map(([k, v]) => `${stableSerialize(k, seen)}:${stableSerialize(v, seen)}`)
        .sort()
        .join(',');
      seen.delete(value);
      return `map:{${entries}}`;
    }
    if (value instanceof Set) {
      seen.add(value);
      const values = [...value]
        .map(v => stableSerialize(v, seen))
        .sort()
        .join(',');
      seen.delete(value);
      return `set:[${values}]`;
    }
    if (!isPlainObject(value)) {
      // Other class instances: identity, not content (cannot compare safely).
      return nonPlainIdentity(value);
    }
    seen.add(value);
    const stringKeys = Object.keys(value as Record<string, unknown>);
    const symbolKeys = Object.getOwnPropertySymbols(value);
    const entries = [
      ...stringKeys.map(k => [JSON.stringify(k), stableSerialize((value as Record<string, unknown>)[k], seen)] as const),
      ...symbolKeys.map(s => [nonPlainIdentity(s), stableSerialize((value as Record<symbol, unknown>)[s], seen)] as const),
    ]
      .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
      .map(([k, v]) => `${k}:${v}`)
      .join(',');
    seen.delete(value);
    return `{${entries}}`;
  }
  return `${typeof value}:${String(value)}`;
}

/**
 * Drops every renderer cache (parsed-document + diagram namespaces). Call
 * from a host memory-warning handler (e.g. RN `AppState` 'warning') to
 * release cached SVGs and ASTs. In-flight renders still resolve to their
 * callers but their results are discarded (the `.then` runs on the
 * orphaned cache, not retained into a new one), so the next mount
 * re-fetches. Safe to call when caching is disabled (no-op).
 */
export function clearSupramarkRenderCache(): void {
  rendererCaches.clear();
}

/** @internal Resets renderer caches for deterministic tests. */
export function clearReactNativeRendererCaches(): void {
  rendererCaches.clear();
}
