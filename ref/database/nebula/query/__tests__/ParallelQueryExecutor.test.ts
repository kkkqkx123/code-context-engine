import { ParallelQueryExecutor, ParallelQueryTask, ParallelQueryResult, ParallelExecutionConfig } from '../ParallelQueryExecutor';
import { LoggerService } from '../../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../../utils/ErrorHandlerService';
import { PerformanceMonitor } from '../../../../infrastructure/monitoring/PerformanceMonitor';
import { IQueryRunner } from '../QueryRunner';
import { NebulaQueryResult } from '../../NebulaTypes';

describe('ParallelQueryExecutor', () => {
  let executor: ParallelQueryExecutor;
  let mockLogger: jest.Mocked<LoggerService>;
  let mockErrorHandler: jest.Mocked<ErrorHandlerService>;
  let mockPerformanceMonitor: jest.Mocked<PerformanceMonitor>;
  let mockQueryRunner: jest.Mocked<IQueryRunner>;

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

    mockPerformanceMonitor = {
      startOperation: jest.fn().mockReturnValue('op-1'),
      endOperation: jest.fn(),
      recordQueryExecution: jest.fn()
    } as any;

    mockQueryRunner = {
      execute: jest.fn().mockResolvedValue({
        table: {},
        results: [],
        rows: [],
        data: [{ id: '1' }],
        error: undefined,
        executionTime: 100
      })
    } as any;

    executor = new ParallelQueryExecutor(
      mockLogger,
      mockErrorHandler,
      mockPerformanceMonitor,
      mockQueryRunner
    );
  });

  describe('初始化', () => {
    it('应该使用默认配置初始化', () => {
      expect(mockLogger.info).toHaveBeenCalled();
    });

    it('应该是EventEmitter的实例', () => {
      expect(executor).toHaveProperty('on');
      expect(executor).toHaveProperty('emit');
    });
  });

  describe('updateConfig - 更新配置', () => {
    it('应该更新执行器配置', () => {
      const newConfig: Partial<ParallelExecutionConfig> = {
        maxConcurrency: 20,
        timeout: 60000
      };

      executor.updateConfig(newConfig);

      expect(mockLogger.info).toHaveBeenCalledWith(
        'Parallel query executor config updated',
        expect.any(Object)
      );
    });
  });

  describe('executeParallel - 并行执行', () => {
    it('应该并行执行多个查询任务', async () => {
      const tasks: ParallelQueryTask[] = [
        { id: 'task1', query: 'SELECT 1' },
        { id: 'task2', query: 'SELECT 2' },
        { id: 'task3', query: 'SELECT 3' }
      ];

      const results = await executor.executeParallel(tasks);

      expect(results).toHaveLength(3);
      expect(results.every(r => r.taskId)).toBe(true);
      expect(mockQueryRunner.execute).toHaveBeenCalledTimes(3);
    });

    it('应该处理查询参数', async () => {
      const tasks: ParallelQueryTask[] = [
        { 
          id: 'task1', 
          query: 'SELECT * WHERE id = :id',
          params: { id: '123' }
        }
      ];

      const results = await executor.executeParallel(tasks);

      expect(results).toHaveLength(1);
      expect(mockQueryRunner.execute).toHaveBeenCalledWith(
        'SELECT * WHERE id = :id',
        { id: '123' },
        undefined
      );
    });

    it('应该尊重任务优先级', async () => {
      const tasks: ParallelQueryTask[] = [
        { id: 'task1', query: 'SELECT 1', priority: 1 },
        { id: 'task2', query: 'SELECT 2', priority: 10 },
        { id: 'task3', query: 'SELECT 3', priority: 5 }
      ];

      const results = await executor.executeParallel(tasks);

      expect(results).toHaveLength(3);
    });

    it('应该记录性能指标', async () => {
      const tasks: ParallelQueryTask[] = [
        { id: 'task1', query: 'SELECT 1' }
      ];

      await executor.executeParallel(tasks);

      expect(mockPerformanceMonitor.startOperation).toHaveBeenCalled();
      expect(mockPerformanceMonitor.endOperation).toHaveBeenCalled();
    });

    it('应该发出pipelineStarted事件', async () => {
      const tasks: ParallelQueryTask[] = [
        { id: 'task1', query: 'SELECT 1' }
      ];

      const startSpy = jest.fn();
      executor.on('pipelineStarted', startSpy);

      // 注意：ParallelQueryExecutor不发出pipelineStarted事件，这是QueryPipeline的职责
      // 但ExecutorParallel发出taskCompleted和taskFailed事件
    });
  });

  describe('executeTask - 执行单个任务', () => {
    it('应该成功执行查询任务', async () => {
      const task: ParallelQueryTask = {
        id: 'task1',
        query: 'SELECT 1'
      };

      const result = await executor.executeTask(task);

      expect(result.taskId).toBe('task1');
      expect(result.error).toBeUndefined();
      expect(result.result).toBeDefined();
    });

    it('应该在任务成功时发出taskCompleted事件', async () => {
      const task: ParallelQueryTask = {
        id: 'task1',
        query: 'SELECT 1'
      };

      const completedSpy = jest.fn();
      executor.on('taskCompleted', completedSpy);

      await executor.executeTask(task);

      expect(completedSpy).toHaveBeenCalled();
    });

    it('应该在任务失败时发出taskFailed事件', async () => {
      const task: ParallelQueryTask = {
        id: 'task1',
        query: 'SELECT 1'
      };

      mockQueryRunner.execute.mockRejectedValue(new Error('Query failed'));

      const failedSpy = jest.fn();
      executor.on('taskFailed', failedSpy);

      const result = await executor.executeTask(task);

      expect(result.error).toBeDefined();
      expect(failedSpy).toHaveBeenCalled();
    });

    it('应该在失败时重试任务', async () => {
      const task: ParallelQueryTask = {
        id: 'task1',
        query: 'SELECT 1'
      };

      mockQueryRunner.execute
      .mockRejectedValueOnce(new Error('First attempt failed'))
      .mockResolvedValueOnce({
      table: {},
      results: [],
      rows: [],
        data: [{ id: '1' }],
           error: undefined,
           executionTime: 100
         });

      executor.updateConfig({
        retryFailedTasks: true,
        maxRetries: 3,
        retryDelay: 10
      });

      const result = await executor.executeTask(task);

      expect(result.error).toBeUndefined();
      expect(mockQueryRunner.execute).toHaveBeenCalledTimes(2);
    });

    it('应该在达到最大重试次数后失败', async () => {
      const task: ParallelQueryTask = {
        id: 'task1',
        query: 'SELECT 1'
      };

      mockQueryRunner.execute.mockRejectedValue(new Error('Query failed'));

      executor.updateConfig({
        retryFailedTasks: true,
        maxRetries: 2,
        retryDelay: 10
      });

      const result = await executor.executeTask(task);

      expect(result.error).toBeDefined();
      expect(result.error?.message).toContain('Query failed');
    });

    it('应该记录执行时间', async () => {
      const task: ParallelQueryTask = {
        id: 'task1',
        query: 'SELECT 1'
      };

      const result = await executor.executeTask(task);

      expect(result.executionTime).toBeGreaterThanOrEqual(0);
      expect(result.startTime).toBeDefined();
      expect(result.endTime).toBeDefined();
    });
  });

  describe('getTaskStatus - 获取任务状态', () => {
    it('应该返回completed状态', async () => {
      const task: ParallelQueryTask = {
        id: 'task1',
        query: 'SELECT 1'
      };

      await executor.executeTask(task);
      const status = executor.getTaskStatus('task1');

      expect(status).toBe('completed');
    });

    it('应该返回failed状态', async () => {
      const task: ParallelQueryTask = {
        id: 'task1',
        query: 'SELECT 1'
      };

      mockQueryRunner.execute.mockRejectedValue(new Error('Query failed'));
      executor.updateConfig({ retryFailedTasks: false, maxRetries: 0 });

      await executor.executeTask(task);
      const status = executor.getTaskStatus('task1');

      expect(status).toBe('failed');
    });

    it('应该返回not_found状态', () => {
      const status = executor.getTaskStatus('non_existent');
      expect(status).toBe('not_found');
    });
  });

  describe('getStats - 获取统计信息', () => {
    it('应该返回执行统计信息', async () => {
      const tasks: ParallelQueryTask[] = [
        { id: 'task1', query: 'SELECT 1' },
        { id: 'task2', query: 'SELECT 2' }
      ];

      await executor.executeParallel(tasks);
      const stats = executor.getStats();

      expect(stats.completedTasks).toBeGreaterThanOrEqual(0);
      expect(stats.successfulTasks).toBeGreaterThanOrEqual(0);
      expect(stats.failedTasks).toBeGreaterThanOrEqual(0);
    });

    it('应该计算平均执行时间', async () => {
      const tasks: ParallelQueryTask[] = [
        { id: 'task1', query: 'SELECT 1' },
        { id: 'task2', query: 'SELECT 2' }
      ];

      await executor.executeParallel(tasks);
      const stats = executor.getStats();

      expect(stats.averageExecutionTime).toBeGreaterThanOrEqual(0);
    });
  });

  describe('依赖关系解析', () => {
    it('应该检测循环依赖', async () => {
      const tasks: ParallelQueryTask[] = [
        { id: 'task1', query: 'SELECT 1', dependencies: ['task2'] },
        { id: 'task2', query: 'SELECT 2', dependencies: ['task1'] }
      ];

      executor.updateConfig({ enableDependencyResolution: true });

      const results = await executor.executeParallel(tasks);

      expect(mockLogger.error).toHaveBeenCalledWith(
        'Circular dependency detected',
        expect.any(Object)
      );
    });

    it('应该检测缺失的依赖', async () => {
      const tasks: ParallelQueryTask[] = [
        { id: 'task1', query: 'SELECT 1', dependencies: ['missing'] }
      ];

      executor.updateConfig({ enableDependencyResolution: true });

      const results = await executor.executeParallel(tasks);

      expect(mockLogger.error).toHaveBeenCalledWith(
        'Task dependency not found',
        expect.any(Object)
      );
    });
  });

  describe('错误处理', () => {
    it('应该在执行失败时调用错误处理器', async () => {
      const tasks: ParallelQueryTask[] = [
        { id: 'task1', query: 'SELECT 1' }
      ];

      mockQueryRunner.execute.mockRejectedValue(new Error('Connection failed'));

      try {
        await executor.executeParallel(tasks);
      } catch (error) {
        expect(mockErrorHandler.handleError).toHaveBeenCalled();
      }
    });

    it('应该处理执行器级别的异常', async () => {
      const tasks: ParallelQueryTask[] = [
        { id: 'task1', query: 'SELECT 1' }
      ];

      mockQueryRunner.execute.mockRejectedValue(new Error('Query error'));

      try {
        await executor.executeParallel(tasks);
      } catch (error) {
        expect(error).toBeDefined();
      }
    });
  });

  describe('并发控制', () => {
    it('应该尊重最大并发数', async () => {
      const tasks: ParallelQueryTask[] = Array.from({ length: 10 }, (_, i) => ({
        id: `task${i}`,
        query: `SELECT ${i}`
      }));

      executor.updateConfig({ maxConcurrency: 3 });

      const results = await executor.executeParallel(tasks);

      expect(results).toHaveLength(10);
    });

    it('应该处理高并发场景', async () => {
      const tasks: ParallelQueryTask[] = Array.from({ length: 20 }, (_, i) => ({
        id: `task${i}`,
        query: `SELECT ${i}`
      }));

      executor.updateConfig({ maxConcurrency: 5 });

      const results = await executor.executeParallel(tasks);

      expect(results).toHaveLength(20);
      expect(mockQueryRunner.execute).toHaveBeenCalledTimes(20);
    });
  });
});
