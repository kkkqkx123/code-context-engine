import { injectable, inject } from 'inversify';
import { EventEmitter } from 'events';
import { TYPES } from '../../../types';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { PerformanceMonitor } from '../../../infrastructure/monitoring/PerformanceMonitor';
import { ConnectionPool } from './ConnectionPool';
import { Connection } from './Connection';

// 连接预热配置
export interface ConnectionWarmingConfig {
  enabled: boolean;
  warmupQueries: string[];
  warmupConcurrency: number;
  warmupTimeout: number;
  retryAttempts: number;
  retryDelay: number;
}

// 预热结果
export interface WarmingResult {
  connectionId: string;
  success: boolean;
  error?: string;
  warmupTime: number;
  queryResults: Array<{
    query: string;
    success: boolean;
    executionTime: number;
    error?: string;
  }>;
}

// 默认预热配置
const DEFAULT_WARMING_CONFIG: ConnectionWarmingConfig = {
  enabled: true,
  warmupQueries: [
    'YIELD 1 AS warmup_test;',
    'SHOW SPACES;',
    'SHOW HOSTS;'
  ],
  warmupConcurrency: 3,
  warmupTimeout: 10000, // 10秒
  retryAttempts: 2,
  retryDelay: 1000
};

/**
 * 连接预热器
 * 负责预热新创建的连接，提高首次查询性能
 */
@injectable()
export class ConnectionWarmer extends EventEmitter {
  private logger: LoggerService;
  private errorHandler: ErrorHandlerService;
  private performanceMonitor: PerformanceMonitor;
  private config: ConnectionWarmingConfig;
  private isWarming: boolean = false;
  private warmingQueue: Connection[] = [];

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.PerformanceMonitor) performanceMonitor: PerformanceMonitor
  ) {
    super();
    this.logger = logger;
    this.errorHandler = errorHandler;
    this.performanceMonitor = performanceMonitor;
    this.config = { ...DEFAULT_WARMING_CONFIG };
  }

  /**
   * 更新预热配置
   */
  updateConfig(config: Partial<ConnectionWarmingConfig>): void {
    this.config = { ...this.config, ...config };
    this.logger.info('Connection warming config updated', { config: this.config });
  }

  /**
   * 预热连接
   */
  async warmConnection(connection: Connection): Promise<WarmingResult> {
    if (!this.config.enabled) {
      return {
        connectionId: connection.getId(),
        success: true,
        warmupTime: 0,
        queryResults: []
      };
    }

    const startTime = Date.now();
    const result: WarmingResult = {
      connectionId: connection.getId(),
      success: false,
      warmupTime: 0,
      queryResults: []
    };

    try {
      this.logger.debug('Starting connection warming', {
        connectionId: connection.getId()
      });

      // 执行预热查询
      for (const query of this.config.warmupQueries) {
        const queryStartTime = Date.now();
        let querySuccess = false;
        let queryError: string | undefined;

        try {
          await this.executeWithRetry(connection, query);
          querySuccess = true;
        } catch (error) {
          queryError = error instanceof Error ? error.message : String(error);
          this.logger.warn('Warmup query failed', {
            connectionId: connection.getId(),
            query,
            error: queryError
          });
        }

        result.queryResults.push({
          query,
          success: querySuccess,
          executionTime: Date.now() - queryStartTime,
          error: queryError
        });
      }

      // 检查是否所有查询都成功
      result.success = result.queryResults.every(qr => qr.success);
      result.warmupTime = Date.now() - startTime;

      // 记录性能指标
      const operationId = this.performanceMonitor.startOperation('connection_warming', {
        connectionId: connection.getId(),
        success: result.success,
        queryCount: result.queryResults.length,
        successRate: result.queryResults.filter(qr => qr.success).length / result.queryResults.length
      });
      this.performanceMonitor.endOperation(operationId, {
        success: result.success,
        duration: result.warmupTime
      });

      this.emit('connectionWarmed', result);
      
      this.logger.debug('Connection warming completed', {
        connectionId: connection.getId(),
        success: result.success,
        warmupTime: result.warmupTime
      });

      return result;
    } catch (error) {
      result.warmupTime = Date.now() - startTime;
      result.error = error instanceof Error ? error.message : String(error);
      
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Connection warming failed'),
        { component: 'ConnectionWarmer', operation: 'warmConnection', connectionId: connection.getId() }
      );

      this.emit('warmingError', result);
      return result;
    }
  }

  /**
   * 批量预热连接
   */
  async warmConnections(connections: Connection[]): Promise<WarmingResult[]> {
    if (!this.config.enabled) {
      return connections.map(conn => ({
        connectionId: conn.getId(),
        success: true,
        warmupTime: 0,
        queryResults: []
      }));
    }

    this.logger.info('Starting batch connection warming', {
      connectionCount: connections.length,
      concurrency: this.config.warmupConcurrency
    });

    const results: WarmingResult[] = [];
    const chunks = this.chunkArray(connections, this.config.warmupConcurrency);

    for (const chunk of chunks) {
      const chunkPromises = chunk.map(conn => this.warmConnection(conn));
      const chunkResults = await Promise.allSettled(chunkPromises);
      
      chunkResults.forEach(result => {
        if (result.status === 'fulfilled') {
          results.push(result.value);
        } else {
          this.logger.error('Connection warming failed', {
            error: result.reason
          });
        }
      });
    }

    const successCount = results.filter(r => r.success).length;
    this.logger.info('Batch connection warming completed', {
      totalConnections: connections.length,
      successCount,
      failureCount: connections.length - successCount,
      averageWarmupTime: results.reduce((sum, r) => sum + r.warmupTime, 0) / results.length
    });

    return results;
  }

  /**
   * 带重试的查询执行
   */
  private async executeWithRetry(connection: Connection, query: string): Promise<any> {
    let lastError: Error | undefined;

    for (let attempt = 1; attempt <= this.config.retryAttempts; attempt++) {
      try {
        return await connection.execute(query);
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));
        
        if (attempt < this.config.retryAttempts) {
          this.logger.debug('Warmup query retry', {
            connectionId: connection.getId(),
            query,
            attempt,
            error: lastError.message
          });
          
          await this.delay(this.config.retryDelay);
        }
      }
    }

    throw lastError;
  }

  /**
   * 将数组分割成指定大小的块
   */
  private chunkArray<T>(array: T[], chunkSize: number): T[][] {
    const chunks: T[][] = [];
    for (let i = 0; i < array.length; i += chunkSize) {
      chunks.push(array.slice(i, i + chunkSize));
    }
    return chunks;
  }

  /**
   * 延迟函数
   */
  private delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}