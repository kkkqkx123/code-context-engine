import { LRUCache, CacheStats } from './LRUCache';

export interface DetailedStats extends CacheStats {
  gets: number;
  deletes: number;
  clears: number;
  hitRate: number;
  maxMemory?: number;
}

export class StatsDecorator<K, V> {
  private cache: LRUCache<K, V>;
  private stats = {
    hits: 0,
    misses: 0,
    evictions: 0,
    sets: 0,
    gets: 0,
    deletes: 0,
    clears: 0
  };

  constructor(cache: LRUCache<K, V>) {
    this.cache = cache;
  }

  get(key: K): V | undefined {
    this.stats.gets++;
    const result = this.cache.get(key);
    if (result === undefined) {
      this.stats.misses++;
    } else {
      this.stats.hits++;
    }
    return result;
  }

  set(key: K, value: V, ttl?: number): void {
    this.stats.sets++;
    const hadKey = this.cache.has(key);
    this.cache.set(key, value, ttl);
    if (!hadKey && this.cache.has(key) && this.cache.size() > 0) {
      // 新插入且缓存未空，说明可能发生了驱逐
      this.stats.evictions++;
    }
  }

  has(key: K): boolean {
    return this.cache.has(key);
  }

  delete(key: K): boolean {
    this.stats.deletes++;
    return this.cache.delete(key);
  }

  clear(): void {
    this.stats.clears++;
    this.cache.clear();
  }

  size(): number {
    return this.cache.size();
  }

  keys(): K[] {
    return this.cache.keys();
  }

  values(): V[] {
    return this.cache.values();
  }

  getStats(): DetailedStats {
    const baseStats = this.cache.getStats?.() || { hits: 0, misses: 0, evictions: 0, sets: 0, size: 0, memoryUsage: 0 };
    const totalGets = this.stats.gets;

    return {
      ...baseStats,
      ...this.stats,
      hitRate: totalGets > 0 ? this.stats.hits / totalGets : 0,
      size: this.cache.size()
    };
  }

  resetStats(): void {
    this.stats = {
      hits: 0,
      misses: 0,
      evictions: 0,
      sets: 0,
      gets: 0,
      deletes: 0,
      clears: 0
    };
  }

  // 获取底层缓存的引用（用于高级操作）
  getUnderlyingCache(): LRUCache<K, V> {
    return this.cache;
  }
}