import { injectable, inject } from 'inversify';
import { LoggerService } from '../../utils/LoggerService';
import { ConfigService } from '../../config/ConfigService';
import { TYPES } from '../../types';
import { DatabaseEvent } from './DatabaseEventTypes';

/**
 * 数据库专用日志服务
 * 提供统一的数据库操作日志记录功能
 */
@injectable()
export class DatabaseLoggerService {
  private loggerService: LoggerService;
  private configService: ConfigService;
  private databaseLogLevel: string;

  constructor(
    @inject(TYPES.LoggerService) loggerService: LoggerService,
    @inject(TYPES.ConfigService) configService: ConfigService
  ) {
    this.loggerService = loggerService;
    this.configService = configService;
    
    // 获取通用日志级别配置，如果没有配置则使用默认级别
    try {
      this.databaseLogLevel = this.configService.get('logging')?.level || 'info';
    } catch (error) {
      this.databaseLogLevel = 'info';
    }
  }

  /**
   * 记录数据库连接事件
   * @param operation 操作类型
   * @param status 状态
   * @param details 详细信息
   */
  async logConnectionEvent(operation: string, status: 'success' | 'failed', details: any): Promise<void> {
    const message = `Database ${operation} ${status}`;
    if (status === 'success') {
      await this.loggerService.info(message, details);
    } else {
      await this.loggerService.error(message, details);
    }
  }

  /**
   * 记录查询性能
   * @param query 查询语句
   * @param duration 执行时间（毫秒）
   * @param resultCount 结果数量
   */
  async logQueryPerformance(query: string, duration: number, resultCount?: number): Promise<void> {
    const message = `Query executed in ${duration}ms`;
    const meta = { query, resultCount };
    
    // 如果执行时间超过阈值，记录为警告
    const threshold = this.getPerformanceThreshold();
    if (duration > threshold) {
      await this.loggerService.warn(message, meta);
    } else {
      await this.loggerService.debug(message, meta);
    }
  }

  /**
   * 记录批量操作
   * @param operation 操作类型
   * @param batchSize 批量大小
   * @param performance 性能数据
   */
  async logBatchOperation(operation: string, batchSize: number, performance: any): Promise<void> {
    const message = `Batch ${operation} completed`;
    const meta = { batchSize, ...performance };
    await this.loggerService.info(message, meta);
  }

  /**
   * 记录数据库事件
   * @param event 数据库事件
   */
  async logDatabaseEvent(event: DatabaseEvent): Promise<void> {
    const level = this.getLogLevelForEvent(event.type);
    const message = `Database event: ${event.type}`;
    const meta = {
      source: event.source,
      timestamp: event.timestamp,
      data: event.data,
      error: event.error ? event.error.message : undefined
    };

    switch (level) {
      case 'error':
        await this.loggerService.error(message, meta);
        break;
      case 'warn':
        await this.loggerService.warn(message, meta);
        break;
      case 'info':
        await this.loggerService.info(message, meta);
        break;
      case 'debug':
        await this.loggerService.debug(message, meta);
        break;
      default:
        await this.loggerService.info(message, meta);
    }
  }

  /**
   * 根据事件类型获取日志级别
   * @param eventType 事件类型
   * @returns 日志级别
   */
  private getLogLevelForEvent(eventType: string): string {
    // 对于数据库事件，我们不依赖配置，而是使用内置的映射
    // 因为LoggingConfig不包含数据库特定的事件配置

    // 默认事件类型到日志级别的映射
    const defaultMapping: Record<string, string> = {
      'connection_opened': 'info',
      'connection_closed': 'info',
      'connection_failed': 'error',
      'connection_error': 'error',
      'space_created': 'info',
      'space_deleted': 'info',
      'space_error': 'error',
      'data_inserted': 'debug',
      'data_updated': 'debug',
      'data_deleted': 'debug',
      'data_queried': 'debug',
      'data_error': 'error',
      'service_initialized': 'info',
      'service_error': 'error',
      'performance_metric': 'info',
      'query_executed': 'debug',
      'batch_operation_completed': 'info',
      'error_occurred': 'error'
    };

    return defaultMapping[eventType] || this.databaseLogLevel;
  }

  /**
   * 获取性能阈值
   * @returns 性能阈值（毫秒）
   */
  private getPerformanceThreshold(): number {
    // 使用固定阈值，因为LoggingConfig不包含性能阈值设置
    return 1000;
  }

  /**
   * 记录集合操作日志
   * @param operation 操作类型
   * @param projectPath 项目路径
   * @param status 操作状态
   * @param details 详细信息
   */
  async logCollectionOperation(operation: string, projectPath: string, status: 'success' | 'failed', details: any): Promise<void> {
    const message = `Collection ${operation} for project ${projectPath} ${status}`;
    if (status === 'success') {
      await this.loggerService.info(message, details);
    } else {
      await this.loggerService.error(message, details);
    }
  }

  /**
   * 记录向量操作日志
   * @param operation 操作类型
   * @param projectPath 项目路径
   * @param status 操作状态
   * @param details 详细信息
   */
  async logVectorOperation(operation: string, projectPath: string, status: 'success' | 'failed', details: any): Promise<void> {
    const message = `Vector ${operation} for project ${projectPath} ${status}`;
    if (status === 'success') {
      await this.loggerService.info(message, details);
    } else {
      await this.loggerService.error(message, details);
    }
  }

  /**
   * 记录查询操作日志
   * @param operation 操作类型
   * @param projectPath 项目路径
   * @param status 操作状态
   * @param details 详细信息
   */
  async logQueryOperation(operation: string, projectPath: string, status: 'success' | 'failed', details: any): Promise<void> {
    const message = `Query ${operation} for project ${projectPath} ${status}`;
    if (status === 'success') {
      await this.loggerService.info(message, details);
    } else {
      await this.loggerService.error(message, details);
    }
  }

  /**
   * 记录项目操作日志
   * @param operation 操作类型
   * @param projectPath 项目路径
   * @param status 操作状态
   * @param details 详细信息
   */
  async logProjectOperation(operation: string, projectPath: string, status: 'success' | 'failed', details: any): Promise<void> {
    const message = `Project ${operation} for project ${projectPath} ${status}`;
    if (status === 'success') {
      await this.loggerService.info(message, details);
    } else {
      await this.loggerService.error(message, details);
    }
  }

  /**
   * 更新日志级别
   * @param level 新的日志级别
   */
  updateLogLevel(level: string): void {
    this.databaseLogLevel = level;
  }
}