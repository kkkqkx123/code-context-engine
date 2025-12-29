import { injectable, inject } from 'inversify';
import { TYPES } from '../../../types';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { LRUCache } from '../../../utils/cache/LRUCache';
import { NebulaQueryResult } from '../NebulaTypes';
import crypto from 'crypto';

// 查询缓存配置
export interface QueryCacheConfig {
  enabled: boolean;
  maxSize: number;
  ttl: number; // 生存时间（毫秒）
  keyPrefix: string;
}

// 缓存统计信息
export interface QueryCacheStats {
  hits: number;
  misses: number;
  sets: number;
  evictions: number;
  size: number;
  hitRate: number;
}

// 默认缓存配置
const DEFAULT_CACHE_CONFIG: QueryCacheConfig = {
  enabled: true,
  maxSize: 1000,
  ttl: 300000, // 5分钟
  keyPrefix: 'nebula_query:'
};

/**
 * 查询结果缓存
 * 使用LRU缓存策略存储查询结果
 */
@injectable()
export class QueryCache {
  private logger: LoggerService;
  private errorHandler: ErrorHandlerService;
  private config: QueryCacheConfig;
  private cache: LRUCache<string, NebulaQueryResult>;
  private stats: QueryCacheStats;

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    config: Partial<QueryCacheConfig> = {}
  ) {
    this.logger = logger;
    this.errorHandler = errorHandler;
    this.config = { ...DEFAULT_CACHE_CONFIG, ...config };
    
    // 初始化LRU缓存
    this.cache = new LRUCache<string, NebulaQueryResult>(this.config.maxSize, {
      enableStats: true,
      defaultTTL: this.config.ttl
    });
    
    this.stats = {
      hits: 0,
      misses: 0,
      sets: 0,
      evictions: 0,
      size: 0,
      hitRate: 0
    };

    this.logger.info('Query cache initialized', {
      enabled: this.config.enabled,
      maxSize: this.config.maxSize,
      ttl: this.config.ttl
    });
  }

  /**
   * 获取缓存的查询结果
   */
  async get(query: string, params?: Record<string, any>): Promise<NebulaQueryResult | null> {
    if (!this.config.enabled) {
      return null;
    }

    try {
      const key = this.generateKey(query, params);
      const result = this.cache.get(key);
      
      if (result) {
        this.stats.hits++;
        this.logger.debug('Query cache hit', { 
          query: query.substring(0, 100),
          key 
        });
        return result;
      } else {
        this.stats.misses++;
        this.logger.debug('Query cache miss', { 
          query: query.substring(0, 100),
          key 
        });
        return null;
      }
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Failed to get query from cache'),
        { component: 'QueryCache', operation: 'get', query }
      );
      return null;
    }
  }

  /**
   * 设置查询结果到缓存
   */
  async set(query: string, result: NebulaQueryResult, params?: Record<string, any>): Promise<void> {
    if (!this.config.enabled) {
      return;
    }

    // 不缓存有错误的结果
    if (result.error) {
      this.logger.debug('Skipping cache for error result', {
        query: query.substring(0, 100),
        error: result.error
      });
      return;
    }

    try {
      const key = this.generateKey(query, params);
      this.cache.set(key, result);
      this.stats.sets++;
      
      this.logger.debug('Query result cached', { 
        query: query.substring(0, 100),
        key,
        dataSize: result.data?.length || 0
      });
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Failed to set query to cache'),
        { component: 'QueryCache', operation: 'set', query }
      );
    }
  }

  /**
   * 删除指定的缓存项
   */
  delete(query: string, params?: Record<string, any>): boolean {
    if (!this.config.enabled) {
      return false;
    }

    try {
      const key = this.generateKey(query, params);
      const deleted = this.cache.delete(key);
      
      if (deleted) {
        this.logger.debug('Query cache entry deleted', { 
          query: query.substring(0, 100),
          key 
        });
      }
      
      return deleted;
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Failed to delete query from cache'),
        { component: 'QueryCache', operation: 'delete', query }
      );
      return false;
    }
  }

  /**
   * 清空所有缓存
   */
  clear(): void {
    if (!this.config.enabled) {
      return;
    }

    try {
      this.cache.clear();
      this.stats = {
        hits: 0,
        misses: 0,
        sets: 0,
        evictions: 0,
        size: 0,
        hitRate: 0
      };
      
      this.logger.info('Query cache cleared');
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Failed to clear query cache'),
        { component: 'QueryCache', operation: 'clear' }
      );
    }
  }

  /**
   * 清理过期的缓存项
   */
  cleanup(): number {
    if (!this.config.enabled) {
      return 0;
    }

    try {
      const cleaned = this.cache.cleanup();
      this.logger.debug('Query cache cleanup completed', { cleaned });
      return cleaned;
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Failed to cleanup query cache'),
        { component: 'QueryCache', operation: 'cleanup' }
      );
      return 0;
    }
  }

  /**
   * 获取缓存统计信息
   */
  getStats(): QueryCacheStats {
    if (!this.config.enabled) {
      return {
        hits: 0,
        misses: 0,
        sets: 0,
        evictions: 0,
        size: 0,
        hitRate: 0
      };
    }

    // 更新统计信息
    const cacheStats = this.cache.getStats();
    if (cacheStats) {
      this.stats.hits = cacheStats.hits;
      this.stats.misses = cacheStats.misses;
      this.stats.sets = cacheStats.sets;
      this.stats.evictions = cacheStats.evictions;
      this.stats.size = cacheStats.size;
      
      const total = this.stats.hits + this.stats.misses;
      this.stats.hitRate = total > 0 ? this.stats.hits / total : 0;
    }

    return { ...this.stats };
  }

  /**
   * 更新缓存配置
   */
  updateConfig(config: Partial<QueryCacheConfig>): void {
    const oldConfig = { ...this.config };
    this.config = { ...this.config, ...config };
    
    // 如果禁用了缓存，清空现有缓存
    if (!this.config.enabled && oldConfig.enabled) {
      this.clear();
    }
    
    // 如果缓存大小改变，重新创建缓存
    if (this.config.maxSize !== oldConfig.maxSize) {
      const oldCache = this.cache;
      this.cache = new LRUCache<string, NebulaQueryResult>(this.config.maxSize, {
        enableStats: true,
        defaultTTL: this.config.ttl
      });
      
      // 迁移现有缓存项（如果新大小更大）
      if (this.config.maxSize > oldConfig.maxSize) {
        const keys = oldCache.keys();
        for (const key of keys) {
          const value = oldCache.get(key);
          if (value) {
            this.cache.set(key, value);
          }
        }
      }
    }
    
    this.logger.info('Query cache configuration updated', { config: this.config });
  }

  /**
   * 检查缓存是否启用
   */
  isEnabled(): boolean {
    return this.config.enabled;
  }

  /**
   * 生成缓存键
   */
  private generateKey(query: string, params?: Record<string, any>): string {
    // 标准化查询（移除多余空格，转换为小写）
    const normalizedQuery = query.trim().replace(/\s+/g, ' ').toLowerCase();
    
    // 如果有参数，将参数序列化并包含在键中
    if (params && Object.keys(params).length > 0) {
      const sortedParams = Object.keys(params)
        .sort()
        .reduce((result, key) => {
          result[key] = params[key];
          return result;
        }, {} as Record<string, any>);
      
      const paramString = JSON.stringify(sortedParams);
      const combined = `${normalizedQuery}:${paramString}`;
      return this.config.keyPrefix + crypto.createHash('md5').update(combined).digest('hex');
    }
    
    return this.config.keyPrefix + crypto.createHash('md5').update(normalizedQuery).digest('hex');
  }
}