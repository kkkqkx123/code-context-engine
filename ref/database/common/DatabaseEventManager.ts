import { injectable } from 'inversify';
import {
  DatabaseEvent,
  DatabaseEventListener,
  IEventManager,
  DatabaseEventType,
  QdrantEventType,
  NebulaEventType
} from './DatabaseEventTypes';
import { randomUUID } from 'crypto';

export interface Subscription {
  id: string;
  eventType: string;
  handler: DatabaseEventListener;
  unsubscribe: () => void;
}

/**
 * 统一的数据库事件管理器实现（订阅模式）
 * 为Qdrant和Nebula数据库服务提供事件管理功能
 *
 * @template TEvents - 事件类型映射接口，将事件类型映射到对应的事件数据类型
 *
 * @example
 * // 定义数据库事件类型映射
 * interface DatabaseEvents {
 *   'qdrant.connected': { host: string; port: number };
 *   'nebula.query.executed': { query: string; executionTime: number };
 *   'project.indexed': { projectId: string; fileCount: number };
 * }
 *
 * // 创建类型安全的事件管理器
 * const eventManager = new DatabaseEventManager<DatabaseEvents>();
 */
@injectable()
export class DatabaseEventManager<TEvents = Record<string, any>> implements IEventManager<TEvents> {
  private eventListeners: Map<string, Map<string, DatabaseEventListener>> = new Map(); // eventType -> Map<subscriptionId, listener>
  private eventHistory: DatabaseEvent[] = [];
  private maxEventHistory: number = 1000;

  /**
   * 发出事件
   */
  emitEvent<K extends keyof TEvents>(event: TEvents[K] | DatabaseEvent): void {
    // 处理泛型事件类型
    const isDatabaseEvent = (e: any): e is DatabaseEvent => {
      return e && typeof e.type !== 'undefined' && e.timestamp instanceof Date;
    };

    if (isDatabaseEvent(event)) {
      // 记录事件历史
      this.recordEvent(event);

      // 通知特定事件类型的监听器
      const specificListeners = this.eventListeners.get(String(event.type));
      if (specificListeners) {
        this.notifyListeners(specificListeners, event);
      }

      // 通知通用监听器
      const generalListeners = this.eventListeners.get('*');
      if (generalListeners) {
        this.notifyListeners(generalListeners, event);
      }

      // 通知错误事件的监听器
      if (event.error) {
        const errorListeners = this.eventListeners.get(String(DatabaseEventType.ERROR_OCCURRED));
        if (errorListeners) {
          this.notifyListeners(errorListeners, event);
        }
      }
    } else {
      // 处理泛型事件 - 需要重构以支持泛型事件
      this.emitGenericEvent<K>(event as TEvents[K]);
    }
 }

  /**
   * 发出泛型事件
   */
  private emitGenericEvent<K extends keyof TEvents>(event: TEvents[K]): void {
    // 由于泛型事件没有统一的类型结构，我们需要使用运行时类型检查
    // 这里我们简化处理，只通知具体的监听器
    
    // 将泛型事件转换为 DatabaseEvent 格式
    // 我们使用 data 字段来存储原始的泛型事件数据
    const adaptedEvent: DatabaseEvent = {
      type: DatabaseEventType.SERVICE_INITIALIZED, // 使用默认类型
      timestamp: new Date(),
      source: 'common',
      data: event
    };
    
    // 通知所有通用监听器
    const generalListeners = this.eventListeners.get('*');
    if (generalListeners) {
      this.notifyListeners(generalListeners, adaptedEvent);
    }
    
    // 尝试从泛型事件中获取类型信息，并通知特定监听器
    const eventType = this.getEventTypeFromGenericEvent(event);
    if (eventType) {
      const specificListeners = this.eventListeners.get(eventType);
      if (specificListeners) {
        this.notifyListeners(specificListeners, adaptedEvent);
      }
    }
  }

  /**
   * 从泛型事件中获取事件类型
   */
  private getEventTypeFromGenericEvent<K extends keyof TEvents>(event: TEvents[K]): string | null {
    // 尝试从事件对象中获取类型信息
    if (event && typeof event === 'object' && 'type' in event) {
      return String((event as any).type);
    }
    return null;
 }

  /**
   * 移除所有监听器
   */
  removeAllListeners(): void {
    this.eventListeners.clear();
  }

  /**
   * 添加事件监听器（订阅模式）
   */
  subscribe<K extends keyof TEvents>(
    eventType: DatabaseEventType | QdrantEventType | NebulaEventType | K,
    listener: DatabaseEventListener<TEvents[K]>
  ): Subscription {
    const eventTypeStr = String(eventType);
    
    // 如果该事件类型还没有监听器映射，创建一个
    if (!this.eventListeners.has(eventTypeStr)) {
      this.eventListeners.set(eventTypeStr, new Map());
    }
    
    // 生成唯一订阅ID
    const subscriptionId = randomUUID();
    const listenersMap = this.eventListeners.get(eventTypeStr)!;
    
    // 存储监听器
    listenersMap.set(subscriptionId, listener as DatabaseEventListener);
    
    // 创建并返回订阅对象
    const subscription = {
      id: subscriptionId,
      eventType: eventTypeStr,
      handler: listener as DatabaseEventListener,
      unsubscribe: () => {
        const map = this.eventListeners.get(eventTypeStr);
        if (map) {
          map.delete(subscriptionId);
          // 如果该事件类型没有监听器了，删除整个映射
          if (map.size === 0) {
            this.eventListeners.delete(eventTypeStr);
          }
        }
      }
    };
    
    return subscription;
  }

  /**
   * 获取监听器数量
   */
  getListenerCount(eventType?: DatabaseEventType | QdrantEventType | NebulaEventType | keyof TEvents): number {
    if (eventType === undefined) {
      // 返回所有监听器的总数
      let totalCount = 0;
      for (const listenersMap of this.eventListeners.values()) {
        totalCount += listenersMap.size;
      }
      return totalCount;
    }
    
    const eventTypeStr = String(eventType);
    const listenersMap = this.eventListeners.get(eventTypeStr);
    return listenersMap ? listenersMap.size : 0;
  }

  /**
   * 获取事件历史
   */
  getEventHistory(limit?: number): DatabaseEvent[] {
    const history = [...this.eventHistory];
    if (limit !== undefined) {
      return history.slice(-limit);
    }
    return history;
  }

  /**
   * 清除事件历史
   */
  clearEventHistory(): void {
    this.eventHistory = [];
  }

  /**
   * 设置最大事件历史记录数
   */
  setMaxEventHistory(max: number): void {
    this.maxEventHistory = Math.max(1, max);
    // 如果当前历史记录超过新的最大值，截断它
    if (this.eventHistory.length > this.maxEventHistory) {
      this.eventHistory = this.eventHistory.slice(-this.maxEventHistory);
    }
  }

  /**
   * 记录事件到历史
   */
  private recordEvent(event: DatabaseEvent): void {
    this.eventHistory.push(event);
    
    // 保持历史记录在限制范围内
    if (this.eventHistory.length > this.maxEventHistory) {
      this.eventHistory = this.eventHistory.slice(-this.maxEventHistory);
    }
  }

  /**
   * 通知监听器
   */
  private notifyListeners(listenersMap: Map<string, DatabaseEventListener>, event: DatabaseEvent): void {
    // 遍历监听器并通知它们
    for (const listener of listenersMap.values()) {
      try {
        listener(event);
      } catch (error) {
        console.error(`Error in event listener for ${event.type}:`, error);
        
        // 创建错误事件
        const errorEvent: DatabaseEvent = {
          type: DatabaseEventType.ERROR_OCCURRED,
          timestamp: new Date(),
          source: 'common',
          error: error instanceof Error ? error : new Error(String(error)),
          metadata: {
            operation: 'event_listener',
            originalEventType: event.type
          }
        };
        
        // 递归发出错误事件，但避免无限循环
        if (event.type !== DatabaseEventType.ERROR_OCCURRED) {
          this.emitEvent(errorEvent);
        }
      }
    }
  }

  /**
   * 创建事件
   */
  static createEvent(
    type: DatabaseEventType | QdrantEventType | NebulaEventType,
    source: 'qdrant' | 'nebula' | 'common',
    data?: any,
    error?: Error,
    metadata?: DatabaseEvent['metadata']
 ): DatabaseEvent {
    return {
      type,
      timestamp: new Date(),
      source,
      data,
      error,
      metadata
    };
  }

  /**
   * 创建性能指标事件
   */
  static createPerformanceEvent(
    operation: string,
    duration: number,
    success: boolean,
    source: 'qdrant' | 'nebula' | 'common',
    dataSize?: number
  ): DatabaseEvent {
    return DatabaseEventManager.createEvent(
      DatabaseEventType.PERFORMANCE_METRIC,
      source,
      {
        operation,
        duration,
        success,
        dataSize,
        timestamp: new Date()
      }
    );
  }

  /**
   * 创建查询执行事件
   */
  static createQueryEvent(
    query: string,
    executionTime: number,
    success: boolean,
    source: 'qdrant' | 'nebula' | 'common',
    parameters?: Record<string, any>,
    resultCount?: number,
    error?: string
  ): DatabaseEvent {
    return DatabaseEventManager.createEvent(
      DatabaseEventType.QUERY_EXECUTED,
      source,
      {
        query,
        parameters,
        executionTime,
        resultCount,
        success,
        error
      }
    );
  }

 /**
   * 创建批量操作事件
   */
  static createBatchOperationEvent(
    operationType: 'insert' | 'update' | 'delete',
    batchSize: number,
    success: boolean,
    duration: number,
    source: 'qdrant' | 'nebula' | 'common',
    failedItems?: number
  ): DatabaseEvent {
    return DatabaseEventManager.createEvent(
      DatabaseEventType.BATCH_OPERATION_COMPLETED,
      source,
      {
        operationType,
        batchSize,
        success,
        duration,
        failedItems
      }
    );
  }
}