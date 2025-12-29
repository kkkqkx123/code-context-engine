import { injectable, inject } from 'inversify';
import { DatabaseLoggerService } from './DatabaseLoggerService';
import { DatabaseEvent, DatabaseEventType, QdrantEventType, NebulaEventType } from './DatabaseEventTypes';
import { TYPES } from '../../types';

/**
 * 事件到日志的桥接类
 * 将数据库事件系统与日志系统集成
 */
@injectable()
export class EventToLogBridge {
  private databaseLogger: DatabaseLoggerService;
  private eventToLogMapping: Map<string, { level: string; template: string }> = new Map();

  constructor(
    @inject(TYPES.DatabaseLoggerService) databaseLogger: DatabaseLoggerService
  ) {
    this.databaseLogger = databaseLogger;
    this.initializeEventMapping();
  }

  /**
   * 初始化事件到日志的映射
   */
  private initializeEventMapping(): void {
    // 基础数据库事件映射
    this.eventToLogMapping.set(DatabaseEventType.CONNECTION_OPENED, { 
      level: 'info', 
      template: 'Database connection opened' 
    });
    
    this.eventToLogMapping.set(DatabaseEventType.CONNECTION_CLOSED, { 
      level: 'info', 
      template: 'Database connection closed' 
    });
    
    this.eventToLogMapping.set(DatabaseEventType.CONNECTION_FAILED, { 
      level: 'error', 
      template: 'Database connection failed' 
    });
    
    this.eventToLogMapping.set(DatabaseEventType.CONNECTION_ERROR, { 
      level: 'error', 
      template: 'Database connection error' 
    });
    
    this.eventToLogMapping.set(DatabaseEventType.SPACE_CREATED, { 
      level: 'info', 
      template: 'Database space created' 
    });
    
    this.eventToLogMapping.set(DatabaseEventType.SPACE_DELETED, { 
      level: 'info', 
      template: 'Database space deleted' 
    });
    
    this.eventToLogMapping.set(DatabaseEventType.SPACE_ERROR, { 
      level: 'error', 
      template: 'Database space error' 
    });
    
    this.eventToLogMapping.set(DatabaseEventType.DATA_INSERTED, { 
      level: 'debug', 
      template: 'Data inserted into database' 
    });
    
    this.eventToLogMapping.set(DatabaseEventType.DATA_UPDATED, { 
      level: 'debug', 
      template: 'Data updated in database' 
    });
    
    this.eventToLogMapping.set(DatabaseEventType.DATA_DELETED, { 
      level: 'debug', 
      template: 'Data deleted from database' 
    });
    
    this.eventToLogMapping.set(DatabaseEventType.DATA_QUERIED, { 
      level: 'debug', 
      template: 'Data queried from database' 
    });
    
    this.eventToLogMapping.set(DatabaseEventType.DATA_ERROR, { 
      level: 'error', 
      template: 'Database data operation error' 
    });
    
    this.eventToLogMapping.set(DatabaseEventType.SERVICE_INITIALIZED, { 
      level: 'info', 
      template: 'Database service initialized' 
    });
    
    this.eventToLogMapping.set(DatabaseEventType.SERVICE_ERROR, { 
      level: 'error', 
      template: 'Database service error' 
    });
    
    this.eventToLogMapping.set(DatabaseEventType.PERFORMANCE_METRIC, { 
      level: 'info', 
      template: 'Database performance metric' 
    });
    
    this.eventToLogMapping.set(DatabaseEventType.QUERY_EXECUTED, { 
      level: 'debug', 
      template: 'Database query executed' 
    });
    
    this.eventToLogMapping.set(DatabaseEventType.BATCH_OPERATION_COMPLETED, { 
      level: 'info', 
      template: 'Database batch operation completed' 
    });
    
    this.eventToLogMapping.set(DatabaseEventType.ERROR_OCCURRED, { 
      level: 'error', 
      template: 'Database error occurred' 
    });

    // Qdrant事件映射
    this.eventToLogMapping.set(QdrantEventType.COLLECTION_CREATED, { 
      level: 'info', 
      template: 'Qdrant collection created' 
    });
    
    this.eventToLogMapping.set(QdrantEventType.COLLECTION_DELETED, { 
      level: 'info', 
      template: 'Qdrant collection deleted' 
    });
    
    this.eventToLogMapping.set(QdrantEventType.COLLECTION_UPDATED, { 
      level: 'info', 
      template: 'Qdrant collection updated' 
    });
    
    this.eventToLogMapping.set(QdrantEventType.COLLECTION_ERROR, { 
      level: 'error', 
      template: 'Qdrant collection error' 
    });
    
    this.eventToLogMapping.set(QdrantEventType.VECTOR_INSERTED, { 
      level: 'debug', 
      template: 'Qdrant vectors inserted' 
    });
    
    this.eventToLogMapping.set(QdrantEventType.VECTOR_UPDATED, { 
      level: 'debug', 
      template: 'Qdrant vectors updated' 
    });
    
    this.eventToLogMapping.set(QdrantEventType.VECTOR_DELETED, { 
      level: 'debug', 
      template: 'Qdrant vectors deleted' 
    });
    
    this.eventToLogMapping.set(QdrantEventType.VECTOR_SEARCHED, { 
      level: 'debug', 
      template: 'Qdrant vectors searched' 
    });
    
    this.eventToLogMapping.set(QdrantEventType.INDEX_CREATED, { 
      level: 'info', 
      template: 'Qdrant index created' 
    });
    
    this.eventToLogMapping.set(QdrantEventType.INDEX_DELETED, { 
      level: 'info', 
      template: 'Qdrant index deleted' 
    });
    
    this.eventToLogMapping.set(QdrantEventType.INDEX_UPDATED, { 
      level: 'info', 
      template: 'Qdrant index updated' 
    });

    // Nebula事件映射
    this.eventToLogMapping.set(NebulaEventType.SPACE_CREATED, { 
      level: 'info', 
      template: 'Nebula space created' 
    });
    
    this.eventToLogMapping.set(NebulaEventType.SPACE_DELETED, { 
      level: 'info', 
      template: 'Nebula space deleted' 
    });
    
    this.eventToLogMapping.set(NebulaEventType.SPACE_UPDATED, { 
      level: 'info', 
      template: 'Nebula space updated' 
    });
    
    this.eventToLogMapping.set(NebulaEventType.SPACE_ERROR, { 
      level: 'error', 
      template: 'Nebula space error' 
    });
    
    this.eventToLogMapping.set(NebulaEventType.NODE_INSERTED, { 
      level: 'debug', 
      template: 'Nebula node inserted' 
    });
    
    this.eventToLogMapping.set(NebulaEventType.NODE_UPDATED, { 
      level: 'debug', 
      template: 'Nebula node updated' 
    });
    
    this.eventToLogMapping.set(NebulaEventType.NODE_DELETED, { 
      level: 'debug', 
      template: 'Nebula node deleted' 
    });
    
    this.eventToLogMapping.set(NebulaEventType.RELATIONSHIP_INSERTED, { 
      level: 'debug', 
      template: 'Nebula relationship inserted' 
    });
    
    this.eventToLogMapping.set(NebulaEventType.RELATIONSHIP_UPDATED, { 
      level: 'debug', 
      template: 'Nebula relationship updated' 
    });
    
    this.eventToLogMapping.set(NebulaEventType.RELATIONSHIP_DELETED, { 
      level: 'debug', 
      template: 'Nebula relationship deleted' 
    });
    
    this.eventToLogMapping.set(NebulaEventType.QUERY_EXECUTED, { 
      level: 'debug', 
      template: 'Nebula query executed' 
    });
    
    this.eventToLogMapping.set(NebulaEventType.QUERY_ERROR, { 
      level: 'error', 
      template: 'Nebula query error' 
    });
  }

  /**
   * 桥接事件到日志
   * @param event 数据库事件
   */
  async bridgeEvent(event: DatabaseEvent): Promise<void> {
    const mapping = this.eventToLogMapping.get(event.type.toString());
    
    if (mapping) {
      const message = mapping.template;
      const meta = {
        source: event.source,
        timestamp: event.timestamp,
        data: event.data,
        error: event.error ? event.error.message : undefined
      };

      // 根据映射的日志级别调用相应的日志方法
      switch (mapping.level) {
        case 'error':
          await this.databaseLogger.logDatabaseEvent(event);
          break;
        case 'warn':
          await this.databaseLogger.logDatabaseEvent(event);
          break;
        case 'info':
          await this.databaseLogger.logDatabaseEvent(event);
          break;
        case 'debug':
          await this.databaseLogger.logDatabaseEvent(event);
          break;
        default:
          await this.databaseLogger.logDatabaseEvent(event);
      }
    } else {
      // 如果没有找到映射，使用默认方式记录事件
      await this.databaseLogger.logDatabaseEvent(event);
    }
  }

  /**
   * 添加自定义事件映射
   * @param eventType 事件类型
   * @param level 日志级别
   * @param template 日志模板
   */
  addEventMapping(eventType: string, level: string, template: string): void {
    this.eventToLogMapping.set(eventType, { level, template });
  }

  /**
   * 移除事件映射
   * @param eventType 事件类型
   */
  removeEventMapping(eventType: string): void {
    this.eventToLogMapping.delete(eventType);
  }
}