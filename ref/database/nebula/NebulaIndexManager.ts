import { injectable, inject } from 'inversify';
import { DatabaseLoggerService } from '../common/DatabaseLoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { TYPES } from '../../types';
import { INebulaQueryService } from './query/NebulaQueryService';
import { DatabaseError } from '../common/DatabaseError';

/**
 * Nebula 索引管理器接口
 */
export interface INebulaIndexManager {
  /**
   * 为标签创建索引
   */
  createTagIndex(projectId: string, tagName: string, propertyName: string): Promise<boolean>;

  /**
   * 为边类型创建索引
   */
  createEdgeIndex(projectId: string, edgeName: string, propertyName: string): Promise<boolean>;

  /**
   * 重建标签索引
   */
  rebuildTagIndex(projectId: string, tagName: string): Promise<boolean>;

  /**
   * 重建边类型索引
   */
  rebuildEdgeIndex(projectId: string, edgeName: string): Promise<boolean>;

  /**
   * 检查标签索引是否存在
   */
  tagIndexExists(projectId: string, tagName: string, propertyName: string): Promise<boolean>;

  /**
   * 检查边类型索引是否存在
   */
  edgeIndexExists(projectId: string, edgeName: string, propertyName: string): Promise<boolean>;

  /**
   * 删除标签索引
   */
  dropTagIndex(projectId: string, tagName: string): Promise<boolean>;

  /**
   * 删除边类型索引
   */
  dropEdgeIndex(projectId: string, edgeName: string): Promise<boolean>;

  /**
   * 获取所有标签索引
   */
  getAllTagIndexes(projectId: string): Promise<any[]>;

  /**
   * 获取所有边类型索引
   */
  getAllEdgeIndexes(projectId: string): Promise<any[]>;
}

/**
 * Nebula 索引管理器实现
 * 
 * 负责管理图数据库的索引，提供索引创建、重建、删除等功能
 */
@injectable()
export class NebulaIndexManager implements INebulaIndexManager {
  private databaseLogger: DatabaseLoggerService;
  private errorHandler: ErrorHandlerService;
  private queryService: INebulaQueryService;

  constructor(
    @inject(TYPES.DatabaseLoggerService) databaseLogger: DatabaseLoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.INebulaQueryService) queryService: INebulaQueryService
  ) {
    this.databaseLogger = databaseLogger;
    this.errorHandler = errorHandler;
    this.queryService = queryService;
  }

  /**
   * 为标签创建索引
   */
  async createTagIndex(projectId: string, tagName: string, propertyName: string): Promise<boolean> {
    try {
      // 检查索引是否已存在
      if (await this.tagIndexExists(projectId, tagName, propertyName)) {
        return true;
      }

      const query = `CREATE TAG INDEX \`${tagName}_${propertyName}_index\` ON \`${tagName}\` (\`${propertyName}\`)`;
      await this.queryService.executeQuery(query);

      // 等待索引构建完成（在实际应用中可能需要更复杂的逻辑来检查索引状态）
      await this.rebuildTagIndex(projectId, tagName);

      return true;
    } catch (error) {
      // 忽略"already exists"错误
      if (error instanceof Error && error.message.includes('already exist')) {
        return true;
      }

      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaIndexManager',
          operation: 'createTagIndex',
          details: { projectId, tagName, propertyName }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      return false;
    }
  }

  /**
   * 为边类型创建索引
   */
  async createEdgeIndex(projectId: string, edgeName: string, propertyName: string): Promise<boolean> {
    try {
      // 检查索引是否已存在
      if (await this.edgeIndexExists(projectId, edgeName, propertyName)) {
        return true;
      }

      const query = `CREATE EDGE INDEX \`${edgeName}_${propertyName}_index\` ON \`${edgeName}\` (\`${propertyName}\`)`;
      await this.queryService.executeQuery(query);

      // 等待索引构建完成（在实际应用中可能需要更复杂的逻辑来检查索引状态）
      await this.rebuildEdgeIndex(projectId, edgeName);

      return true;
    } catch (error) {
      // 忽略"already exists"错误
      if (error instanceof Error && error.message.includes('already exist')) {
        return true;
      }

      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaIndexManager',
          operation: 'createEdgeIndex',
          details: { projectId, edgeName, propertyName }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      return false;
    }
  }

  /**
   * 重建标签索引
   */
  async rebuildTagIndex(projectId: string, tagName: string): Promise<boolean> {
    try {
      const query = `REBUILD TAG INDEX \`${tagName}\``;
      await this.queryService.executeQuery(query);

      // 可能需要等待重建完成，或者检查重建状态
      return true;
    } catch (error) {
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaIndexManager',
          operation: 'rebuildTagIndex',
          details: { projectId, tagName }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      return false;
    }
  }

  /**
   * 重建边类型索引
   */
  async rebuildEdgeIndex(projectId: string, edgeName: string): Promise<boolean> {
    try {
      const query = `REBUILD EDGE INDEX \`${edgeName}\``;
      await this.queryService.executeQuery(query);

      // 可能需要等待重建完成，或者检查重建状态
      return true;
    } catch (error) {
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaIndexManager',
          operation: 'rebuildEdgeIndex',
          details: { projectId, edgeName }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      return false;
    }
  }

  /**
   * 检查标签索引是否存在
   */
  async tagIndexExists(projectId: string, tagName: string, propertyName: string): Promise<boolean> {
    try {
      const allIndexes = await this.getAllTagIndexes(projectId);
      const expectedIndexName = `${tagName}_${propertyName}_index`;
      return allIndexes.some(index =>
        index.Name === expectedIndexName ||
        index['Name'] === expectedIndexName ||
        index.name === expectedIndexName ||
        index['name'] === expectedIndexName
      );
    } catch (error) {
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaIndexManager',
          operation: 'tagIndexExists',
          details: { projectId, tagName, propertyName }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      return false;
    }
  }

  /**
   * 检查边类型索引是否存在
   */
  async edgeIndexExists(projectId: string, edgeName: string, propertyName: string): Promise<boolean> {
    try {
      const allIndexes = await this.getAllEdgeIndexes(projectId);
      const expectedIndexName = `${edgeName}_${propertyName}_index`;
      return allIndexes.some(index =>
        index.Name === expectedIndexName ||
        index['Name'] === expectedIndexName ||
        index.name === expectedIndexName ||
        index['name'] === expectedIndexName
      );
    } catch (error) {
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaIndexManager',
          operation: 'edgeIndexExists',
          details: { projectId, edgeName, propertyName }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      return false;
    }
  }

  /**
   * 删除标签索引
   */
  async dropTagIndex(projectId: string, tagName: string): Promise<boolean> {
    try {
      const indexName = `${tagName}_index`; // 使用默认索引名
      const query = `DROP TAG INDEX \`${indexName}\``;
      await this.queryService.executeQuery(query);

      return true;
    } catch (error) {
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaIndexManager',
          operation: 'dropTagIndex',
          details: { projectId, tagName }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      return false;
    }
  }

  /**
   * 删除边类型索引
   */
  async dropEdgeIndex(projectId: string, edgeName: string): Promise<boolean> {
    try {
      const indexName = `${edgeName}_index`; // 使用默认索引名
      const query = `DROP EDGE INDEX \`${indexName}\``;
      await this.queryService.executeQuery(query);

      return true;
    } catch (error) {
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaIndexManager',
          operation: 'dropEdgeIndex',
          details: { projectId, edgeName }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      return false;
    }
  }

  /**
   * 获取所有标签索引
   */
  async getAllTagIndexes(projectId: string): Promise<any[]> {
    try {
      const result = await this.queryService.executeQuery('SHOW TAG INDEXES');
      return result.data || [];
    } catch (error) {
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaIndexManager',
          operation: 'getAllTagIndexes',
          details: { projectId }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      return [];
    }
  }

  /**
   * 获取所有边类型索引
   */
  async getAllEdgeIndexes(projectId: string): Promise<any[]> {
    try {
      const result = await this.queryService.executeQuery('SHOW EDGE INDEXES');
      return result.data || [];
    } catch (error) {
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaIndexManager',
          operation: 'getAllEdgeIndexes',
          details: { projectId }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      return [];
    }
  }
}