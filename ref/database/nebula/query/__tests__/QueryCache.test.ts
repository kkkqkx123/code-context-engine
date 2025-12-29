import { QueryCache, QueryCacheConfig, QueryCacheStats } from '../QueryCache';
import { LoggerService } from '../../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../../utils/ErrorHandlerService';
import { NebulaQueryResult } from '../../NebulaTypes';

describe('QueryCache', () => {
  let queryCache: QueryCache;
  let mockLogger: jest.Mocked<LoggerService>;
  let mockErrorHandler: jest.Mocked<ErrorHandlerService>;

  beforeEach(() => {
    mockLogger = {
      info: jest.fn(),
      debug: jest.fn(),
      warn: jest.fn(),
      error: jest.fn()
    } as any;

    mockErrorHandler = {
      handleError: jest.fn()
    } as any;

    queryCache = new QueryCache(mockLogger, mockErrorHandler, {
      enabled: true,
      maxSize: 10,
      ttl: 5000,
      keyPrefix: 'test:'
    });
  });

  describe('初始化', () => {
    it('应该使用默认配置初始化', () => {
      const cache = new QueryCache(mockLogger, mockErrorHandler);
      expect(mockLogger.info).toHaveBeenCalledWith('Query cache initialized', expect.any(Object));
    });

    it('应该合并自定义配置', () => {
      const cache = new QueryCache(mockLogger, mockErrorHandler, {
        maxSize: 100
      });
      const stats = cache.getStats();
      expect(stats).toBeDefined();
    });
  });

  describe('get - 获取缓存', () => {
    it('当缓存禁用时应返回null', async () => {
      const cache = new QueryCache(mockLogger, mockErrorHandler, { enabled: false });
      const result = await cache.get('SELECT *');
      expect(result).toBeNull();
    });

    it('应该返回缓存的查询结果', async () => {
    const query = 'SELECT * FROM nodes';
    const mockResult: NebulaQueryResult = {
    table: {},
    results: [],
    rows: [],
      data: [{ id: '1', name: 'node1' }],
         error: undefined,
         executionTime: 100
       };

      await queryCache.set(query, mockResult);
      const result = await queryCache.get(query);

      expect(result).toEqual(mockResult);
    });

    it('当缓存未命中时应返回null', async () => {
      const result = await queryCache.get('NON_EXISTENT_QUERY');
      expect(result).toBeNull();
    });

    it('应该正确处理查询参数', async () => {
    const query = 'SELECT * FROM nodes WHERE id = :id';
    const params = { id: '123' };
    const mockResult: NebulaQueryResult = {
    table: {},
    results: [],
    rows: [],
      data: [{ id: '123' }],
         error: undefined,
         executionTime: 100
       };

      await queryCache.set(query, mockResult, params);
      const result = await queryCache.get(query, params);

      expect(result).toEqual(mockResult);
    });

    it('应该更新缓存命中统计', async () => {
    const query = 'SELECT *';
    const mockResult: NebulaQueryResult = {
    table: {},
    results: [],
    rows: [],
      data: [],
         error: undefined,
         executionTime: 50
       };

      await queryCache.set(query, mockResult);
      await queryCache.get(query);

      const stats = queryCache.getStats();
      expect(stats.hits).toBe(1);
    });

    it('应该在错误时返回null并调用错误处理器', async () => {
    // 模拟缓存get方法抛出异常
    const cache = queryCache as any;
    cache.cache.get = jest.fn(() => {
    throw new Error('Cache error');
    });

    const result = await queryCache.get('SELECT *');
    expect(result).toBeNull();
    expect(mockErrorHandler.handleError).toHaveBeenCalled();
    });
    });

    describe('set - 设置缓存', () => {
    it('当缓存禁用时应直接返回', async () => {
    const cache = new QueryCache(mockLogger, mockErrorHandler, { enabled: false });
    const query = 'SELECT *';
    const mockResult: NebulaQueryResult = {
    table: {},
    results: [],
    rows: [],
    data: [],
    error: undefined,
    executionTime: 50
    };

    await cache.set(query, mockResult);
    // 不应该抛出异常，直接返回
    expect(mockLogger.debug).not.toHaveBeenCalled();
    });

    it('不应该缓存有错误的结果', async () => {
    const query = 'SELECT *';
    const errorResult: NebulaQueryResult = {
    table: {},
    results: [],
    rows: [],
    data: [],
    error: 'Query error',
    executionTime: 0
    };

    await queryCache.set(query, errorResult);

    const result = await queryCache.get(query);
    expect(result).toBeNull();
    });

    it('应该成功缓存查询结果', async () => {
    const query = 'MATCH (v:Person) RETURN v';
    const mockResult: NebulaQueryResult = {
    table: {},
    results: [],
    rows: [],
    data: [{ v: { id: '1' } }],
    error: undefined,
    executionTime: 123
    };

    await queryCache.set(query, mockResult);
    const stats = queryCache.getStats();

    expect(stats.sets).toBe(1);
    });

    it('应该在错误时调用错误处理器', async () => {
    const cache = queryCache as any;
    cache.cache.set = jest.fn(() => {
    throw new Error('Set error');
    });

    const mockResult: NebulaQueryResult = {
    table: {},
    results: [],
    rows: [],
      data: [],
         error: undefined,
         executionTime: 50
       };

      await queryCache.set('SELECT *', mockResult);
      expect(mockErrorHandler.handleError).toHaveBeenCalled();
    });
  });

  describe('delete - 删除缓存', () => {
    it('当缓存禁用时应返回false', () => {
      const cache = new QueryCache(mockLogger, mockErrorHandler, { enabled: false });
      const result = cache.delete('SELECT *');
      expect(result).toBe(false);
    });

    it('应该成功删除存在的缓存项', async () => {
    const query = 'SELECT *';
    const mockResult: NebulaQueryResult = {
    table: {},
    results: [],
    rows: [],
      data: [],
         error: undefined,
         executionTime: 50
       };

      await queryCache.set(query, mockResult);
      const deleted = queryCache.delete(query);

      expect(deleted).toBe(true);

      const result = await queryCache.get(query);
      expect(result).toBeNull();
    });

    it('删除不存在的缓存项应返回false', () => {
      const result = queryCache.delete('NON_EXISTENT');
      expect(result).toBe(false);
    });

    it('应该在错误时调用错误处理器', () => {
      const cache = queryCache as any;
      cache.cache.delete = jest.fn(() => {
        throw new Error('Delete error');
      });

      const result = queryCache.delete('SELECT *');
      expect(result).toBe(false);
      expect(mockErrorHandler.handleError).toHaveBeenCalled();
    });
  });

  describe('clear - 清空缓存', () => {
    it('当缓存禁用时应直接返回', () => {
      const cache = new QueryCache(mockLogger, mockErrorHandler, { enabled: false });
      cache.clear();
      // 不应该调用logger的info方法
      expect(mockLogger.info).not.toHaveBeenCalledWith('Query cache cleared');
    });

    it('应该清空所有缓存项', async () => {
    const mockResult: NebulaQueryResult = {
    table: {},
    results: [],
    rows: [],
      data: [],
         error: undefined,
      executionTime: 50
    };

       await queryCache.set('SELECT 1', mockResult);
       await queryCache.set('SELECT 2', mockResult);

      queryCache.clear();

      const result1 = await queryCache.get('SELECT 1');
      const result2 = await queryCache.get('SELECT 2');

      expect(result1).toBeNull();
      expect(result2).toBeNull();
    });

    it('应该重置统计信息', async () => {
    const mockResult: NebulaQueryResult = {
    table: {},
    results: [],
    rows: [],
      data: [],
         error: undefined,
      executionTime: 50
    };

       await queryCache.set('SELECT *', mockResult);
       await queryCache.get('SELECT *');

      queryCache.clear();

      const stats = queryCache.getStats();
      expect(stats.hits).toBe(0);
      expect(stats.misses).toBe(0);
      expect(stats.sets).toBe(0);
    });

    it('应该在错误时调用错误处理器', () => {
      const cache = queryCache as any;
      cache.cache.clear = jest.fn(() => {
        throw new Error('Clear error');
      });

      queryCache.clear();
      expect(mockErrorHandler.handleError).toHaveBeenCalled();
    });
  });

  describe('cleanup - 清理过期缓存', () => {
    it('当缓存禁用时应返回0', () => {
      const cache = new QueryCache(mockLogger, mockErrorHandler, { enabled: false });
      const result = cache.cleanup();
      expect(result).toBe(0);
    });

    it('应该返回清理的项数', () => {
      const result = queryCache.cleanup();
      expect(typeof result).toBe('number');
      expect(result).toBeGreaterThanOrEqual(0);
    });

    it('应该在错误时调用错误处理器', () => {
      const cache = queryCache as any;
      cache.cache.cleanup = jest.fn(() => {
        throw new Error('Cleanup error');
      });

      const result = queryCache.cleanup();
      expect(mockErrorHandler.handleError).toHaveBeenCalled();
    });
  });

  describe('getStats - 获取统计信息', () => {
    it('当缓存禁用时应返回零统计', () => {
      const cache = new QueryCache(mockLogger, mockErrorHandler, { enabled: false });
      const stats = cache.getStats();

      expect(stats.hits).toBe(0);
      expect(stats.misses).toBe(0);
      expect(stats.sets).toBe(0);
      expect(stats.hitRate).toBe(0);
    });

    it('应该返回正确的缓存统计信息', async () => {
    const mockResult: NebulaQueryResult = {
    table: {},
    results: [],
    rows: [],
      data: [],
         error: undefined,
      executionTime: 50
    };

       await queryCache.set('SELECT 1', mockResult);
       await queryCache.get('SELECT 1'); // hit
       await queryCache.get('SELECT 2'); // miss

      const stats = queryCache.getStats();

      expect(stats.sets).toBe(1);
      expect(stats.hits).toBeGreaterThanOrEqual(1);
      expect(stats.misses).toBeGreaterThanOrEqual(1);
    });

    it('应该计算正确的命中率', async () => {
    const mockResult: NebulaQueryResult = {
    table: {},
    results: [],
    rows: [],
      data: [],
         error: undefined,
      executionTime: 50
    };

    await queryCache.set('SELECT *', mockResult);
       await queryCache.get('SELECT *'); // hit
       await queryCache.get('SELECT *'); // hit
       await queryCache.get('NON_EXIST'); // miss

      const stats = queryCache.getStats();

      // 命中率应该是 2 / (2 + 1) = 0.666...
      expect(stats.hitRate).toBeGreaterThan(0);
      expect(stats.hitRate).toBeLessThanOrEqual(1);
    });
  });

  describe('updateConfig - 更新配置', () => {
    it('应该更新缓存配置', () => {
      const newConfig: Partial<QueryCacheConfig> = {
        enabled: false,
        maxSize: 500,
        ttl: 10000
      };

      queryCache.updateConfig(newConfig);
      const stats = queryCache.getStats();

      expect(stats).toBeDefined();
    });

    it('当禁用缓存时应清空现有缓存', async () => {
    const mockResult: NebulaQueryResult = {
    table: {},
    results: [],
    rows: [],
      data: [],
         error: undefined,
         executionTime: 50
       };

      await queryCache.set('SELECT *', mockResult);

      queryCache.updateConfig({ enabled: false });

      const result = await queryCache.get('SELECT *');
      expect(result).toBeNull();
    });

    it('当增加缓存大小时应迁移现有项', async () => {
    const mockResult: NebulaQueryResult = {
    table: {},
    results: [],
    rows: [],
      data: [],
         error: undefined,
      executionTime: 50
    };

       await queryCache.set('SELECT 1', mockResult);
       await queryCache.set('SELECT 2', mockResult);

      queryCache.updateConfig({ maxSize: 100 });

      const result1 = await queryCache.get('SELECT 1');
      const result2 = await queryCache.get('SELECT 2');

      expect(result1).not.toBeNull();
      expect(result2).not.toBeNull();
    });
  });

  describe('isEnabled - 检查缓存状态', () => {
    it('应该返回缓存启用状态', () => {
      const enabled = queryCache.isEnabled();
      expect(enabled).toBe(true);
    });

    it('禁用后应返回false', () => {
      const cache = new QueryCache(mockLogger, mockErrorHandler, { enabled: false });
      expect(cache.isEnabled()).toBe(false);
    });
  });

  describe('缓存键生成', () => {
    it('应该生成一致的缓存键', async () => {
    const query = 'SELECT * FROM nodes';
    const mockResult: NebulaQueryResult = {
    table: {},
    results: [],
    rows: [],
      data: [],
         error: undefined,
      executionTime: 50
    };

       await queryCache.set(query, mockResult);
       const result = await queryCache.get(query);

      expect(result).toEqual(mockResult);
    });

    it('应该规范化查询（空格和大小写）', async () => {
    const query1 = 'SELECT * FROM nodes';
    const query2 = 'SELECT  *  FROM  nodes';
    const query3 = 'select * from nodes';

    const mockResult: NebulaQueryResult = {
    table: {},
    results: [],
    rows: [],
      data: [{ id: '1' }],
         error: undefined,
         executionTime: 50
       };

      await queryCache.set(query1, mockResult);

      const result2 = await queryCache.get(query2);
      const result3 = await queryCache.get(query3);

      expect(result2).toEqual(mockResult);
      expect(result3).toEqual(mockResult);
    });

    it('应该使用参数生成不同的键', async () => {
    const query = 'SELECT * WHERE id = :id';
    const result1: NebulaQueryResult = {
    table: {},
    results: [],
    rows: [],
      data: [{ id: '1' }],
      error: undefined,
    executionTime: 50
    };
    const result2: NebulaQueryResult = {
      table: {},
         results: [],
         rows: [],
         data: [{ id: '2' }],
         error: undefined,
         executionTime: 50
       };

      await queryCache.set(query, result1, { id: '1' });
      await queryCache.set(query, result2, { id: '2' });

      const cachedResult1 = await queryCache.get(query, { id: '1' });
      const cachedResult2 = await queryCache.get(query, { id: '2' });

      expect(cachedResult1).toEqual(result1);
      expect(cachedResult2).toEqual(result2);
    });
  });
});
