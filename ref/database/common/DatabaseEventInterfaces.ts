import { DatabaseEventType, QdrantEventType, NebulaEventType } from './DatabaseEventTypes';

/**
 * Qdrant 连接事件数据
 */
export interface QdrantConnectionEvent {
  type: QdrantEventType.COLLECTION_CREATED | DatabaseEventType.CONNECTION_OPENED | DatabaseEventType.CONNECTION_CLOSED | DatabaseEventType.CONNECTION_FAILED;
  timestamp: Date;
  source: 'qdrant';
  data: {
    host: string;
    port: number;
    status: 'connected' | 'disconnected' | 'failed';
    message?: string;
  };
}

/**
 * Qdrant 集合事件数据
 */
export interface QdrantCollectionEvent {
  type: QdrantEventType.COLLECTION_CREATED | QdrantEventType.COLLECTION_DELETED | QdrantEventType.COLLECTION_UPDATED | QdrantEventType.COLLECTION_ERROR;
  timestamp: Date;
  source: 'qdrant';
  data: {
    collectionName: string;
    vectorSize: number;
    distance: 'Cosine' | 'Euclidean' | 'DotProduct';
    status: 'success' | 'failed';
    error?: string;
  };
}

/**
 * Qdrant 向量操作事件数据
 */
export interface QdrantVectorEvent {
  type: QdrantEventType.VECTOR_INSERTED | QdrantEventType.VECTOR_UPDATED | QdrantEventType.VECTOR_DELETED | QdrantEventType.VECTOR_SEARCHED;
  timestamp: Date;
  source: 'qdrant';
  data: {
    collectionName: string;
    vectorCount: number;
    operation: 'insert' | 'update' | 'delete' | 'search';
    status: 'success' | 'failed';
    executionTime?: number;
    error?: string;
    searchResult?: {
      count: number;
      score: number;
    };
  };
}

/**
 * Qdrant 索引事件数据
 */
export interface QdrantIndexEvent {
  type: QdrantEventType.INDEX_CREATED | QdrantEventType.INDEX_DELETED | QdrantEventType.INDEX_UPDATED;
  timestamp: Date;
  source: 'qdrant';
  data: {
    collectionName: string;
    fieldName: string;
    indexType: string;
    status: 'success' | 'failed';
    error?: string;
  };
}

/**
 * Nebula 空间事件数据
 */
export interface NebulaSpaceEvent {
  type: NebulaEventType.SPACE_CREATED | NebulaEventType.SPACE_DELETED | NebulaEventType.SPACE_UPDATED | NebulaEventType.SPACE_ERROR;
  timestamp: Date;
  source: 'nebula';
  data: {
    spaceName: string;
    partitionNum: number;
    replicaFactor: number;
    status: 'success' | 'failed';
    error?: string;
  };
}

/**
 * Nebula 节点事件数据
 */
export interface NebulaNodeEvent {
  type: NebulaEventType.NODE_INSERTED | NebulaEventType.NODE_UPDATED | NebulaEventType.NODE_DELETED;
  timestamp: Date;
  source: 'nebula';
  data: {
    spaceName: string;
    tagName: string;
    nodeId: string;
    properties: Record<string, any>;
    operation: 'insert' | 'update' | 'delete';
    status: 'success' | 'failed';
    error?: string;
  };
}

/**
 * Nebula 关系事件数据
 */
export interface NebulaRelationshipEvent {
  type: NebulaEventType.RELATIONSHIP_INSERTED | NebulaEventType.RELATIONSHIP_UPDATED | NebulaEventType.RELATIONSHIP_DELETED;
  timestamp: Date;
  source: 'nebula';
  data: {
    spaceName: string;
    edgeName: string;
    sourceId: string;
    targetId: string;
    properties: Record<string, any>;
    operation: 'insert' | 'update' | 'delete';
    status: 'success' | 'failed';
    error?: string;
  };
}

/**
 * Nebula 查询事件数据
 */
export interface NebulaQueryEvent {
  type: NebulaEventType.QUERY_EXECUTED | NebulaEventType.QUERY_ERROR;
  timestamp: Date;
  source: 'nebula';
  data: {
    query: string;
    parameters?: Record<string, any>;
    executionTime: number;
    resultCount?: number;
    status: 'success' | 'failed';
    error?: string;
  };
}

/**
 * 性能指标事件数据
 */
export interface PerformanceMetricEvent {
  type: DatabaseEventType.PERFORMANCE_METRIC;
  timestamp: Date;
  source: 'qdrant' | 'nebula' | 'common';
  data: {
    operation: string;
    duration: number;
    success: boolean;
    dataSize?: number;
    memoryUsage?: number;
    cpuUsage?: number;
  };
}

/**
 * 查询执行事件数据
 */
export interface QueryExecutionEvent {
  type: DatabaseEventType.QUERY_EXECUTED;
  timestamp: Date;
  source: 'qdrant' | 'nebula' | 'common';
  data: {
    query: string;
    parameters?: Record<string, any>;
    executionTime: number;
    resultCount?: number;
    success: boolean;
    error?: string;
    cached: boolean;
  };
}

/**
 * 批量操作事件数据
 */
export interface BatchOperationEvent {
  type: DatabaseEventType.BATCH_OPERATION_COMPLETED;
  timestamp: Date;
  source: 'qdrant' | 'nebula' | 'common';
  data: {
    operationType: 'insert' | 'update' | 'delete';
    batchSize: number;
    success: boolean;
    duration: number;
    failedItems?: number;
    throughput?: number;
  };
}

/**
 * 项目索引事件数据
 */
export interface ProjectIndexEvent {
  type: DatabaseEventType.SERVICE_INITIALIZED;
  timestamp: Date;
  source: 'common';
  data: {
    projectId: string;
    fileCount: number;
    indexingTime: number;
    status: 'started' | 'completed' | 'failed';
    error?: string;
  };
}

/**
 * 错误事件数据
 */
export interface ErrorEvent {
  type: DatabaseEventType.ERROR_OCCURRED;
  timestamp: Date;
  source: 'qdrant' | 'nebula' | 'common';
  data: {
    operation: string;
    errorType: string;
    errorMessage: string;
    stackTrace?: string;
    context?: Record<string, any>;
  };
}

/**
 * 数据库事件联合类型
 * 包含所有可能的数据库事件类型
 */
export type DatabaseEventUnion = 
  | QdrantConnectionEvent
  | QdrantCollectionEvent
  | QdrantVectorEvent
  | QdrantIndexEvent
  | NebulaSpaceEvent
  | NebulaNodeEvent
  | NebulaRelationshipEvent
  | NebulaQueryEvent
  | PerformanceMetricEvent
  | QueryExecutionEvent
  | BatchOperationEvent
  | ProjectIndexEvent
  | ErrorEvent;

/**
 * 事件类型映射接口
 * 将事件类型字符串映射到对应的事件数据类型
 */
export interface DatabaseEventTypes {
  'qdrant.connection': QdrantConnectionEvent;
  'qdrant.collection': QdrantCollectionEvent;
  'qdrant.vector': QdrantVectorEvent;
  'qdrant.index': QdrantIndexEvent;
  'nebula.space': NebulaSpaceEvent;
  'nebula.node': NebulaNodeEvent;
  'nebula.relationship': NebulaRelationshipEvent;
  'nebula.query': NebulaQueryEvent;
  'performance.metric': PerformanceMetricEvent;
  'query.execution': QueryExecutionEvent;
  'batch.operation': BatchOperationEvent;
  'project.index': ProjectIndexEvent;
  'error.occurred': ErrorEvent;
}

/**
 * 类型安全的事件监听器类型
 * 
 * @example
 * // 创建类型安全的事件监听器
 * const qdrantConnectionListener: DatabaseEventListener<QdrantConnectionEvent> = (event) => {
 *   console.log(`Qdrant ${event.data.status} to ${event.data.host}:${event.data.port}`);
 * };
 * 
 * // 使用事件监听器
 * eventManager.addEventListener('qdrant.connection', qdrantConnectionListener);
 */
export type DatabaseEventListener<T = DatabaseEventUnion> = (event: T) => void;

/**
 * 事件类型守卫
 * 用于在运行时确定事件的具体类型
 */
export const isQdrantConnectionEvent = (event: any): event is QdrantConnectionEvent => {
  try {
    if (!event) return false;
    if (![QdrantEventType.COLLECTION_CREATED, DatabaseEventType.CONNECTION_OPENED, DatabaseEventType.CONNECTION_CLOSED, DatabaseEventType.CONNECTION_FAILED].includes(event.type)) return false;
    if (event.source !== 'qdrant') return false;
    if (!event.data) return false;
    if (typeof event.data.host !== 'string') return false;
    if (typeof event.data.port !== 'number') return false;
    return true;
  } catch (error) {
    return false;
  }
};

export const isQdrantCollectionEvent = (event: any): event is QdrantCollectionEvent => {
  try {
    if (!event) return false;
    if (![QdrantEventType.COLLECTION_CREATED, QdrantEventType.COLLECTION_DELETED, QdrantEventType.COLLECTION_UPDATED, QdrantEventType.COLLECTION_ERROR].includes(event.type)) return false;
    if (event.source !== 'qdrant') return false;
    if (!event.data) return false;
    if (typeof event.data.collectionName !== 'string') return false;
    return true;
  } catch (error) {
    return false;
  }
};

export const isQdrantVectorEvent = (event: any): event is QdrantVectorEvent => {
  try {
    if (!event) return false;
    if (![QdrantEventType.VECTOR_INSERTED, QdrantEventType.VECTOR_UPDATED, QdrantEventType.VECTOR_DELETED, QdrantEventType.VECTOR_SEARCHED].includes(event.type)) return false;
    if (event.source !== 'qdrant') return false;
    if (!event.data) return false;
    if (typeof event.data.collectionName !== 'string') return false;
    return true;
  } catch (error) {
    return false;
  }
};

export const isNebulaSpaceEvent = (event: any): event is NebulaSpaceEvent => {
  try {
    if (!event) return false;
    if (![NebulaEventType.SPACE_CREATED, NebulaEventType.SPACE_DELETED, NebulaEventType.SPACE_UPDATED, NebulaEventType.SPACE_ERROR].includes(event.type)) return false;
    if (event.source !== 'nebula') return false;
    if (!event.data) return false;
    if (typeof event.data.spaceName !== 'string') return false;
    return true;
  } catch (error) {
    return false;
  }
};

export const isNebulaQueryEvent = (event: any): event is NebulaQueryEvent => {
  try {
    if (!event) return false;
    if (![NebulaEventType.QUERY_EXECUTED, NebulaEventType.QUERY_ERROR].includes(event.type)) return false;
    if (event.source !== 'nebula') return false;
    if (!event.data) return false;
    if (typeof event.data.query !== 'string') return false;
    return true;
  } catch (error) {
    return false;
  }
};

export const isPerformanceMetricEvent = (event: any): event is PerformanceMetricEvent => {
  try {
    if (!event) return false;
    if (event.type !== DatabaseEventType.PERFORMANCE_METRIC) return false;
    if (!event.data) return false;
    if (typeof event.data.operation !== 'string') return false;
    if (typeof event.data.duration !== 'number') return false;
    return true;
  } catch (error) {
    return false;
  }
};

export const isQueryExecutionEvent = (event: any): event is QueryExecutionEvent => {
  try {
    if (!event) return false;
    if (event.type !== DatabaseEventType.QUERY_EXECUTED) return false;
    if (!event.data) return false;
    if (typeof event.data.query !== 'string') return false;
    if (typeof event.data.cached !== 'boolean') return false;
    return true;
  } catch (error) {
    return false;
  }
};

export const isBatchOperationEvent = (event: any): event is BatchOperationEvent => {
  try {
    if (!event) return false;
    if (event.type !== DatabaseEventType.BATCH_OPERATION_COMPLETED) return false;
    if (!event.data) return false;
    if (typeof event.data.operationType !== 'string') return false;
    if (typeof event.data.batchSize !== 'number') return false;
    return true;
  } catch (error) {
    return false;
  }
};

export const isProjectIndexEvent = (event: any): event is ProjectIndexEvent => {
  try {
    if (!event) return false;
    if (event.type !== DatabaseEventType.SERVICE_INITIALIZED) return false;
    if (event.source !== 'common') return false;
    if (!event.data) return false;
    if (typeof event.data.projectId !== 'string') return false;
    return true;
  } catch (error) {
    return false;
  }
};

export const isErrorEvent = (event: any): event is ErrorEvent => {
  try {
    if (!event) return false;
    if (event.type !== DatabaseEventType.ERROR_OCCURRED) return false;
    if (!event.data) return false;
    if (typeof event.data.operation !== 'string') return false;
    if (typeof event.data.errorMessage !== 'string') return false;
    return true;
  } catch (error) {
    return false;
  }
};