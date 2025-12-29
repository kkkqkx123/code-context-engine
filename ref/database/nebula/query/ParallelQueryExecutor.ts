import { injectable, inject } from 'inversify';
import { EventEmitter } from 'events';
import { TYPES } from '../../../types';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { PerformanceMonitor } from '../../../infrastructure/monitoring/PerformanceMonitor';
import { NebulaQueryResult } from '../NebulaTypes';
import { IQueryRunner } from './QueryRunner';

// 并行查询任务
export interface ParallelQueryTask {
  id: string;
  query: string;
  params?: Record<string, any>;
  options?: any;
  priority?: number;
  dependencies?: string[];
}

// 并行查询结果
export interface ParallelQueryResult {
  taskId: string;
  result?: NebulaQueryResult;
  error?: Error;
  executionTime: number;
  startTime: Date;
  endTime: Date;
}

// 并行执行配置
export interface ParallelExecutionConfig {
  maxConcurrency: number;
  enableDependencyResolution: boolean;
  timeout: number;
  retryFailedTasks: boolean;
  maxRetries: number;
  retryDelay: number;
}

// 默认并行执行配置
const DEFAULT_PARALLEL_CONFIG: ParallelExecutionConfig = {
  maxConcurrency: 10,
  enableDependencyResolution: true,
  timeout: 30000,
  retryFailedTasks: true,
  maxRetries: 3,
  retryDelay: 1000
};

/**
 * 并行查询执行器
 * 支持并行执行多个查询，处理任务依赖关系和优先级
 */
@injectable()
export class ParallelQueryExecutor extends EventEmitter {
  private logger: LoggerService;
  private errorHandler: ErrorHandlerService;
  private performanceMonitor: PerformanceMonitor;
  private queryRunner: IQueryRunner;
  private config: ParallelExecutionConfig;
  private runningTasks: Map<string, Promise<void>> = new Map();
  private completedTasks: Map<string, ParallelQueryResult> = new Map();
  private taskQueue: ParallelQueryTask[] = [];

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.PerformanceMonitor) performanceMonitor: PerformanceMonitor,
    @inject(TYPES.IQueryRunner) queryRunner: IQueryRunner
  ) {
    super();
    this.logger = logger;
    this.errorHandler = errorHandler;
    this.performanceMonitor = performanceMonitor;
    this.queryRunner = queryRunner;
    this.config = { ...DEFAULT_PARALLEL_CONFIG };
  }

  /**
   * 更新并行执行配置
   */
  updateConfig(config: Partial<ParallelExecutionConfig>): void {
    this.config = { ...this.config, ...config };
    this.logger.info('Parallel query executor config updated', { config: this.config });
  }

  /**
   * 执行并行查询
   */
  async executeParallel(tasks: ParallelQueryTask[]): Promise<ParallelQueryResult[]> {
    const startTime = Date.now();
    
    try {
      this.logger.info('Starting parallel query execution', {
        taskCount: tasks.length,
        maxConcurrency: this.config.maxConcurrency
      });

      // 清理之前的结果
      this.completedTasks.clear();
      this.taskQueue = [...tasks];

      // 按优先级排序
      this.taskQueue.sort((a, b) => (b.priority || 0) - (a.priority || 0));

      // 解析依赖关系
      if (this.config.enableDependencyResolution) {
        this.resolveDependencies();
      }

      // 执行任务
      const results = await this.executeTasks();

      const totalTime = Date.now() - startTime;
      
      // 记录性能指标
      this.recordExecutionMetrics(tasks.length, results, totalTime);

      this.logger.info('Parallel query execution completed', {
        totalTasks: tasks.length,
        successfulTasks: results.filter(r => !r.error).length,
        failedTasks: results.filter(r => r.error).length,
        totalTime
      });

      return results;
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Parallel execution failed'),
        { component: 'ParallelQueryExecutor', operation: 'executeParallel', taskCount: tasks.length }
      );
      throw error;
    }
  }

  /**
   * 执行单个查询任务
   */
  async executeTask(task: ParallelQueryTask): Promise<ParallelQueryResult> {
    const startTime = new Date();
    let retryCount = 0;

    while (retryCount <= this.config.maxRetries) {
      try {
        this.logger.debug('Executing query task', {
          taskId: task.id,
          query: task.query.substring(0, 100) + (task.query.length > 100 ? '...' : ''),
          retryCount
        });

        const result = await this.queryRunner.execute(task.query, task.params, task.options);
        
        const parallelResult: ParallelQueryResult = {
          taskId: task.id,
          result,
          executionTime: Date.now() - startTime.getTime(),
          startTime,
          endTime: new Date()
        };

        this.emit('taskCompleted', parallelResult);
        return parallelResult;
      } catch (error) {
        retryCount++;
        
        if (retryCount <= this.config.maxRetries && this.config.retryFailedTasks) {
          this.logger.warn('Query task failed, retrying', {
            taskId: task.id,
            error: error instanceof Error ? error.message : String(error),
            retryCount,
            maxRetries: this.config.maxRetries
          });

          await this.delay(this.config.retryDelay);
        } else {
          const parallelResult: ParallelQueryResult = {
            taskId: task.id,
            error: error instanceof Error ? error : new Error(String(error)),
            executionTime: Date.now() - startTime.getTime(),
            startTime,
            endTime: new Date()
          };

          this.emit('taskFailed', parallelResult);
          return parallelResult;
        }
      }
    }

    // 这行代码理论上不会执行到，但为了类型安全
    throw new Error('Max retries exceeded');
  }

  /**
   * 获取任务状态
   */
  getTaskStatus(taskId: string): 'pending' | 'running' | 'completed' | 'failed' | 'not_found' {
    if (this.completedTasks.has(taskId)) {
      const result = this.completedTasks.get(taskId)!;
      return result.error ? 'failed' : 'completed';
    }
    
    if (this.runningTasks.has(taskId)) {
      return 'running';
    }
    
    if (this.taskQueue.some(task => task.id === taskId)) {
      return 'pending';
    }
    
    return 'not_found';
  }

  /**
   * 获取执行统计信息
   */
  getStats(): any {
    const completedResults = Array.from(this.completedTasks.values());
    const successfulTasks = completedResults.filter(r => !r.error);
    const failedTasks = completedResults.filter(r => r.error);

    return {
      runningTasks: this.runningTasks.size,
      pendingTasks: this.taskQueue.length,
      completedTasks: this.completedTasks.size,
      successfulTasks: successfulTasks.length,
      failedTasks: failedTasks.length,
      averageExecutionTime: successfulTasks.length > 0 
        ? successfulTasks.reduce((sum, task) => sum + task.executionTime, 0) / successfulTasks.length 
        : 0,
      config: this.config
    };
  }

  /**
   * 解析任务依赖关系
   */
  private resolveDependencies(): void {
    const taskMap = new Map(this.taskQueue.map(task => [task.id, task]));
    const resolved: string[] = [];
    const unresolved: string[] = [];

    // 拓扑排序
    while (resolved.length < this.taskQueue.length) {
      let progressMade = false;

      for (const task of this.taskQueue) {
        if (resolved.includes(task.id) || unresolved.includes(task.id)) {
          continue;
        }

        const dependencies = task.dependencies || [];
        const allDependenciesResolved = dependencies.every(dep => resolved.includes(dep));

        if (allDependenciesResolved) {
          resolved.push(task.id);
          progressMade = true;
        } else if (dependencies.some(dep => !taskMap.has(dep))) {
          // 依赖的任务不存在
          this.logger.error('Task dependency not found', {
            taskId: task.id,
            missingDependency: dependencies.find(dep => !taskMap.has(dep))
          });
          unresolved.push(task.id);
        }
      }

      if (!progressMade) {
        // 检测循环依赖
        const remainingTasks = this.taskQueue.filter(task => 
          !resolved.includes(task.id) && !unresolved.includes(task.id)
        );
        
        this.logger.error('Circular dependency detected', {
          remainingTasks: remainingTasks.map(t => t.id)
        });
        
        remainingTasks.forEach(task => unresolved.push(task.id));
        break;
      }
    }

    // 重新排序任务队列
    this.taskQueue = [
      ...this.taskQueue.filter(task => resolved.includes(task.id)),
      ...this.taskQueue.filter(task => unresolved.includes(task.id))
    ];
  }

  /**
   * 执行所有任务
   */
  private async executeTasks(): Promise<ParallelQueryResult[]> {
    const results: ParallelQueryResult[] = [];
    const executing: Promise<void>[] = [];

    while (this.taskQueue.length > 0 || executing.length > 0) {
      // 启动新任务直到达到最大并发数
      while (this.runningTasks.size < this.config.maxConcurrency && this.taskQueue.length > 0) {
        const task = this.taskQueue.shift()!;
        
        const taskPromise = this.executeTask(task).then(result => {
          this.runningTasks.delete(task.id);
          this.completedTasks.set(task.id, result);
        });

        this.runningTasks.set(task.id, taskPromise);
        executing.push(taskPromise);
      }

      // 等待至少一个任务完成
      if (executing.length > 0) {
        await Promise.race(executing);
        
        // 移除已完成的任务
        for (let i = executing.length - 1; i >= 0; i--) {
          const promise = executing[i];
          if (await this.isPromiseSettled(promise)) {
            executing.splice(i, 1);
          }
        }
      }
    }

    // 收集所有结果
    for (const result of this.completedTasks.values()) {
      results.push(result);
    }

    return results;
  }

  /**
   * 检查Promise是否已解决
   */
  private async isPromiseSettled(promise: Promise<any>): Promise<boolean> {
    try {
      await Promise.race([promise, Promise.resolve('timeout')]);
      return true;
    } catch {
      return false;
    }
  }

  /**
   * 记录执行性能指标
   */
  private recordExecutionMetrics(taskCount: number, results: ParallelQueryResult[], totalTime: number): void {
    const operationId = this.performanceMonitor.startOperation('parallel_query_execution', {
      taskCount,
      maxConcurrency: this.config.maxConcurrency,
      successfulTasks: results.filter(r => !r.error).length,
      failedTasks: results.filter(r => r.error).length
    });

    this.performanceMonitor.endOperation(operationId, {
      success: results.every(r => !r.error),
      duration: totalTime,
      metadata: {
        averageTaskTime: results.reduce((sum, r) => sum + r.executionTime, 0) / results.length,
        throughput: taskCount / (totalTime / 1000), // tasks per second
        concurrencyUtilization: this.runningTasks.size / this.config.maxConcurrency
      }
    });
  }

  /**
   * 延迟函数
   */
  private delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}