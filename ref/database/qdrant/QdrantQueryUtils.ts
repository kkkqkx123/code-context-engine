import { injectable, inject } from 'inversify';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { TYPES } from '../../types';
import { IQdrantConnectionManager } from './QdrantConnectionManager';
import { SearchOptions } from './IVectorStore';
import { DatabaseLoggerService } from '../common/DatabaseLoggerService';
import { PerformanceMonitor } from '../../infrastructure/monitoring/PerformanceMonitor';
import { DatabaseEventType } from '../common/DatabaseEventTypes';
import {
  QueryFilter,
  ERROR_MESSAGES,
  QdrantEventType,
  QdrantEvent
} from './QdrantTypes';

/**
 * Qdrant 查询工具接口
 */
export interface IQdrantQueryUtils {
  buildFilter(filter: SearchOptions['filter']): any;
  buildAdvancedFilter(filter: QueryFilter | undefined): any;
  getChunkIdsByFiles(collectionName: string, filePaths: string[]): Promise<string[]>;
  getExistingChunkIds(collectionName: string, chunkIds: string[]): Promise<string[]>;
  scrollPoints(collectionName: string, filter?: any, limit?: number, offset?: any): Promise<any[]>;
  countPoints(collectionName: string, filter?: any): Promise<number>;
  subscribe(type: QdrantEventType, listener: (event: QdrantEvent) => void): { id: string; eventType: string; handler: any; unsubscribe: () => void };
}

/**
 * Qdrant 查询工具实现
 * 
 * 负责构建查询过滤器、提供查询辅助功能、处理复杂查询逻辑
 */
@injectable()
export class QdrantQueryUtils implements IQdrantQueryUtils {
  private logger: LoggerService;
  private errorHandler: ErrorHandlerService;
  private databaseLogger: DatabaseLoggerService;
  private performanceMonitor: PerformanceMonitor;
  private connectionManager: IQdrantConnectionManager;
  private eventListeners: Map<QdrantEventType, ((event: QdrantEvent) => void)[]> = new Map();

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.IQdrantConnectionManager) connectionManager: IQdrantConnectionManager,
    @inject(TYPES.DatabaseLoggerService) databaseLogger: DatabaseLoggerService,
    @inject(TYPES.PerformanceMonitor) performanceMonitor: PerformanceMonitor
  ) {
    this.logger = logger;
    this.errorHandler = errorHandler;
    this.connectionManager = connectionManager;
    this.databaseLogger = databaseLogger;
    this.performanceMonitor = performanceMonitor;
  }

  /**
   * 构建查询过滤器
   */
  buildFilter(filter: SearchOptions['filter']): any {
    if (!filter) return undefined;

    const must: any[] = [];

    if (filter.language) {
      must.push({
        key: 'language',
        match: {
          any: Array.isArray(filter.language) ? filter.language : [filter.language],
        },
      });
    }

    if (filter.chunkType) {
      must.push({
        key: 'chunkType',
        match: {
          any: Array.isArray(filter.chunkType) ? filter.chunkType : [filter.chunkType],
        },
      });
    }

    if (filter.filePath) {
      must.push({
        key: 'filePath',
        match: {
          any: Array.isArray(filter.filePath) ? filter.filePath : [filter.filePath],
        },
      });
    }

    if (filter.projectId) {
      must.push({
        key: 'projectId',
        match: {
          value: filter.projectId,
        },
      });
    }

    if (filter.snippetType) {
      must.push({
        key: 'snippetMetadata.snippetType',
        match: {
          any: Array.isArray(filter.snippetType) ? filter.snippetType : [filter.snippetType],
        },
      });
    }

    return must.length > 0 ? { must } : undefined;
  }

  /**
   * 构建高级查询过滤器
   */
  buildAdvancedFilter(filter: QueryFilter | undefined): any {
    if (!filter) return undefined;

    const conditions: any[] = [];

    // 处理语言过滤
    if (filter.language && filter.language.length > 0) {
      conditions.push({
        key: 'language',
        match: {
          any: filter.language,
        },
      });
    }

    // 处理块类型过滤
    if (filter.chunkType && filter.chunkType.length > 0) {
      conditions.push({
        key: 'chunkType',
        match: {
          any: filter.chunkType,
        },
      });
    }

    // 处理文件路径过滤
    if (filter.filePath && filter.filePath.length > 0) {
      conditions.push({
        key: 'filePath',
        match: {
          any: filter.filePath,
        },
      });
    }

    // 处理项目ID过滤
    if (filter.projectId) {
      conditions.push({
        key: 'projectId',
        match: {
          value: filter.projectId,
        },
      });
    }

    // 处理代码片段类型过滤
    if (filter.snippetType && filter.snippetType.length > 0) {
      conditions.push({
        key: 'snippetMetadata.snippetType',
        match: {
          any: filter.snippetType,
        },
      });
    }

    // 处理自定义过滤器
    if (filter.customFilters) {
      for (const [key, value] of Object.entries(filter.customFilters)) {
        if (Array.isArray(value)) {
          conditions.push({
            key,
            match: {
              any: value,
            },
          });
        } else if (typeof value === 'object' && value !== null) {
          // 处理范围查询
          if ('gte' in value || 'lte' in value || 'gt' in value || 'lt' in value) {
            conditions.push({
              key,
              range: value,
            });
          }
          // 处理嵌套匹配
          else if ('match' in value) {
            conditions.push({
              key,
              match: value.match,
            });
          }
          // 处理嵌套过滤
          else if ('filter' in value) {
            conditions.push({
              key,
              filter: value.filter,
            });
          }
        } else {
          // 简单值匹配
          conditions.push({
            key,
            match: {
              value: value,
            },
          });
        }
      }
    }

    return conditions.length > 0 ? { must: conditions } : undefined;
  }

  /**
   * 根据文件路径获取块ID
   */
  async getChunkIdsByFiles(collectionName: string, filePaths: string[]): Promise<string[]> {
    try {
      const client = this.connectionManager.getClient();
      if (!client) {
        throw new Error(ERROR_MESSAGES.CONNECTION_FAILED);
      }

      // 构建过滤器以匹配任何提供的文件路径
      const filter = {
        must: [
          {
            key: 'filePath',
            match: {
              any: filePaths,
            },
          },
        ],
      };

      // 搜索匹配过滤器的点，只检索ID
      const results = await client.scroll(collectionName, {
        filter,
        with_payload: false,
        with_vector: false,
        limit: 1000, // 根据需要调整此限制
      });

      // 从结果中提取ID
      const chunkIds = results.points.map(point => point.id as string);

      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SERVICE_INITIALIZED,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'get_chunk_ids_by_files_completed',
          collectionName,
          fileCount: filePaths.length,
          chunkCount: chunkIds.length
        }
      });

      return chunkIds;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(
          `Failed to get chunk IDs by files from ${collectionName}: ${error instanceof Error ? error.message : String(error)}`
        ),
        { component: 'QdrantQueryUtils', operation: 'getChunkIdsByFiles' }
      );

      this.emitEvent(QdrantEventType.ERROR, {
        error: error instanceof Error ? error : new Error(String(error)),
        operation: 'getChunkIdsByFiles',
        collectionName
      });

      return [];
    }
  }

  /**
   * 获取已存在的块ID
   */
  async getExistingChunkIds(collectionName: string, chunkIds: string[]): Promise<string[]> {
    try {
      const client = this.connectionManager.getClient();
      if (!client) {
        throw new Error(ERROR_MESSAGES.CONNECTION_FAILED);
      }

      // 构建过滤器以匹配任何提供的块ID
      const filter = {
        must: [
          {
            key: 'id',
            match: {
              any: chunkIds,
            },
          },
        ],
      };

      // 搜索匹配过滤器的点，只检索ID
      const results = await client.scroll(collectionName, {
        filter,
        with_payload: false,
        with_vector: false,
        limit: 1000, // 根据需要调整此限制
      });

      // 从结果中提取ID
      const existingChunkIds = results.points.map(point => point.id as string);

      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SERVICE_INITIALIZED,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'get_existing_chunk_ids_completed',
          collectionName,
          requestedCount: chunkIds.length,
          existingCount: existingChunkIds.length
        }
      });

      return existingChunkIds;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(
          `Failed to get existing chunk IDs from ${collectionName}: ${error instanceof Error ? error.message : String(error)}`
        ),
        { component: 'QdrantQueryUtils', operation: 'getExistingChunkIds' }
      );

      this.emitEvent(QdrantEventType.ERROR, {
        error: error instanceof Error ? error : new Error(String(error)),
        operation: 'getExistingChunkIds',
        collectionName
      });

      return [];
    }
  }

  /**
   * 滚动浏览点
   */
  async scrollPoints(collectionName: string, filter?: any, limit: number = 100, offset?: any): Promise<any[]> {
    try {
      const client = this.connectionManager.getClient();
      if (!client) {
        throw new Error(ERROR_MESSAGES.CONNECTION_FAILED);
      }

      const scrollParams: any = {
        with_payload: true,
        with_vector: false,
        limit,
      };

      if (filter) {
        scrollParams.filter = filter;
      }

      if (offset) {
        scrollParams.offset = offset;
      }

      const results = await client.scroll(collectionName, scrollParams);

      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SERVICE_INITIALIZED,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'scroll_points_completed',
          collectionName,
          pointCount: results.points.length,
          limit,
          hasFilter: !!filter,
          hasOffset: !!offset
        }
      });

      return results.points;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(
          `Failed to scroll points from ${collectionName}: ${error instanceof Error ? error.message : String(error)}`
        ),
        { component: 'QdrantQueryUtils', operation: 'scrollPoints' }
      );

      this.emitEvent(QdrantEventType.ERROR, {
        error: error instanceof Error ? error : new Error(String(error)),
        operation: 'scrollPoints',
        collectionName
      });

      return [];
    }
  }

  /**
   * 计算点数量
   */
  async countPoints(collectionName: string, filter?: any): Promise<number> {
    try {
      const client = this.connectionManager.getClient();
      if (!client) {
        throw new Error(ERROR_MESSAGES.CONNECTION_FAILED);
      }

      const countParams: any = {};

      if (filter) {
        countParams.filter = filter;
      }

      const result = await client.count(collectionName, countParams);

      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SERVICE_INITIALIZED,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'count_points_completed',
          collectionName,
          count: result.count,
          hasFilter: !!filter
        }
      });

      return result.count;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(
          `Failed to count points in ${collectionName}: ${error instanceof Error ? error.message : String(error)}`
        ),
        { component: 'QdrantQueryUtils', operation: 'countPoints' }
      );

      this.emitEvent(QdrantEventType.ERROR, {
        error: error instanceof Error ? error : new Error(String(error)),
        operation: 'countPoints',
        collectionName
      });

      return 0;
    }
  }

  /**
   * 构建嵌套过滤器
   */
  buildNestedFilter(keyPath: string, value: any): any {
    const keys = keyPath.split('.');
    let filter: any = {};

    // 从最内层开始构建
    let currentFilter: any = {
      match: {
        value: value
      }
    };

    // 从右向左构建嵌套结构
    for (let i = keys.length - 1; i >= 0; i--) {
      if (i === keys.length - 1) {
        filter = {
          key: keys[i],
          ...currentFilter
        };
      } else {
        filter = {
          key: keys[i],
          nested: filter
        };
      }
    }

    return filter;
  }

  /**
   * 构建范围过滤器
   */
  buildRangeFilter(key: string, range: { gte?: number; lte?: number; gt?: number; lt?: number }): any {
    return {
      key,
      range: range
    };
  }

  /**
   * 构建条件过滤器
   */
  buildConditionalFilter(conditions: any[], conditionType: 'must' | 'should' | 'must_not' = 'must'): any {
    if (conditions.length === 0) {
      return undefined;
    }

    if (conditions.length === 1) {
      return conditions[0];
    }

    return {
      [conditionType]: conditions
    };
  }

  /**
   * 订阅事件
   */
  subscribe(type: QdrantEventType, listener: (event: QdrantEvent) => void) {
    if (!this.eventListeners.has(type)) {
      this.eventListeners.set(type, []);
    }
    this.eventListeners.get(type)!.push(listener);
    
    // 返回订阅对象，允许取消订阅
    const subscription = {
      id: `${type}_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
      eventType: type,
      handler: listener,
      unsubscribe: () => {
        const listeners = this.eventListeners.get(type);
        if (listeners) {
          const index = listeners.indexOf(listener);
          if (index > -1) {
            listeners.splice(index, 1);
          }
        }
      }
    };
    
    return subscription;
  }

  /**
   * 发射事件
   */
  private emitEvent(type: QdrantEventType, data?: any, error?: Error): void {
    const event: QdrantEvent = {
      type,
      timestamp: new Date(),
      data,
      error
    };

    const listeners = this.eventListeners.get(type);
    if (listeners) {
      listeners.forEach(async listener => {
        try {
          listener(event);
        } catch (err) {
          await this.databaseLogger.logDatabaseEvent({
            type: DatabaseEventType.ERROR_OCCURRED,
            timestamp: new Date(),
            source: 'qdrant',
            data: {
              operation: 'event_listener_error',
              eventType: type,
              error: err instanceof Error ? err.message : String(err)
            },
            error: err instanceof Error ? err : new Error(String(err))
          });
        }
      });
    }
  }
}