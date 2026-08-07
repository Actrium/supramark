import { LRUCache } from '@supramark/core';

/** Runtime cache policy resolved from the host's Supramark configuration. */
export interface RendererCachePolicy {
  enabled: boolean;
  maxSize: number;
  ttl?: number;
}

/** Cache policy input accepted by existing Supramark diagram configuration. */
export interface RendererCachePolicyInput {
  enabled?: boolean;
  maxSize?: number;
  ttl?: number;
}

// Default bounds apply only when a host explicitly enables caching without limits.
const DEFAULT_CACHE_MAX_SIZE = 100;

/**
 * Keeps resolved values in the core LRU cache and shares equivalent in-flight work.
 * Rejected work is never retained, so a later mount can retry normally.
 */
export class AsyncRendererCache<T> {
  private readonly values: LRUCache<T>;
  private readonly pending = new Map<string, Promise<T>>();

  constructor(policy: RendererCachePolicy) {
    this.values = new LRUCache<T>({
      maxSize: policy.maxSize,
      ttl: policy.ttl,
      // Eviction is entry-count based (LRUCache compares cache.size to maxSize),
      // so each entry contributes a fixed unit. Override the default
      // sizeCalculator — which JSON.stringifies the whole value on every set()
      // (entire AST / SVG string) to feed a totalSize stat that never affects
      // eviction — to avoid that O(payload) work in the render path.
      sizeCalculator: () => 1,
    });
  }

  /** Returns a resolved cache value synchronously for first-render restoration. */
  get(key: string): T | undefined {
    return this.values.get(key);
  }

  /** Returns cached work or starts one shared asynchronous computation. */
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
      return existing;
    }

    const promise = factory()
      .then(value => {
        if (shouldRetain(value)) {
          this.values.set(key, value);
        }
        return value;
      })
      .finally(() => {
        if (this.pending.get(key) === promise) {
          this.pending.delete(key);
        }
      });

    this.pending.set(key, promise);
    return promise;
  }
}

// Renderer-level caches are keyed by namespace and policy rather than config identity.
// This lets equivalent inline config objects share completed and in-flight work while
// each AsyncRendererCache remains bounded by its resolved LRU policy.
//
// Lifecycle note: the key encodes `namespace:maxSize:ttl`, so if a host changes cache
// policy at runtime (e.g. maxSize 10 -> 20) a NEW AsyncRendererCache is created under
// the new key and the old one stays in this Map. This is intentional: we cannot safely
// tell whether the old cache is still referenced by an in-flight render. The leak is
// bounded — each stranded cache holds at most `maxSize` entries plus a transient
// `pending` map — and changing policy at runtime is rare (policy usually comes from a
// static host config for the whole app lifetime), so no proactive sweep is wired.
const rendererCaches = new Map<string, AsyncRendererCache<unknown>>();

/** Resolves an engine override on top of the diagram-wide cache policy. */
export function resolveRendererCachePolicy(
  override: RendererCachePolicyInput | undefined,
  fallback: RendererCachePolicyInput | undefined
): RendererCachePolicy {
  const enabled = override?.enabled ?? fallback?.enabled ?? false;
  const configuredMaxSize = override?.maxSize ?? fallback?.maxSize ?? DEFAULT_CACHE_MAX_SIZE;
  const configuredTtl = override?.ttl ?? fallback?.ttl;

  return {
    enabled,
    maxSize: Number.isFinite(configuredMaxSize)
      ? Math.max(0, Math.floor(configuredMaxSize))
      : DEFAULT_CACHE_MAX_SIZE,
    ttl:
      configuredTtl !== undefined && Number.isFinite(configuredTtl) && configuredTtl > 0
        ? configuredTtl
        : undefined,
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

/** Returns the renderer-level cache for one namespace and resolved policy. */
export function getRendererCache<T>(
  namespace: string,
  policy: RendererCachePolicy
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

  const policyKey = `${namespace}:${policy.maxSize}:${policy.ttl ?? 'none'}`;
  let cache = rendererCaches.get(policyKey);
  if (!cache) {
    cache = new AsyncRendererCache<unknown>(policy);
    rendererCaches.set(policyKey, cache);
  }

  return cache as AsyncRendererCache<T>;
}

/**
 * Deterministically serializes JSON-like render options for cache keys.
 *
 * Plain objects/arrays are serialized structurally; non-plain values
 * (Date / Map / Set / RegExp / class instances / functions) cannot be compared
 * by content cheaply or safely, so each distinct instance gets a stable
 * process-local identity id via a WeakMap. This keeps two options objects that
 * differ only by Date/Map/Set value from colliding on the same cache key (which
 * would serve a wrong cached SVG), and a cyclic config object from overflowing
 * the stack (the seen-set short-circuits re-entrant cycles to the identity id).
 */
const nonPlainIdentities = new WeakMap<object, number>();
let nextNonPlainId = 1;

function isPlainObject(value: object): boolean {
  // Object.getPrototypeOf is typed `any` in the ES5 lib, so assert the return
  // to keep the type-aware lint (no-unsafe-assignment) happy.
  const proto = Object.getPrototypeOf(value) as object | null;
  return proto === null || proto === Object.prototype;
}

function nonPlainIdentity(value: object): string {
  const existing = nonPlainIdentities.get(value);
  if (existing !== undefined) {
    return `obj:${existing}`;
  }
  const next = nextNonPlainId++;
  nonPlainIdentities.set(value, next);
  return `obj:${next}`;
}

export function stableSerialize(value: unknown, seen = new Set<object>()): string {
  if (value === null) {
    return 'null';
  }
  if (value === undefined) {
    return 'undefined';
  }
  if (Array.isArray(value)) {
    if (seen.has(value)) {
      return nonPlainIdentity(value);
    }
    const nextSeen = new Set(seen);
    nextSeen.add(value);
    return `[${value.map(item => stableSerialize(item, nextSeen)).join(',')}]`;
  }
  if (typeof value === 'object') {
    if (seen.has(value)) {
      return nonPlainIdentity(value);
    }
    if (!isPlainObject(value)) {
      // Date / Map / Set / RegExp / class instances: identity, not content.
      return nonPlainIdentity(value);
    }
    const nextSeen = new Set(seen);
    nextSeen.add(value);
    return `{${Object.entries(value as Record<string, unknown>)
      .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
      .map(([key, entryValue]) => `${JSON.stringify(key)}:${stableSerialize(entryValue, nextSeen)}`)
      .join(',')}}`;
  }
  if (typeof value === 'function') {
    // Source text isn't a stable key (engines may reorder params, truncate
    // bodies); treat functions like non-plain objects — identity-stable.
    return nonPlainIdentity(value);
  }
  if (typeof value === 'string') {
    return `string:${JSON.stringify(value)}`;
  }
  return `${typeof value}:${String(value)}`;
}

/** @internal Resets renderer caches for deterministic tests. */
export function clearWebRendererCaches(): void {
  rendererCaches.clear();
}
