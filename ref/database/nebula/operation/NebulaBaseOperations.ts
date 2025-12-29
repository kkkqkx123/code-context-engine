import { injectable, inject } from 'inversify';
import { TYPES } from '../../../types';
import { DatabaseLoggerService } from '../../common/DatabaseLoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { INebulaSpaceManager } from '../space/NebulaSpaceManager';
import { INebulaQueryService } from '../query/NebulaQueryService';
import { DatabaseError } from '../../common/DatabaseError';
import { DatabaseEventType } from '../../common/DatabaseEventTypes';

/**
 * Nebula操作基类
 * 提供公共的空间管理、参数验证和错误处理功能
 */
@injectable()
export abstract class NebulaBaseOperations {
  protected databaseLogger: DatabaseLoggerService;
  protected errorHandler: ErrorHandlerService;
  protected spaceManager: INebulaSpaceManager;
  protected queryService: INebulaQueryService;

  constructor(
    @inject(TYPES.DatabaseLoggerService) databaseLogger: DatabaseLoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.INebulaSpaceManager) spaceManager: INebulaSpaceManager,
    @inject(TYPES.INebulaQueryService) queryService: INebulaQueryService
  ) {
    this.databaseLogger = databaseLogger;
    this.errorHandler = errorHandler;
    this.spaceManager = spaceManager;
    this.queryService = queryService;
  }

  /**
   * 确保空间存在并切换到该空间
   */
  protected async ensureSpaceAndSwitch(projectId: string, spaceName: string): Promise<void> {
    this.validateSpaceName(spaceName, projectId);
    
    const spaceExists = await this.spaceManager.checkSpaceExists(projectId);
    if (!spaceExists) {
      const created = await this.spaceManager.createSpace(projectId);
      if (!created) {
        throw new Error(`Failed to create space ${spaceName} for project ${projectId}`);
      }
    }
    
    await this.queryService.executeQuery(`USE \`${spaceName}\``);
  }

  /**
   * 验证空间名称
   */
  protected validateSpaceName(spaceName: string, projectId: string): void {
    if (spaceName === 'undefined' || spaceName === '') {
      throw new Error(`Invalid space name for project: ${projectId}`);
    }
  }

  /**
   * 验证必需参数
   */
  protected validateRequiredParams(params: Record<string, any>, paramNames: string[]): void {
    for (const name of paramNames) {
      if (!params[name]) {
        throw new Error(`${name} is required`);
      }
    }
  }

  /**
   * 处理操作错误
   */
  protected handleOperationError(
    error: unknown,
    component: string,
    operation: string,
    details: Record<string, any>
  ): never {
    const dbError = DatabaseError.fromError(
      error instanceof Error ? error : new Error(String(error)),
      { component, operation, details }
    );
    
    this.errorHandler.handleError(dbError, dbError.context);
    throw error;
  }

  /**
   * 为数据添加项目ID
   */
  protected addProjectId<T extends { properties?: Record<string, any> }>(
    items: T[],
    projectId: string
  ): T[] {
    return items.map(item => ({
      ...item,
      properties: {
        ...item.properties,
        projectId
      }
    }));
  }

  /**
   * 格式化查询值
   */
  protected formatValue(value: any): string {
    return typeof value === 'string' ? `"${value}"` : String(value);
  }

  /**
   * 记录成功操作日志
   */
  protected logSuccess(operation: string, data: Record<string, any>): void {
    this.databaseLogger.logDatabaseEvent({
      type: DatabaseEventType.QUERY_EXECUTED,
      source: 'nebula',
      timestamp: new Date(),
      data: { operation, ...data }
    }).catch(err => console.error('Failed to log success:', err));
  }

  /**
   * 记录错误操作日志
   */
  protected logError(operation: string, data: Record<string, any>, error: unknown): void {
    this.databaseLogger.logDatabaseEvent({
      type: DatabaseEventType.ERROR_OCCURRED,
      source: 'nebula',
      timestamp: new Date(),
      data: {
        operation,
        ...data,
        error: error instanceof Error ? error.message : String(error),
        stack: error instanceof Error ? error.stack : undefined
      }
    }).catch(err => console.error('Failed to log error:', err));
  }
}