/**
 * LRU (Least Recently Used) 缓存实现
 * 用于缓存图表渲染结果等计算密集型操作的结果
 */
export interface LRUCacheOptions {
    /**
     * 缓存最大容量（条目数量）
     * @default 100
     */
    maxSize?: number;
    /**
     * 缓存项的 TTL（生存时间，毫秒）
     * @default undefined（永不过期）
     */
    ttl?: number;
    /**
     * 可选的值序列化函数（用于估算内存大小）
     * @default (value) => JSON.stringify(value).length
     */
    sizeCalculator?: (value: unknown) => number;
}
/**
 * LRU 缓存类
 *
 * @example
 * ```typescript
 * const cache = new LRUCache<string>({ maxSize: 100, ttl: 60000 });
 *
 * // 存入缓存
 * cache.set('key1', 'value1');
 *
 * // 读取缓存
 * const value = cache.get('key1'); // 'value1'
 *
 * // 检查是否存在
 * const exists = cache.has('key1'); // true
 *
 * // 清除缓存
 * cache.clear();
 * ```
 */
export declare class LRUCache<T> {
    private cache;
    private readonly maxSize;
    private readonly ttl;
    private readonly sizeCalculator;
    private totalSize;
    constructor(options?: LRUCacheOptions);
    /**
     * 获取缓存项
     * @param key 缓存键
     * @returns 缓存值，如果不存在或已过期则返回 undefined
     */
    get(key: string): T | undefined;
    /**
     * 设置缓存项
     * @param key 缓存键
     * @param value 缓存值
     */
    set(key: string, value: T): void;
    /**
     * 检查缓存中是否存在指定键
     * @param key 缓存键
     * @returns 是否存在且未过期
     */
    has(key: string): boolean;
    /**
     * 删除缓存项
     * @param key 缓存键
     * @returns 是否成功删除
     */
    delete(key: string): boolean;
    /**
     * 清空所有缓存
     */
    clear(): void;
    /**
     * 获取当前缓存的条目数量
     */
    get size(): number;
    /**
     * 获取所有缓存键
     */
    keys(): IterableIterator<string>;
    /**
     * 获取缓存统计信息
     */
    getStats(): {
        size: number;
        maxSize: number;
        totalSize: number;
        ttl: number | undefined;
    };
    /**
     * 清理过期的缓存项
     * @returns 清理的条目数量
     */
    purgeExpired(): number;
}
/**
 * 生成缓存键的辅助函数
 * @param parts 键的组成部分
 * @returns 缓存键
 */
export declare function createCacheKey(...parts: (string | number | boolean | undefined | null)[]): string;
/**
 * 简单的哈希函数（用于生成短缓存键）
 * @param str 输入字符串
 * @returns 哈希值
 */
export declare function simpleHash(str: string): string;
