import { injectable, inject } from 'inversify';
import { TYPES } from '../../../types';
import { DatabaseLoggerService } from '../../common/DatabaseLoggerService';
import { DatabaseEventType } from '../../common/DatabaseEventTypes';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { NebulaQueryResult, NebulaConfig } from '../NebulaTypes';
import { PerformanceMonitor } from '../../../infrastructure/monitoring/PerformanceMonitor';
import { NebulaConfigService } from '../../../config/service/NebulaConfigService';
import { INebulaConnectionManager } from '../NebulaConnectionManager';
import { IQueryRunner } from './QueryRunner';

export interface INebulaQueryService {
  executeQuery(nGQL: string, parameters?: Record<string, any>): Promise<NebulaQueryResult>;
  executeParameterizedQuery(nGQL: string, parameters?: Record<string, any>): Promise<NebulaQueryResult>;
  prepareQuery(nGQL: string, parameters?: Record<string, any>): string;
  formatResults(result: any, executionTime: number): NebulaQueryResult;
  validateQuery(nGQL: string): boolean;
  executeBatch(queries: Array<{ query: string; params?: Record<string, any> }>): Promise<NebulaQueryResult[]>;
}

@injectable()
export class NebulaQueryService implements INebulaQueryService {
  private databaseLogger: DatabaseLoggerService;
  private errorHandler: ErrorHandlerService;
  private performanceMonitor: PerformanceMonitor;
  private configService: NebulaConfigService;
  private connectionManager: INebulaConnectionManager;
  private queryRunner: IQueryRunner;
  private isInitialized: boolean = false;

  constructor(
    @inject(TYPES.DatabaseLoggerService) databaseLogger: DatabaseLoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.PerformanceMonitor) performanceMonitor: PerformanceMonitor,
    @inject(TYPES.NebulaConfigService) configService: NebulaConfigService,
    @inject(TYPES.INebulaConnectionManager) connectionManager: INebulaConnectionManager,
    @inject(TYPES.IQueryRunner) queryRunner: IQueryRunner
  ) {
    this.databaseLogger = databaseLogger;
    this.errorHandler = errorHandler;
    this.performanceMonitor = performanceMonitor;
    this.configService = configService;
    this.connectionManager = connectionManager;
    this.queryRunner = queryRunner;
  }

  /**
   * 确保连接已建立
   */
  private async ensureConnection(): Promise<void> {
    if (!this.isInitialized) {
      await this.connectionManager.connect();
      this.isInitialized = true;
    }

    // 使用connectionManager的isConnected()检查连接状态
    if (!this.connectionManager.isConnected()) {
      await this.connectionManager.connect();
    }
  }

  async executeQuery(nGQL: string, parameters?: Record<string, any>): Promise<NebulaQueryResult> {
    // 确保连接已建立
    await this.ensureConnection();
    
    // 委托给新的QueryRunner
    return await this.queryRunner.execute(nGQL, parameters);
  }

  async executeParameterizedQuery(nGQL: string, parameters?: Record<string, any>): Promise<NebulaQueryResult> {
    // 简化实现，直接调用executeQuery
    return this.executeQuery(nGQL, parameters);
  }

  prepareQuery(nGQL: string, parameters?: Record<string, any>): string {
    if (!parameters || Object.keys(parameters).length === 0) {
      return nGQL;
    }

    // 简单的参数插值，实际实现可能需要更复杂的参数处理
    let preparedQuery = nGQL;
    for (const [key, value] of Object.entries(parameters)) {
      const placeholder = new RegExp(`\\$${key}`, 'g');
      const escapedValue = typeof value === 'string' ? `"${value}"` : String(value);
      preparedQuery = preparedQuery.replace(placeholder, escapedValue);
    }

    return preparedQuery;
  }

  formatResults(result: any, executionTime: number): NebulaQueryResult {
    return {
      table: result?.table || {},
      results: result?.results || [],
      rows: result?.rows || [],
      data: result?.data || [],
      executionTime,
      timeCost: result?.timeCost || 0,
      space: result?.space,
      error: result?.error || undefined
    };
  }


  validateQuery(nGQL: string): boolean {
    if (!nGQL || typeof nGQL !== 'string' || nGQL.trim() === '') {
      return false;
    }

    // 简单的查询验证
    const trimmedQuery = nGQL.trim();
    const uppercaseQuery = trimmedQuery.toUpperCase();

    // 检查是否是不允许的查询类型（如DROP SPACE等）
    if (uppercaseQuery.startsWith('DROP SPACE') ||
      uppercaseQuery.startsWith('USE ') && trimmedQuery.includes('undefined')) {
      return false;
    }

    return true;
  }

  async executeBatch(queries: Array<{ query: string; params?: Record<string, any> }>): Promise<NebulaQueryResult[]> {
    // 转换查询格式以匹配QueryRunner的接口
    const queryBatches = queries.map(q => ({
      query: q.query,
      params: q.params,
      options: undefined
    }));

    // 委托给新的QueryRunner
    return await this.queryRunner.executeBatch(queryBatches);
  }
}