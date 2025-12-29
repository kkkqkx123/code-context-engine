/**
 * 统一的数据库事件类型定义
 * 适用于Qdrant和Nebula数据库服务
 */

/**
 * 基础数据库事件类型
 */
export enum DatabaseEventType {
  // 连接事件
  CONNECTION_OPENED = 'connection_opened',
  CONNECTION_CLOSED = 'connection_closed',
  CONNECTION_FAILED = 'connection_failed',
  CONNECTION_ERROR = 'connection_error',

  // 项目空间事件
  SPACE_CREATED = 'space_created',
  SPACE_DELETED = 'space_deleted',
  SPACE_SELECTED = 'space_selected',
  SPACE_CLEARED = 'space_cleared',
  SPACE_ERROR = 'space_error',

  // 数据操作事件
  DATA_INSERTED = 'data_inserted',
  DATA_UPDATED = 'data_updated',
  DATA_DELETED = 'data_deleted',
  DATA_QUERIED = 'data_queried',
  DATA_RETRIEVED = 'data_retrieved',
  DATA_ERROR = 'data_error',

  // 服务生命周期事件
  SERVICE_INITIALIZED = 'service_initialized',
  SERVICE_STARTED = 'service_started',
  SERVICE_STOPPED = 'service_stopped',
  SERVICE_ERROR = 'service_error',

  // 日志级别事件
  DEBUG = 'debug',
  INFO = 'info',
  WARNING = 'warning',

  // 性能和监控事件
  PERFORMANCE_METRIC = 'performance_metric',
  QUERY_EXECUTED = 'query_executed',
  BATCH_OPERATION_COMPLETED = 'batch_operation_completed',
  HEALTH_CHECK = 'health_check',
  ERROR_OCCURRED = 'error_occurred',

  // 信息检索事件
  INFO_RETRIEVED = 'info_retrieved',

  // 配置管理事件
  CONFIG_ACCESSED = 'config_accessed',
  CONFIG_SET = 'config_set',
  CONFIG_REMOVED = 'config_removed',
  CONFIG_RELOADED = 'config_reloaded',
  ENVIRONMENT_CHANGED = 'environment_changed',

  // 连接池事件
  POOL_INITIALIZED = 'pool_initialized',
  POOL_CLOSED = 'pool_closed',
  CONNECTION_ACQUIRED = 'connection_acquired',
  CONNECTION_RELEASED = 'connection_released',
  CONNECTION_CREATED = 'connection_created',
  CONNECTION_REMOVED = 'connection_removed'
}

/**
 * Qdrant特定事件类型
 */
export enum QdrantEventType {
  // 集合事件
  COLLECTION_CREATED = 'collection_created',
  COLLECTION_DELETED = 'collection_deleted',
  COLLECTION_UPDATED = 'collection_updated',
  COLLECTION_ERROR = 'collection_error',

  // 向量操作事件
  VECTOR_INSERTED = 'vector_inserted',
  VECTOR_UPDATED = 'vector_updated',
  VECTOR_DELETED = 'vector_deleted',
  VECTOR_SEARCHED = 'vector_searched',

  // 索引事件
  INDEX_CREATED = 'index_created',
  INDEX_DELETED = 'index_deleted',
  INDEX_UPDATED = 'index_updated'
}

/**
 * Nebula特定事件类型
 */
export enum NebulaEventType {
  // 空间事件
  SPACE_CREATED = 'space_created',
  SPACE_DELETED = 'space_deleted',
  SPACE_UPDATED = 'space_updated',
  SPACE_ERROR = 'space_error',

  // 节点和边事件
  NODE_INSERTED = 'node_inserted',
  NODE_UPDATED = 'node_updated',
  NODE_DELETED = 'node_deleted',
  RELATIONSHIP_INSERTED = 'relationship_inserted',
  RELATIONSHIP_UPDATED = 'relationship_updated',
  RELATIONSHIP_DELETED = 'relationship_deleted',

  // 查询事件
  QUERY_EXECUTED = 'query_executed',
  QUERY_ERROR = 'query_error'
}

/**
 * 统一的事件接口
 */
export interface DatabaseEvent {
  type: DatabaseEventType | QdrantEventType | NebulaEventType;
  timestamp: Date;
  source: 'qdrant' | 'nebula' | 'common';
  data?: any;
  error?: Error;
  metadata?: {
    projectId?: string;
    userId?: string;
    sessionId?: string;
    duration?: number;
    operation?: string;
    originalEventType?: string;
  };
}

/**
 * Nebula特定的事件接口
 */
export interface NebulaEvent {
  type: NebulaEventType | DatabaseEventType;
  timestamp: Date;
  data?: any;
  error?: Error;
}

/**
 * 事件监听器接口
 *
 * 这是一个泛型接口，允许指定事件数据的具体类型以增强类型安全性。
 * 如果未指定泛型参数，则默认使用 DatabaseEvent 类型以保持向后兼容性。
 *
 * @template T - 事件数据的类型，默认为 DatabaseEvent
 * @param event - 事件数据
 * @returns void
 *
 * @example
 * // 使用默认的 DatabaseEvent 类型（向后兼容）
 * const listener: DatabaseEventListener = (event) => {
 *   console.log(event.type);
 * };
 *
 * @example
 * // 使用具体类型增强类型安全性
 * interface QdrantConnectionEvent {
 *   type: QdrantEventType.COLLECTION_CREATED;
 *   timestamp: Date;
 *   source: 'qdrant';
 *   data: {
 *     collectionName: string;
 *     vectorSize: number;
 *   };
 * }
 *
 * const collectionListener: DatabaseEventListener<QdrantConnectionEvent> = (event) => {
 *   // 此时 TypeScript 会知道 event 是 QdrantConnectionEvent 类型
 *   console.log(`Collection ${event.data.collectionName} created`);
 * };
 */
export interface DatabaseEventListener<T = DatabaseEvent> {
  (event: T): void;
}

/**
 * 事件管理器接口
 *
 * 提供类型安全的事件管理功能，支持泛型事件监听器。
 *
 * @template TEvents - 事件类型映射接口，将事件类型映射到对应的事件数据类型
 *
 * @example
 * // 定义事件类型映射
 * interface AppEvents {
 *   'user.login': { userId: string; timestamp: Date };
 *   'user.logout': { userId: string; timestamp: Date };
 *   'data.updated': { count: number; source: string };
 * }
 *
 * // 使用泛型事件管理器
 * const eventManager: IEventManager<AppEvents> = ...;
 *
 * // 添加类型安全的监听器
 * eventManager.addEventListener('user.login', (event) => {
 *   // TypeScript 会知道 event 包含 userId 和 timestamp
 *   console.log(`User ${event.userId} logged in at ${event.timestamp}`);
 * });
 */
export interface Subscription {
  id: string;
  eventType: string;
  handler: any;
  unsubscribe: () => void;
}

export interface IEventManager<TEvents = Record<string, any>> {
  /**
   * 添加事件监听器（订阅模式）
   */
  subscribe<K extends keyof TEvents>(
    eventType: DatabaseEventType | QdrantEventType | NebulaEventType | K,
    listener: DatabaseEventListener<TEvents[K] | DatabaseEvent>
  ): Subscription;

  /**
   * 发出事件
   */
  emitEvent(event: DatabaseEvent): void;

  /**
   * 移除所有监听器
   */
  removeAllListeners(): void;

  /**
   * 获取监听器数量
   */
  getListenerCount(eventType?: DatabaseEventType | QdrantEventType | NebulaEventType | keyof TEvents): number;
}

/**
 * 数据库事件管理器类
 */
export { DatabaseEventManager } from './DatabaseEventManager';

/**
 * 事件过滤器接口
 */
export interface IEventFilter {
  filter(event: DatabaseEvent): boolean;
}

/**
 * 事件处理器接口
 */
export interface IEventHandler {
  handle(event: DatabaseEvent): Promise<void>;
}

/**
 * 性能指标事件数据
 */
export interface PerformanceMetricData {
  operation: string;
  duration: number;
  success: boolean;
  dataSize?: number;
  timestamp: Date;
}

/**
 * 查询执行事件数据
 */
export interface QueryExecutionData {
  query: string;
  parameters?: Record<string, any>;
  executionTime: number;
  resultCount?: number;
  success: boolean;
  error?: string;
}

/**
 * 批量操作事件数据
 */
export interface BatchOperationData {
  operationType: 'insert' | 'update' | 'delete';
  batchSize: number;
  success: boolean;
  failedItems?: number;
  duration: number;
}