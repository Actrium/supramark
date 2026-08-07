import { LRUCache, createCacheKey, simpleHash } from '../src/cache';

describe('LRUCache', () => {
  describe('basic functionality', () => {
    it('can set and get a value', () => {
      const cache = new LRUCache<string>({ maxSize: 10 });
      cache.set('key1', 'value1');
      expect(cache.get('key1')).toBe('value1');
    });

    it('returns undefined when the key does not exist', () => {
      const cache = new LRUCache<string>({ maxSize: 10 });
      expect(cache.get('nonexistent')).toBeUndefined();
    });

    it('can check whether a key exists', () => {
      const cache = new LRUCache<string>({ maxSize: 10 });
      cache.set('key1', 'value1');
      expect(cache.has('key1')).toBe(true);
      expect(cache.has('key2')).toBe(false);
    });

    it('can delete a key', () => {
      const cache = new LRUCache<string>({ maxSize: 10 });
      cache.set('key1', 'value1');
      expect(cache.delete('key1')).toBe(true);
      expect(cache.get('key1')).toBeUndefined();
    });

    it('can clear the entire cache', () => {
      const cache = new LRUCache<string>({ maxSize: 10 });
      cache.set('key1', 'value1');
      cache.set('key2', 'value2');
      cache.clear();
      expect(cache.size).toBe(0);
      expect(cache.get('key1')).toBeUndefined();
    });
  });

  describe('LRU eviction policy', () => {
    it('evicts the oldest entry once capacity is exceeded', () => {
      const cache = new LRUCache<string>({ maxSize: 3 });
      cache.set('key1', 'value1');
      cache.set('key2', 'value2');
      cache.set('key3', 'value3');
      cache.set('key4', 'value4'); // should evict key1

      expect(cache.get('key1')).toBeUndefined();
      expect(cache.get('key2')).toBe('value2');
      expect(cache.get('key3')).toBe('value3');
      expect(cache.get('key4')).toBe('value4');
    });

    it('accessing an entry updates its position (LRU)', () => {
      const cache = new LRUCache<string>({ maxSize: 3 });
      cache.set('key1', 'value1');
      cache.set('key2', 'value2');
      cache.set('key3', 'value3');

      // Access key1, making it the most recently used
      cache.get('key1');

      // Add key4, which should evict key2 (the oldest unaccessed entry)
      cache.set('key4', 'value4');

      expect(cache.get('key1')).toBe('value1');
      expect(cache.get('key2')).toBeUndefined();
      expect(cache.get('key3')).toBe('value3');
      expect(cache.get('key4')).toBe('value4');
    });
  });

  describe('TTL expiration', () => {
    it('returns undefined once the TTL has expired', async () => {
      const cache = new LRUCache<string>({ maxSize: 10, ttl: 50 }); // 50ms TTL
      cache.set('key1', 'value1');

      expect(cache.get('key1')).toBe('value1');

      // Wait for the TTL to expire
      await new Promise(resolve => setTimeout(resolve, 100));

      expect(cache.get('key1')).toBeUndefined();
    });

    it('can purge expired entries', async () => {
      const cache = new LRUCache<string>({ maxSize: 10, ttl: 50 });
      cache.set('key1', 'value1');
      cache.set('key2', 'value2');

      await new Promise(resolve => setTimeout(resolve, 100));

      const purged = cache.purgeExpired();
      expect(purged).toBe(2);
      expect(cache.size).toBe(0);
    });
  });

  describe('statistics', () => {
    it('returns the correct statistics', () => {
      const cache = new LRUCache<string>({ maxSize: 10, ttl: 1000 });
      cache.set('key1', 'value1');
      cache.set('key2', 'value2');

      const stats = cache.getStats();
      expect(stats.size).toBe(2);
      expect(stats.maxSize).toBe(10);
      expect(stats.ttl).toBe(1000);
      expect(stats.totalSize).toBeGreaterThan(0);
    });
  });

  describe('byte-aware eviction (#124)', () => {
    it('evicts oldest entries once totalSize exceeds maxBytes', () => {
      const cache = new LRUCache<string>({
        maxSize: 100,
        maxBytes: 10,
        sizeCalculator: value => value.length,
      });
      cache.set('a', 'aaaa'); // 4 bytes, total 4
      cache.set('b', 'aaaa'); // 4 bytes, total 8
      expect(cache.get('a')).toBe('aaaa'); // touch a so b is older
      cache.set('c', 'aaaa'); // 4 bytes, total 12 > 10 -> evict b
      expect(cache.get('a')).toBe('aaaa');
      expect(cache.get('b')).toBeUndefined();
      expect(cache.get('c')).toBe('aaaa');
    });

    it('retains a single entry larger than maxBytes rather than dropping it', () => {
      const cache = new LRUCache<string>({
        maxSize: 100,
        maxBytes: 5,
        sizeCalculator: value => value.length,
      });
      cache.set('big', 'aaaaaaaaaa'); // 10 bytes > 5 cap, but cannot evict itself
      expect(cache.get('big')).toBe('aaaaaaaaaa');
      expect(cache.size).toBe(1);
    });

    it('maxBytes undefined disables the byte bound (only entry count applies)', () => {
      const cache = new LRUCache<string>({
        maxSize: 2,
        sizeCalculator: value => value.length,
      });
      cache.set('a', 'aaaa');
      cache.set('b', 'aaaa');
      cache.set('c', 'aaaa'); // entry-count eviction: evicts a
      expect(cache.get('a')).toBeUndefined();
      expect(cache.size).toBe(2);
    });
  });

  describe('reconfigure (#124)', () => {
    it('shrinks bounds in place and evicts to fit', () => {
      const cache = new LRUCache<string>({ maxSize: 100, sizeCalculator: value => value.length });
      cache.set('a', 'aaaa');
      cache.set('b', 'aaaa');
      cache.set('c', 'aaaa');
      cache.reconfigure({ maxSize: 1 });
      expect(cache.getStats().maxSize).toBe(1);
      expect(cache.size).toBe(1);
      expect(cache.get('a')).toBeUndefined();
      expect(cache.get('c')).toBe('aaaa'); // most recent survives
    });

    it('updates maxBytes and evicts to fit', () => {
      const cache = new LRUCache<string>({
        maxSize: 100,
        sizeCalculator: value => value.length,
      });
      cache.set('a', 'aaaa');
      cache.set('b', 'aaaa');
      cache.set('c', 'aaaa');
      cache.reconfigure({ maxBytes: 5 });
      expect(cache.getStats().maxBytes).toBe(5);
      // 5-byte cap keeps at most one 4-byte entry
      expect(cache.size).toBeLessThanOrEqual(2);
      expect(cache.get('c')).toBe('aaaa');
    });

    it('clearing ttl via reconfigure sets ttl to undefined', () => {
      const cache = new LRUCache<string>({ maxSize: 10, ttl: 1000 });
      cache.reconfigure({ ttl: undefined });
      expect(cache.getStats().ttl).toBeUndefined();
    });
  });
});

describe('createCacheKey', () => {
  it('creates the correct cache key', () => {
    expect(createCacheKey('mermaid', 'abc123')).toBe('mermaid:abc123');
    expect(createCacheKey('math', 'formula', 'inline')).toBe('math:formula:inline');
  });

  it('filters out null and undefined values', () => {
    expect(createCacheKey('mermaid', null, 'abc123')).toBe('mermaid:abc123');
    expect(createCacheKey('mermaid', undefined, 'abc123')).toBe('mermaid:abc123');
  });
});

describe('simpleHash', () => {
  it('generates the same hash for the same string', () => {
    const hash1 = simpleHash('test string');
    const hash2 = simpleHash('test string');
    expect(hash1).toBe(hash2);
  });

  it('generates different hashes for different strings', () => {
    const hash1 = simpleHash('test string 1');
    const hash2 = simpleHash('test string 2');
    expect(hash1).not.toBe(hash2);
  });

  it('returns a hash of type string', () => {
    const hash = simpleHash('test');
    expect(typeof hash).toBe('string');
  });
});
