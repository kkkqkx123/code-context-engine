import { injectable, inject } from 'inversify';
import { EventEmitter } from 'events';
import { TYPES } from '../../../types';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { NebulaConfigService } from '../../../config/service/NebulaConfigService';
import { PerformanceMonitor } from '../../../infrastructure/monitoring/PerformanceMonitor';
import { NebulaQueryResult, NebulaConfig } from '../NebulaTypes';
import { ISessionManager } from '../session/SessionManager';
import { Session } from '../session/Session';
import { QueryCache, QueryCacheConfig } from './QueryCache';
import { IBatchProcessingService, BatchContext, BatchProcessingOptions } from '../../../infrastructure/batching/types';
import { IRetryStrategy } from '../retry/RetryStrategy';
import { ICircuitBreaker } from '../circuit-breaker/CircuitBreaker';
import { HashUtils } from '../../../utils/cache/HashUtils';

// 查询选项
export interface QueryOptions {
  timeout?: number;
  retryAttempts?: number;
  useCache?: boolean;
  spaceName?: string;
}

// 查询批次
export interface QueryBatch {
  query: string;
  params?: Record<string, any>;
  options?: QueryOptions;
}

// 查询执行器配置
export interface QueryRunnerConfig {
  defaultTimeout: number;
  defaultRetryAttempts: number;
  enableCache: boolean;
  cacheConfig: QueryCacheConfig;
}

// 查询执行器统计信息
export interface QueryRunnerStats {
  totalQueries: number;
  successfulQueries: number;
  failedQueries: number;
  cacheHits: number;
  cacheMisses: number;
  averageExecutionTime: number;
  totalExecutionTime: number;
  queriesByType: Record<string, number>;
}

// 默认查询执行器配置
const DEFAULT_QUERY_RUNNER_CONFIG: QueryRunnerConfig = {
  defaultTimeout: 30000,
  defaultRetryAttempts: 3,
  enableCache: true,
  cacheConfig: {
    enabled: true,
    maxSize: 1000,
    ttl: 300000, // 5分钟
    keyPrefix: 'nebula_query:'
  }
};

// 查询执行器接口
export interface IQueryRunner {
  // 查询执行
  execute(query: string, params?: Record<string, any>, options?: QueryOptions): Promise<NebulaQueryResult>;
  executeBatch(queries: QueryBatch[]): Promise<NebulaQueryResult[]>;
  
  // 查询缓存
  getCachedResult(queryKey: string): Promise<NebulaQueryResult | null>;
  setCachedResult(queryKey: string, result: NebulaQueryResult): Promise<void>;
  
  // 性能监控
  recordQueryMetrics(query: string, duration: number, success: boolean): void;
  
  // 统计信息
  getStats(): QueryRunnerStats;
}

/**
 * 查询执行器
 * 封装单次查询的完整执行逻辑，集成批处理服务
 */
@injectable()
export class QueryRunner extends EventEmitter implements IQueryRunner {
  private logger: LoggerService;
  private errorHandler: ErrorHandlerService;
  private configService: NebulaConfigService;
  private performanceMonitor: PerformanceMonitor;
  private sessionManager: ISessionManager;
  private queryCache: QueryCache;
  private config: QueryRunnerConfig;
  private nebulaConfig: NebulaConfig;
  private batchProcessingService: IBatchProcessingService;
  private retryStrategy: IRetryStrategy;
  private circuitBreaker: ICircuitBreaker;
  
  // 统计信息
  private stats: QueryRunnerStats = {
    totalQueries: 0,
    successfulQueries: 0,
    failedQueries: 0,
    cacheHits: 0,
    cacheMisses: 0,
    averageExecutionTime: 0,
    totalExecutionTime: 0,
    queriesByType: {}
  };

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.NebulaConfigService) configService: NebulaConfigService,
    @inject(TYPES.PerformanceMonitor) performanceMonitor: PerformanceMonitor,
    @inject(TYPES.ISessionManager) sessionManager: ISessionManager,
    @inject(TYPES.BatchProcessingService) batchProcessingService: IBatchProcessingService,
    @inject(TYPES.IRetryStrategy) retryStrategy: IRetryStrategy,
    @inject(TYPES.ICircuitBreaker) circuitBreaker: ICircuitBreaker,
    config: Partial<QueryRunnerConfig> = {},
    queryCache?: any
  ) {
    super();
    this.logger = logger;
    this.errorHandler = errorHandler;
    this.configService = configService;
    this.performanceMonitor = performanceMonitor;
    this.sessionManager = sessionManager;
    this.batchProcessingService = batchProcessingService;
    this.retryStrategy = retryStrategy;
    this.circuitBreaker = circuitBreaker;
    this.config = { ...DEFAULT_QUERY_RUNNER_CONFIG, ...config };
    
    // 加载Nebula配置
    this.nebulaConfig = this.configService.loadConfig();
    
    // 初始化查询缓存
    this.queryCache = queryCache || new QueryCache(logger, errorHandler, this.config.cacheConfig);
    
    this.logger.info('QueryRunner initialized', {
      defaultTimeout: this.config.defaultTimeout,
      defaultRetryAttempts: this.config.defaultRetryAttempts,
      enableCache: this.config.enableCache
    });
  }

  /**
   * 执行查询
   */
  async execute(
    query: string,
    params?: Record<string, any>,
    options?: QueryOptions
  ): Promise<NebulaQueryResult> {
    const queryType = this.detectQueryType(query);
    const mergedOptions = this.mergeOptions(options);
    
    this.stats.totalQueries++;
    this.stats.queriesByType[queryType] = (this.stats.queriesByType[queryType] || 0) + 1;

    // 定义查询操作
    const queryOperation = async (): Promise<NebulaQueryResult> => {
      const startTime = Date.now();

      try {
        this.logger.debug('Executing query', {
          query: query.substring(0, 100),
          queryType,
          hasParams: !!params,
          options: mergedOptions
        });

        // 检查缓存
        if (mergedOptions.useCache && this.config.enableCache) {
          const cachedResult = await this.queryCache.get(query, params);
          if (cachedResult) {
            this.stats.cacheHits++;
            this.emit('queryCompleted', {
              query,
              params,
              result: cachedResult,
              fromCache: true
            });
            return cachedResult;
          } else {
            this.stats.cacheMisses++;
          }
        }

        // 执行查询
        const result = await this.executeWithSession(query, params, mergedOptions);
        
        // 缓存结果
        if (mergedOptions.useCache && this.config.enableCache && !result.error) {
          await this.queryCache.set(query, result, params);
        }

        const executionTime = Date.now() - startTime;
        
        this.emit('queryCompleted', {
          query,
          params,
          result,
          executionTime,
          fromCache: false
        });

        return result;
      } catch (error) {
        const executionTime = Date.now() - startTime;
        
        this.emit('queryError', {
          query,
          params,
          error,
          executionTime
        });
        
        // 分类错误并重新抛出
        throw this.classifyAndWrapError(error, 'queryExecution', {
          query: query.substring(0, 100),
          queryType,
          hasParams: !!params
        });
      }
    };

    try {
      // 使用断路器和重试策略执行查询
      const result = await this.circuitBreaker.execute(() =>
        this.retryStrategy.executeWithRetry(queryOperation, {
          operation: 'executeQuery',
          metadata: {
            queryType,
            hasParams: !!params
          }
        })
      );

      // 从结果中获取执行时间
      const executionTime = result.executionTime || 0;
      this.updateStats(executionTime, true);

      return result;
    } catch (error) {
      const executionTime = Date.now() - Date.now(); // 修正时间计算
      this.updateStats(executionTime, false);
      
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.errorHandler.handleError(
        new Error(`Query execution failed: ${errorMessage}`),
        {
          component: 'QueryRunner',
          operation: 'execute',
          query,
          params,
          executionTime
        }
      );
      
      throw error;
    }
  }

  /**
   * 执行批量查询
   */
  async executeBatch(queries: QueryBatch[]): Promise<NebulaQueryResult[]> {
    const startTime = Date.now();
    
    try {
      this.logger.info('Executing batch queries', {
        queryCount: queries.length
      });

      // 创建批处理上下文
      const context: BatchContext = {
        domain: 'database',
        subType: 'nebula',
        metadata: {
          operationType: 'query',
          queryCount: queries.length,
          databaseType: 'nebula'
        }
      };

      // 使用批处理服务执行查询
      const results = await this.batchProcessingService.executeBatch(
        queries,
        async (batch: QueryBatch[]) => {
          const batchResults: NebulaQueryResult[] = [];
          
          for (const { query, params, options } of batch) {
            const result = await this.execute(query, params, options);
            batchResults.push(result);
          }
          
          return batchResults;
        },
        {
          context,
          enableRetry: true,
          enableMonitoring: true
        } as BatchProcessingOptions
      );

      const executionTime = Date.now() - startTime;
      
      this.emit('batchQueryCompleted', {
        queries,
        results,
        executionTime
      });

      return results;
    } catch (error) {
      const executionTime = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      this.errorHandler.handleError(
        new Error(`Batch query execution failed: ${errorMessage}`),
        {
          component: 'QueryRunner',
          operation: 'executeBatch',
          queries,
          executionTime
        }
      );

      this.emit('batchQueryError', {
        queries,
        error,
        executionTime
      });
      
      // 分类错误并重新抛出
      throw this.classifyAndWrapError(error, 'batchQueryExecution', {
        queryCount: queries.length
      });
    }
  }

  /**
   * 获取缓存的查询结果
   */
  async getCachedResult(queryKey: string): Promise<NebulaQueryResult | null> {
    return await this.queryCache.get(queryKey, undefined);
  }

  /**
   * 设置查询结果到缓存
   */
  async setCachedResult(queryKey: string, result: NebulaQueryResult): Promise<void> {
    await this.queryCache.set(queryKey, result, undefined);
  }

  /**
   * 记录查询指标
   */
  recordQueryMetrics(query: string, duration: number, success: boolean): void {
    const queryType = this.detectQueryType(query);
    
    // 记录查询执行时间
    this.performanceMonitor.recordQueryExecution(duration);
    
    // 更新缓存命中率（假设查询会使用缓存）
    // 这里我们无法准确知道是否命中缓存，所以不更新缓存命中率

    this.updateStats(duration, success);
  }

  /**
   * 获取统计信息
   */
  getStats(): QueryRunnerStats {
    // 更新缓存统计
    const cacheStats = this.queryCache.getStats();
    this.stats.cacheHits = cacheStats.hits;
    this.stats.cacheMisses = cacheStats.misses;

    return { ...this.stats };
  }

  /**
   * 清空查询缓存
   */
  clearCache(): void {
    this.queryCache.clear();
    this.logger.info('Query cache cleared');
  }

  /**
   * 更新配置
   */
  updateConfig(config: Partial<QueryRunnerConfig>): void {
    this.config = { ...this.config, ...config };
    
    // 更新缓存配置
    if (config.cacheConfig) {
      this.queryCache.updateConfig(config.cacheConfig);
    }
    
    this.logger.info('QueryRunner configuration updated', { config: this.config });
  }

  /**
   * 使用会话执行查询
   */
  private async executeWithSession(
    query: string,
    params?: Record<string, any>,
    options?: QueryOptions
  ): Promise<NebulaQueryResult> {
    let session: Session | null = null;
    const startTime = Date.now();
    
    try {
      // 获取会话
      session = await this.sessionManager.getSession(options?.spaceName);
      
      // 准备查询（参数插值）
      const preparedQuery = this.prepareQuery(query, params);
      
      // 执行查询
      const result = await session.execute(preparedQuery);
      
      // 记录执行时间
      const executionTime = Date.now() - startTime;
      
      // 释放会话
      this.sessionManager.releaseSession(session);
      
      return {
        ...result,
        executionTime
      };
    } catch (error) {
      // 记录执行时间
      const executionTime = Date.now() - startTime;
      
      // 确保会话被释放
      if (session) {
        try {
          this.sessionManager.releaseSession(session);
        } catch (releaseError) {
          this.logger.error('Failed to release session', {
            sessionId: session.getId(),
            error: releaseError instanceof Error ? releaseError.message : String(releaseError)
          });
        }
      }
      
      // 分类错误并重新抛出
      throw this.classifyAndWrapError(error, 'sessionExecution', {
        query: query.substring(0, 100),
        hasSession: !!session
      });
    }
  }

  /**
   * 准备查询（参数插值）
   */
  private prepareQuery(query: string, params?: Record<string, any>): string {
    if (!params || Object.keys(params).length === 0) {
      return query;
    }

    // 简单的参数插值
    let preparedQuery = query;
    for (const [key, value] of Object.entries(params)) {
      const placeholder = new RegExp(`\\$${key}`, 'g');
      const escapedValue = typeof value === 'string' ? `"${value}"` : String(value);
      preparedQuery = preparedQuery.replace(placeholder, escapedValue);
    }

    return preparedQuery;
  }

  /**
   * 检测查询类型
   */
  private detectQueryType(query: string): string {
    const trimmedQuery = query.trim().toUpperCase();
    
    if (trimmedQuery.startsWith('SELECT') || trimmedQuery.startsWith('MATCH') || trimmedQuery.startsWith('LOOKUP') || trimmedQuery.startsWith('GO')) {
      return 'read';
    } else if (trimmedQuery.startsWith('INSERT') || trimmedQuery.startsWith('UPDATE') || trimmedQuery.startsWith('DELETE') || trimmedQuery.startsWith('CREATE') || trimmedQuery.startsWith('DROP') || trimmedQuery.startsWith('ALTER')) {
      return 'write';
    } else if (trimmedQuery.startsWith('USE')) {
      return 'use';
    } else {
      return 'other';
    }
  }

  /**
   * 合并选项
   */
  private mergeOptions(options?: QueryOptions): QueryOptions {
    return {
      timeout: options?.timeout ?? this.config.defaultTimeout,
      retryAttempts: options?.retryAttempts ?? this.config.defaultRetryAttempts,
      useCache: options?.useCache ?? this.config.enableCache,
      spaceName: options?.spaceName
    };
  }

  /**
   * 更新统计信息
   */
  private updateStats(executionTime: number, success: boolean): void {
    this.stats.totalExecutionTime += executionTime;
    
    if (success) {
      this.stats.successfulQueries++;
    } else {
      this.stats.failedQueries++;
    }
    
    // 计算平均执行时间
    if (this.stats.totalQueries > 0) {
      this.stats.averageExecutionTime = this.stats.totalExecutionTime / this.stats.totalQueries;
    }
  }

  /**
   * 生成查询哈希 - 使用FNV-1a算法（中等抗碰撞能力）
   * 选择FNV-1a而非SHA256的原因：
   * - 查询缓存键需要中等抗碰撞能力，不需要加密级别的哈希
   * - FNV-1a具有更好的分布特性，比简单哈希碰撞率低
   * - 性能更好，计算速度快，适合频繁的缓存键生成
   * - 36进制输出长度相对较短，节省存储空间
   */
  private generateQueryHash(query: string): string {
    // 标准化查询（移除多余空格，转换为小写）
    const normalizedQuery = query.trim().replace(/\s+/g, ' ').toLowerCase();
    
    // 使用HashUtils中的FNV-1a算法
    return HashUtils.fnv1aHash(normalizedQuery);
  }

  /**
   * 分类和包装错误
   */
  private classifyAndWrapError(error: any, operation: string, context?: Record<string, any>): Error {
    const errorMessage = error instanceof Error ? error.message : String(error);
    const errorName = error instanceof Error ? error.name : 'UnknownError';
    
    // 根据错误消息分类
    if (errorMessage.includes('ECONN') || errorMessage.includes('connection') || errorMessage.includes('timeout')) {
      // 连接错误
      const connectionError = new Error(`Connection error during ${operation}: ${errorMessage}`);
      (connectionError as any).type = 'CONNECTION_ERROR';
      (connectionError as any).originalError = error;
      (connectionError as any).context = context;
      return connectionError;
    } else if (errorMessage.includes('syntax') || errorMessage.includes('Syntax')) {
      // 语法错误
      const syntaxError = new Error(`Syntax error in query during ${operation}: ${errorMessage}`);
      (syntaxError as any).type = 'SYNTAX_ERROR';
      (syntaxError as any).originalError = error;
      (syntaxError as any).context = context;
      return syntaxError;
    } else if (errorMessage.includes('permission') || errorMessage.includes('access') || errorMessage.includes('auth')) {
      // 权限错误
      const authError = new Error(`Authorization error during ${operation}: ${errorMessage}`);
      (authError as any).type = 'AUTHORIZATION_ERROR';
      (authError as any).originalError = error;
      (authError as any).context = context;
      return authError;
    } else if (errorMessage.includes('not found') || errorMessage.includes('NotFound')) {
      // 资源未找到错误
      const notFoundError = new Error(`Resource not found during ${operation}: ${errorMessage}`);
      (notFoundError as any).type = 'NOT_FOUND_ERROR';
      (notFoundError as any).originalError = error;
      (notFoundError as any).context = context;
      return notFoundError;
    } else if (errorMessage.includes('duplicate') || errorMessage.includes('conflict')) {
      // 冲突错误
      const conflictError = new Error(`Conflict error during ${operation}: ${errorMessage}`);
      (conflictError as any).type = 'CONFLICT_ERROR';
      (conflictError as any).originalError = error;
      (conflictError as any).context = context;
      return conflictError;
    } else {
      // 未知错误
      const unknownError = new Error(`Unknown error during ${operation}: ${errorMessage}`);
      (unknownError as any).type = 'UNKNOWN_ERROR';
      (unknownError as any).originalError = error;
      (unknownError as any).context = context;
      return unknownError;
    }
  }
}