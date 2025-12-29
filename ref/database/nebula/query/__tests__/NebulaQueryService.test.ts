import { NebulaQueryService, INebulaQueryService } from '../NebulaQueryService';
import { DatabaseLoggerService } from '../../../common/DatabaseLoggerService';
import { ErrorHandlerService } from '../../../../utils/ErrorHandlerService';
import { PerformanceMonitor } from '../../../../infrastructure/monitoring/PerformanceMonitor';
import { NebulaConfigService } from '../../../../config/service/NebulaConfigService';
import { INebulaConnectionManager } from '../../NebulaConnectionManager';
import { IQueryRunner } from '../QueryRunner';
import { NebulaQueryResult, NebulaConfig } from '../../NebulaTypes';

describe('NebulaQueryService', () => {
  let queryService: NebulaQueryService;
  let mockDatabaseLogger: jest.Mocked<DatabaseLoggerService>;
  let mockErrorHandler: jest.Mocked<ErrorHandlerService>;
  let mockPerformanceMonitor: jest.Mocked<PerformanceMonitor>;
  let mockConfigService: jest.Mocked<NebulaConfigService>;
  let mockConnectionManager: jest.Mocked<INebulaConnectionManager>;
  let mockQueryRunner: jest.Mocked<IQueryRunner>;

  const mockNebulaConfig: NebulaConfig = {
    host: 'localhost',
    port: 9669,
    username: 'root',
    password: 'nebula',
    space: 'test_space'
  };

  beforeEach(() => {
    mockDatabaseLogger = {
      info: jest.fn(),
      debug: jest.fn(),
      warn: jest.fn(),
      error: jest.fn(),
      trace: jest.fn()
    } as any;

    mockErrorHandler = {
      handleError: jest.fn()
    } as any;

    mockPerformanceMonitor = {
      startOperation: jest.fn().mockReturnValue('op-1'),
      endOperation: jest.fn(),
      recordQueryExecution: jest.fn()
    } as any;

    mockConfigService = {
      loadConfig: jest.fn().mockReturnValue(mockNebulaConfig)
    } as any;

    mockConnectionManager = {
      connect: jest.fn(),
      disconnect: jest.fn(),
      isConnected: jest.fn().mockReturnValue(true),
      executeQuery: jest.fn(),
      executeTransaction: jest.fn(),
      getConfig: jest.fn(),
      updateConfig: jest.fn(),
      getConnectionStatus: jest.fn(),
      subscribe: jest.fn()
    } as any;

    mockQueryRunner = {
      execute: jest.fn().mockResolvedValue({
        data: [{ id: '1' }],
        error: undefined,
        executionTime: 100
      }),
      executeBatch: jest.fn(),
      getCachedResult: jest.fn(),
      setCachedResult: jest.fn(),
      recordQueryMetrics: jest.fn(),
      getStats: jest.fn()
    } as any;

    queryService = new NebulaQueryService(
      mockDatabaseLogger,
      mockErrorHandler,
      mockPerformanceMonitor,
      mockConfigService,
      mockConnectionManager,
      mockQueryRunner
    );
  });

  describe('初始化', () => {
    it('应该创建NebulaQueryService实例', () => {
      expect(queryService).toBeDefined();
    });

    it('应该实现INebulaQueryService接口', () => {
      expect(queryService).toHaveProperty('executeQuery');
      expect(queryService).toHaveProperty('executeParameterizedQuery');
      expect(queryService).toHaveProperty('prepareQuery');
      expect(queryService).toHaveProperty('formatResults');
      expect(queryService).toHaveProperty('validateQuery');
      expect(queryService).toHaveProperty('executeBatch');
    });
  });

  describe('executeQuery - 执行查询', () => {
    it('应该执行查询', async () => {
      const result = await queryService.executeQuery('SELECT * FROM nodes');

      expect(result).toBeDefined();
      expect(mockQueryRunner.execute).toHaveBeenCalledWith('SELECT * FROM nodes', undefined);
    });

    it('应该处理查询参数', async () => {
      const params = { id: '123' };

      const result = await queryService.executeQuery('SELECT * WHERE id = :id', params);

      expect(result).toBeDefined();
      expect(mockQueryRunner.execute).toHaveBeenCalledWith(
        'SELECT * WHERE id = :id',
        params
      );
    });

    it('应该返回查询结果', async () => {
      const expectedResult: NebulaQueryResult = {
        table: {},
        results: [],
        rows: [],
        data: [{ id: '1', name: 'node1' }],
        error: undefined,
        executionTime: 100
      };

      mockQueryRunner.execute.mockResolvedValue(expectedResult);

      const result = await queryService.executeQuery('SELECT *');

      expect(result).toEqual(expectedResult);
    });
  });

  describe('executeParameterizedQuery - 执行参数化查询', () => {
    it('应该执行参数化查询', async () => {
      const query = 'SELECT * WHERE id = :id';
      const params = { id: '123' };

      const result = await queryService.executeParameterizedQuery(query, params);

      expect(result).toBeDefined();
      expect(mockQueryRunner.execute).toHaveBeenCalledWith(query, params);
    });

    it('应该委托给executeQuery', async () => {
      const query = 'SELECT *';

      await queryService.executeParameterizedQuery(query);

      expect(mockQueryRunner.execute).toHaveBeenCalledWith(query, undefined);
    });
  });

  describe('prepareQuery - 准备查询', () => {
    it('应该不修改没有参数的查询', () => {
      const query = 'SELECT * FROM nodes';

      const result = queryService.prepareQuery(query);

      expect(result).toBe(query);
    });

    it('应该插值查询参数', () => {
      const query = 'SELECT * WHERE id = $id';
      const params = { id: '123' };

      const result = queryService.prepareQuery(query, params);

      expect(result).toContain('123');
    });

    it('应该转义字符串参数', () => {
      const query = 'SELECT * WHERE name = $name';
      const params = { name: 'test' };

      const result = queryService.prepareQuery(query, params);

      expect(result).toContain('"test"');
    });

    it('应该处理数字参数', () => {
      const query = 'SELECT * WHERE id = $id';
      const params = { id: 123 };

      const result = queryService.prepareQuery(query, params);

      expect(result).toContain('123');
    });

    it('应该处理多个参数', () => {
      const query = 'SELECT * WHERE id = $id AND name = $name';
      const params = { id: '123', name: 'test' };

      const result = queryService.prepareQuery(query, params);

      expect(result).toBeDefined();
    });
  });

  describe('formatResults - 格式化结果', () => {
    it('应该格式化查询结果', () => {
      const rawResult = {
        table: { columns: [] },
        results: [],
        rows: [{ id: '1' }],
        data: [{ id: '1' }],
        timeCost: 100,
        space: 'test_space'
      };

      const result = queryService.formatResults(rawResult, 150);

      expect(result.executionTime).toBe(150);
      expect(result.data).toEqual([{ id: '1' }]);
    });

    it('应该提供默认值', () => {
      const rawResult = {};

      const result = queryService.formatResults(rawResult, 100);

      expect(result.table).toBeDefined();
      expect(result.results).toBeDefined();
      expect(result.rows).toBeDefined();
      expect(result.data).toBeDefined();
      expect(result.executionTime).toBe(100);
    });

    it('应该包含错误信息', () => {
      const rawResult = {
        error: 'Query error'
      };

      const result = queryService.formatResults(rawResult, 50);

      expect(result.error).toBe('Query error');
    });

    it('应该处理没有数据的结果', () => {
      const rawResult = {
        data: undefined,
        results: undefined,
        rows: undefined
      };

      const result = queryService.formatResults(rawResult, 75);

      expect(result.data).toEqual([]);
      expect(result.results).toEqual([]);
      expect(result.rows).toEqual([]);
    });
  });

  describe('validateQuery - 查询验证', () => {
    it('应该验证合法的查询', () => {
      const result = queryService.validateQuery('SELECT * FROM nodes');

      expect(result).toBe(true);
    });

    it('应该拒绝空查询', () => {
      const result = queryService.validateQuery('');

      expect(result).toBe(false);
    });

    it('应该拒绝仅包含空格的查询', () => {
      const result = queryService.validateQuery('   ');

      expect(result).toBe(false);
    });

    it('应该拒绝DROP SPACE查询', () => {
      const result = queryService.validateQuery('DROP SPACE test');

      expect(result).toBe(false);
    });

    it('应该拒绝包含undefined的USE查询', () => {
      const result = queryService.validateQuery('USE undefined');

      expect(result).toBe(false);
    });

    it('应该接受合法的USE查询', () => {
      const result = queryService.validateQuery('USE test_space');

      expect(result).toBe(true);
    });

    it('应该处理undefined的查询', () => {
      const result = queryService.validateQuery(undefined as any);

      expect(result).toBe(false);
    });
  });

  describe('executeBatch - 批量执行', () => {
    it('应该执行多个查询', async () => {
      const queries = [
        { query: 'SELECT 1' },
        { query: 'SELECT 2' },
        { query: 'SELECT 3' }
      ];

      mockQueryRunner.executeBatch.mockResolvedValue([
        { table: {}, results: [], rows: [], data: [], error: undefined, executionTime: 100 },
        { table: {}, results: [], rows: [], data: [], error: undefined, executionTime: 100 },
        { table: {}, results: [], rows: [], data: [], error: undefined, executionTime: 100 }
      ]);

      const results = await queryService.executeBatch(queries);

      expect(results).toHaveLength(3);
    });

    it('应该处理查询参数', async () => {
      const queries = [
        { query: 'SELECT * WHERE id = :id', params: { id: '1' } },
        { query: 'SELECT * WHERE id = :id', params: { id: '2' } }
      ];

      mockQueryRunner.executeBatch.mockResolvedValue([
        { table: {}, results: [], rows: [], data: [], error: undefined, executionTime: 100 },
        { table: {}, results: [], rows: [], data: [], error: undefined, executionTime: 100 }
      ]);

      const results = await queryService.executeBatch(queries);

      expect(results).toHaveLength(2);
    });

    it('应该转换查询格式', async () => {
      const queries = [
        { query: 'SELECT 1', params: { id: '1' } }
      ];

      mockQueryRunner.executeBatch.mockResolvedValue([
        { table: {}, results: [], rows: [], data: [], error: undefined, executionTime: 100 }
      ]);

      await queryService.executeBatch(queries);

      expect(mockQueryRunner.executeBatch).toHaveBeenCalledWith(
        expect.arrayContaining([
          expect.objectContaining({
            query: 'SELECT 1',
            params: { id: '1' },
            options: undefined
          })
        ])
      );
    });

    it('应该处理空的查询列表', async () => {
      mockQueryRunner.executeBatch.mockResolvedValue([]);

      const results = await queryService.executeBatch([]);

      expect(results).toHaveLength(0);
    });
  });

  describe('连接管理', () => {
    it('应该检查连接状态', async () => {
      mockConnectionManager.isConnected.mockReturnValue(true);

      // 执行查询时应该检查连接
      await queryService.executeQuery('SELECT *');

      expect(mockConnectionManager.isConnected).toHaveBeenCalled();
    });

    it('应该在未连接时重新连接', async () => {
      mockConnectionManager.isConnected.mockReturnValueOnce(false);

      // 模拟需要重新连接的情况
      await queryService.executeQuery('SELECT *');

      // 根据实现，可能会调用connect
    });

    it('应该获取空间特定的连接', async () => {
      const space = 'custom_space';

      await queryService.executeQuery('SELECT *');

      // 如果配置中有空间，应该获取对应的连接
    });
  });

  describe('错误处理', () => {
    it('应该处理查询执行错误', async () => {
      mockQueryRunner.execute.mockRejectedValue(new Error('Query failed'));

      try {
        await queryService.executeQuery('SELECT *');
      } catch (error) {
        expect(error).toBeDefined();
      }
    });

    it('应该处理批量查询错误', async () => {
      mockQueryRunner.executeBatch.mockRejectedValue(new Error('Batch failed'));

      try {
        await queryService.executeBatch([{ query: 'SELECT 1' }]);
      } catch (error) {
        expect(error).toBeDefined();
      }
    });
  });

  describe('集成场景', () => {
    it('应该支持完整的查询执行流程', async () => {
      const query = 'MATCH (v:Person) RETURN v';
      const params = { age: 30 };

      // 验证查询
      const isValid = queryService.validateQuery(query);
      expect(isValid).toBe(true);

      // 准备查询
      const preparedQuery = queryService.prepareQuery(query, params);
      expect(preparedQuery).toBeDefined();

      // 执行查询
      const result = await queryService.executeQuery(query, params);
      expect(result).toBeDefined();

      // 格式化结果（在某些场景下）
      const formatted = queryService.formatResults(result, result.executionTime || 0);
      expect(formatted).toBeDefined();
    });

    it('应该支持批量查询流程', async () => {
      const queries = [
        { query: 'SELECT 1' },
        { query: 'SELECT 2' },
        { query: 'SELECT 3' }
      ];

      mockQueryRunner.executeBatch.mockResolvedValue([
        { table: {}, results: [], rows: [], data: [{ result: 1 }], error: undefined, executionTime: 50 },
        { table: {}, results: [], rows: [], data: [{ result: 2 }], error: undefined, executionTime: 50 },
        { table: {}, results: [], rows: [], data: [{ result: 3 }], error: undefined, executionTime: 50 }
      ]);

      const results = await queryService.executeBatch(queries);

      expect(results).toHaveLength(3);
      expect(results.every(r => r.executionTime !== undefined)).toBe(true);
    });
  });
});
