import { injectable, inject } from 'inversify';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { TYPES } from '../../types';
import { IQdrantConnectionManager } from './QdrantConnectionManager';
import { DatabaseLoggerService } from '../common/DatabaseLoggerService';
import { PerformanceMonitor } from '../../infrastructure/monitoring/PerformanceMonitor';
import { DatabaseEventType } from '../common/DatabaseEventTypes';
import {
  CollectionInfo,
  VectorDistance,
  CollectionCreateOptions,
  CollectionStatus,
  CollectionStats,
  DEFAULT_COLLECTION_OPTIONS,
  ERROR_MESSAGES,
  QdrantEventType,
  QdrantEvent
} from './QdrantTypes';

/**
 * Qdrant 集合管理器接口
 */
export interface IQdrantCollectionManager {
  createCollection(name: string, vectorSize: number, distance?: VectorDistance, recreateIfExists?: boolean): Promise<boolean>;
  createCollectionWithOptions(name: string, options: CollectionCreateOptions): Promise<boolean>;
  deleteCollection(name: string): Promise<boolean>;
  collectionExists(name: string): Promise<boolean>;
  getCollectionInfo(collectionName: string): Promise<CollectionInfo | null>;
  getCollectionStats(collectionName: string): Promise<CollectionStats | null>;
  createPayloadIndex(collectionName: string, field: string, fieldType?: string): Promise<boolean>;
  createPayloadIndexes(collectionName: string, fields: string[]): Promise<boolean>;
  listCollections(): Promise<string[]>;
  subscribe(type: QdrantEventType, listener: (event: QdrantEvent) => void): { id: string; eventType: string; handler: any; unsubscribe: () => void };
}

/**
 * Qdrant 集合管理器实现
 * 
 * 负责创建、删除、检查集合，获取集合信息，管理集合配置等
 */
@injectable()
export class QdrantCollectionManager implements IQdrantCollectionManager {
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
   * 创建集合
   */
  async createCollection(
    name: string,
    vectorSize: number,
    distance: VectorDistance = 'Cosine',
    recreateIfExists: boolean = false
  ): Promise<boolean> {
    const options: CollectionCreateOptions = {
      vectorSize,
      distance,
      recreateIfExists
    };
    return this.createCollectionWithOptions(name, options);
  }

  /**
   * 使用选项创建集合
   */
  async createCollectionWithOptions(name: string, options: CollectionCreateOptions): Promise<boolean> {
    try {
      const client = this.connectionManager.getClient();
      if (!client) {
        throw new Error(ERROR_MESSAGES.CONNECTION_FAILED);
      }

      const exists = await this.collectionExists(name);

      // 如果集合已存在，检查向量维度是否匹配
      if (exists && !options.recreateIfExists) {
        const collectionInfo = await this.getCollectionInfo(name);
        if (collectionInfo && collectionInfo.vectors.size !== options.vectorSize) {
          await this.databaseLogger.logDatabaseEvent({
            type: DatabaseEventType.SERVICE_ERROR,
            timestamp: new Date(),
            source: 'qdrant',
            data: {
              operation: 'collection_exists_with_different_size',
              collectionName: name,
              existingSize: collectionInfo.vectors.size,
              requestedSize: options.vectorSize
            }
          });
          // 如果维度不匹配，删除并重新创建集合
          await this.deleteCollection(name);
        } else {
          await this.databaseLogger.logDatabaseEvent({
            type: DatabaseEventType.SERVICE_INITIALIZED,
            timestamp: new Date(),
            source: 'qdrant',
            data: {
              operation: 'collection_exists_with_correct_size',
              collectionName: name,
              vectorSize: options.vectorSize
            }
          });
          return true;
        }
      } else if (exists && options.recreateIfExists) {
        await this.deleteCollection(name);
      }

      // 合并默认选项
      const finalOptions = {
        ...DEFAULT_COLLECTION_OPTIONS,
        ...options,
        optimizersConfig: {
          ...DEFAULT_COLLECTION_OPTIONS.optimizersConfig,
          ...options.optimizersConfig
        }
      };

      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SERVICE_INITIALIZED,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'creating_collection',
          collectionName: name,
          vectorSize: finalOptions.vectorSize,
          distance: finalOptions.distance,
          optimizersConfig: finalOptions.optimizersConfig
        }
      });

      await client.createCollection(name, {
        vectors: {
          size: finalOptions.vectorSize,
          distance: finalOptions.distance,
        },
        optimizers_config: finalOptions.optimizersConfig,
        replication_factor: finalOptions.replicationFactor,
        write_consistency_factor: finalOptions.writeConsistencyFactor,
      });

      // 创建payload索引
      const payloadIndexes = finalOptions.payloadIndexes || DEFAULT_COLLECTION_OPTIONS.payloadIndexes;
      if (payloadIndexes) {
        for (const field of payloadIndexes) {
          const indexCreated = await this.createPayloadIndex(name, field);
          if (!indexCreated) {
            throw new Error(`Failed to create payload index for field: ${field}`);
          }
        }
      }

      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SERVICE_INITIALIZED,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'collection_created',
          collectionName: name,
          vectorSize: finalOptions.vectorSize,
          distance: finalOptions.distance,
          indexesCount: payloadIndexes?.length || 0
        }
      });

      this.emitEvent(QdrantEventType.COLLECTION_CREATED, {
        collectionName: name,
        vectorSize: finalOptions.vectorSize,
        distance: finalOptions.distance
      });

      return true;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(
          `Failed to create collection ${name}: ${error instanceof Error ? error.message : String(error)}`
        ),
        { component: 'QdrantCollectionManager', operation: 'createCollection' }
      );
      return false;
    }
  }

  /**
   * 检查集合是否存在
   */
  async collectionExists(name: string): Promise<boolean> {
    try {
      const client = this.connectionManager.getClient();
      if (!client) {
        return false;
      }

      const collections = await client.getCollections();
      return collections.collections.some(col => col.name === name);
    } catch (error) {
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SERVICE_ERROR,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'collection_existence_check_failed',
          collectionName: name,
          error: error instanceof Error ? error.message : String(error)
        }
      });
      return false;
    }
  }

  /**
   * 删除集合
   */
  async deleteCollection(name: string): Promise<boolean> {
    try {
      const client = this.connectionManager.getClient();
      if (!client) {
        throw new Error(ERROR_MESSAGES.CONNECTION_FAILED);
      }

      await client.deleteCollection(name);
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SERVICE_INITIALIZED,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'collection_deleted',
          collectionName: name
        }
      });
      return true;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(
          `Failed to delete collection ${name}: ${error instanceof Error ? error.message : String(error)}`
        ),
        { component: 'QdrantCollectionManager', operation: 'deleteCollection' }
      );
      return false;
    }
  }

  /**
   * 获取集合信息
   */
  async getCollectionInfo(collectionName: string): Promise<CollectionInfo | null> {
    try {
      const client = this.connectionManager.getClient();
      if (!client) {
        return null;
      }

      const info = await client.getCollection(collectionName);

      // 处理向量配置的新结构
      const vectorsConfig = info.config.params.vectors;
      const vectorSize =
        typeof vectorsConfig === 'object' && vectorsConfig !== null && 'size' in vectorsConfig
          ? vectorsConfig.size
          : 0;
      const vectorDistance =
        typeof vectorsConfig === 'object' && vectorsConfig !== null && 'distance' in vectorsConfig
          ? vectorsConfig.distance
          : 'Cosine';

      return {
        name: collectionName,
        vectors: {
          size: typeof vectorSize === 'number' ? vectorSize : 0,
          distance:
            typeof vectorDistance === 'string'
              ? (vectorDistance as VectorDistance)
              : 'Cosine',
        },
        pointsCount: info.points_count || 0,
        status: info.status,
      };
    } catch (error) {
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SERVICE_ERROR,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'get_collection_info_failed',
          collectionName,
          error: error instanceof Error ? error.message : String(error)
        }
      });
      return null;
    }
  }

  /**
   * 获取集合统计信息
   */
  async getCollectionStats(collectionName: string): Promise<CollectionStats | null> {
    try {
      const collectionInfo = await this.getCollectionInfo(collectionName);
      if (!collectionInfo) {
        return null;
      }

      // 这里可以添加更多统计信息的获取逻辑
      // 例如磁盘使用情况、内存使用情况等
      return {
        name: collectionInfo.name,
        vectorsCount: collectionInfo.pointsCount,
        vectorSize: collectionInfo.vectors.size,
        distance: collectionInfo.vectors.distance,
        status: this.mapStatusToEnum(collectionInfo.status),
        indexedVectorsCount: collectionInfo.pointsCount, // 假设所有向量都已索引
        payloadSchema: {}, // 可以从集合配置中获取
        diskUsage: 0, // 需要从 Qdrant API 获取
        memoryUsage: 0 // 需要从 Qdrant API 获取
      };
    } catch (error) {
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SERVICE_ERROR,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'get_collection_stats_failed',
          collectionName,
          error: error instanceof Error ? error.message : String(error)
        }
      });
      return null;
    }
  }

  /**
   * 创建有效载荷索引
   */
  async createPayloadIndex(collectionName: string, field: string, fieldType: string = 'keyword'): Promise<boolean> {
    try {
      const client = this.connectionManager.getClient();
      if (!client) {
        throw new Error(ERROR_MESSAGES.CONNECTION_FAILED);
      }

      await client.createPayloadIndex(collectionName, {
        field_name: field,
        field_schema: fieldType as any,
      });

      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SERVICE_INITIALIZED,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'payload_index_created',
          collectionName,
          field
        }
      });
      return true;
    } catch (error) {
      // 检查是否为"已存在"错误，如果是则返回true
      const errorMessage = error instanceof Error ? error.message : String(error);
      if (errorMessage.toLowerCase().includes('already exists')) {
        await this.databaseLogger.logDatabaseEvent({
          type: DatabaseEventType.SERVICE_INITIALIZED,
          timestamp: new Date(),
          source: 'qdrant',
          data: {
            operation: 'payload_index_already_exists',
            collectionName,
            field
          }
        });
        return true;
      }

      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.ERROR_OCCURRED,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'create_payload_index_failed',
          collectionName,
          field,
          fieldType,
          error: errorMessage
        },
        error: new Error(errorMessage)
      });
      return false;
    }
  }

  /**
   * 批量创建有效载荷索引
   */
  async createPayloadIndexes(collectionName: string, fields: string[]): Promise<boolean> {
    try {
      const results = await Promise.all(
        fields.map(field => this.createPayloadIndex(collectionName, field))
      );

      const successCount = results.filter(result => result).length;
      const totalCount = results.length;

      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SERVICE_INITIALIZED,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'payload_indexes_created',
          collectionName,
          successCount,
          totalCount,
          fields
        }
      });

      return successCount === totalCount;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(
          `Failed to create payload indexes for collection ${collectionName}: ${error instanceof Error ? error.message : String(error)}`
        ),
        { component: 'QdrantCollectionManager', operation: 'createPayloadIndexes' }
      );
      return false;
    }
  }

  /**
   * 列出所有集合
   */
  async listCollections(): Promise<string[]> {
    try {
      const client = this.connectionManager.getClient();
      if (!client) {
        return [];
      }

      const collections = await client.getCollections();
      return collections.collections.map(col => col.name);
    } catch (error) {
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SERVICE_ERROR,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          operation: 'list_collections_failed',
          error: error instanceof Error ? error.message : String(error)
        }
      });
      return [];
    }
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

  /**
   * 将状态字符串映射为枚举
   */
  private mapStatusToEnum(status: string): CollectionStatus {
    switch (status.toLowerCase()) {
      case 'green':
      case 'ready':
        return CollectionStatus.READY;
      case 'yellow':
      case 'updating':
        return CollectionStatus.UPDATING;
      case 'red':
      case 'error':
        return CollectionStatus.ERROR;
      default:
        return CollectionStatus.READY;
    }
  }
}