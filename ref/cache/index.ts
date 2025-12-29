// 缓存工具模块导出
export { LRUCache, CacheOptions, CacheStats } from './LRUCache';
export { MemoryAwareCache, MemoryOptions } from './MemoryAwareCache';
export { StatsDecorator, DetailedStats } from './StatsDecorator';

// 导入所需的类以在工厂函数中使用
import { LRUCache } from './LRUCache';
import { MemoryAwareCache } from './MemoryAwareCache';
import { StatsDecorator } from './StatsDecorator';

// 便捷工厂函数
export function createCache<K, V>(
  type: 'basic' | 'memory-aware' | 'stats-decorated',
  maxSize: number = 1000,
  options: any = {}
): LRUCache<K, V> {
  switch (type) {
    case 'basic':
      return new LRUCache<K, V>(maxSize, options);
    case 'memory-aware':
      return new MemoryAwareCache<K, V>(maxSize, options);
    case 'stats-decorated':
      const cache = new LRUCache<K, V>(maxSize, options);
      return new StatsDecorator(cache) as any;
    default:
      throw new Error(`Unsupported cache type: ${type}`);
  }
}