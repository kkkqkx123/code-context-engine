import { injectable, inject } from 'inversify';
import { EventListener } from '../../types';
import { IDatabaseService, IConnectionManager, IProjectManager } from './IDatabaseService';
import { DatabaseLoggerService } from './DatabaseLoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { DatabaseEventType, Subscription } from './DatabaseEventTypes';
import { DatabaseError, DatabaseErrorType } from './DatabaseError';
import { PerformanceMonitor } from '../../infrastructure/monitoring/PerformanceMonitor';

/**
 * 抽象数据库服务基类
 * 提供通用功能的默认实现，如事件发射、错误处理、日志记录等
 */
@injectable()
export abstract class AbstractDatabaseService implements IDatabaseService {
  protected eventListeners: Map<string, EventListener[]> = new Map();
  protected initialized = false;
  protected connectionManager: IConnectionManager;
  protected projectManager: IProjectManager;
  protected eventSubscriptions: Subscription[] = [];
  protected databaseLogger: DatabaseLoggerService;
  protected errorHandler: ErrorHandlerService;
  protected performanceMonitor: PerformanceMonitor;

  constructor(
    @inject('IConnectionManager') connectionManager: IConnectionManager,
    @inject('IProjectManager') projectManager: IProjectManager,
    @inject('DatabaseLoggerService') databaseLogger: DatabaseLoggerService,
    @inject('ErrorHandlerService') errorHandler: ErrorHandlerService,
    @inject('PerformanceMonitor') performanceMonitor: PerformanceMonitor
  ) {
    this.connectionManager = connectionManager;
    this.projectManager = projectManager;
    this.databaseLogger = databaseLogger;
    this.errorHandler = errorHandler;
    this.performanceMonitor = performanceMonitor;
    this.setupEventForwarding();
  }

  /**
   * 初始化数据库服务
   */
  async initialize(): Promise<boolean> {
    try {
      if (this.initialized) {
        return true;
      }

      // 初始化连接管理器
      const connectionInitialized = await this.connectionManager.connect();
      if (!connectionInitialized) {
        throw DatabaseError.connectionError(
          'Failed to initialize connection manager',
          { 
            component: this.constructor.name, 
            operation: 'initialize'
          }
        );
      }

      // 初始化项目管理器
      // 注意：这里不直接初始化projectManager，因为它可能依赖于连接
      // 具体的初始化逻辑由子类处理

      this.initialized = true;
      
      // 记录初始化事件
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SERVICE_INITIALIZED,
        source: this.getServiceType(),
        timestamp: new Date(),
        data: { message: `${this.constructor.name} initialized successfully` }
      });

      this.emitEvent('initialized', { timestamp: new Date() });
      return true;
    } catch (error) {
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        { 
          component: this.constructor.name, 
          operation: 'initialize'
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.ERROR_OCCURRED,
        source: this.getServiceType(),
        timestamp: new Date(),
        data: { message: 'Service initialization failed', error: dbError.message },
        error: dbError
      });

      this.emitEvent('error', dbError);
      return false;
    }
  }

  /**
   * 检查连接状态
   */
  isConnected(): boolean {
    return this.connectionManager.isConnected();
  }

  /**
   * 关闭数据库服务
   */
  async close(): Promise<void> {
    try {
      await this.connectionManager.disconnect();
      this.initialized = false;
      
      // 记录关闭事件
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.CONNECTION_CLOSED,
        source: this.getServiceType(),
        timestamp: new Date(),
        data: { message: `${this.constructor.name} closed` }
      });

      this.emitEvent('closed', { timestamp: new Date() });
    } catch (error) {
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        { 
          component: this.constructor.name, 
          operation: 'close'
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      throw dbError;
    }
  }

  /**
   * 创建项目空间
   */
  async createProjectSpace(projectPath: string, config?: any): Promise<boolean> {
    this.ensureInitialized();
    
    const startTime = Date.now();
    try {
      const result = await this.projectManager.createProjectSpace(projectPath, config);
      
      const operationDuration = Date.now() - startTime;
      this.performanceMonitor.recordOperation('createProjectSpace', operationDuration);
      
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SPACE_CREATED,
        source: this.getServiceType(),
        timestamp: new Date(),
        data: { 
          message: `Project space created for: ${projectPath}`,
          projectPath,
          operationDuration
        }
      });

      this.emitEvent('project_space_created', { projectPath, duration: operationDuration });
      return result;
    } catch (error) {
      const operationDuration = Date.now() - startTime;
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        { 
          component: this.constructor.name, 
          operation: 'createProjectSpace',
          details: { projectPath, config }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.ERROR_OCCURRED,
        source: this.getServiceType(),
        timestamp: new Date(),
        data: { 
          message: `Failed to create project space for: ${projectPath}`,
          projectPath,
          operationDuration,
          error: dbError.message 
        },
        error: dbError
      });

      this.emitEvent('error', dbError);
      return false;
    }
  }

  /**
   * 删除项目空间
   */
  async deleteProjectSpace(projectPath: string): Promise<boolean> {
    this.ensureInitialized();
    
    const startTime = Date.now();
    try {
      const result = await this.projectManager.deleteProjectSpace(projectPath);
      
      const operationDuration = Date.now() - startTime;
      this.performanceMonitor.recordOperation('deleteProjectSpace', operationDuration);
      
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SPACE_DELETED,
        source: this.getServiceType(),
        timestamp: new Date(),
        data: { 
          message: `Project space deleted for: ${projectPath}`,
          projectPath,
          operationDuration
        }
      });

      this.emitEvent('project_space_deleted', { projectPath, operationDuration });
      return result;
    } catch (error) {
      const operationDuration = Date.now() - startTime;
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        { 
          component: this.constructor.name, 
          operation: 'deleteProjectSpace',
          details: { projectPath }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.ERROR_OCCURRED,
        source: this.getServiceType(),
        timestamp: new Date(),
        data: { 
          message: `Failed to delete project space for: ${projectPath}`,
          projectPath,
          operationDuration,
          error: dbError.message 
        },
        error: dbError
      });

      this.emitEvent('error', dbError);
      return false;
    }
  }

 /**
   * 获取项目空间信息
   */
  async getProjectSpaceInfo(projectPath: string): Promise<any> {
    this.ensureInitialized();
    
    const startTime = Date.now();
    try {
      const result = await this.projectManager.getProjectSpaceInfo(projectPath);
      
      const operationDuration = Date.now() - startTime;
      this.performanceMonitor.recordOperation('getProjectSpaceInfo', operationDuration);
      
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.INFO_RETRIEVED,
        source: this.getServiceType(),
        timestamp: new Date(),
        data: { 
          message: `Project space info retrieved for: ${projectPath}`,
          projectPath,
          operationDuration
        }
      });

      this.emitEvent('project_space_info_retrieved', { projectPath, operationDuration });
      return result;
    } catch (error) {
      const operationDuration = Date.now() - startTime;
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        { 
          component: this.constructor.name, 
          operation: 'getProjectSpaceInfo',
          details: { projectPath }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.ERROR_OCCURRED,
        source: this.getServiceType(),
        timestamp: new Date(),
        data: { 
          message: `Failed to get project space info for: ${projectPath}`,
          projectPath,
          operationDuration,
          error: dbError.message 
        },
        error: dbError
      });

      this.emitEvent('error', dbError);
      throw error;
    }
  }

  /**
   * 插入数据
   */
  async insertData(projectPath: string, data: any): Promise<boolean> {
    this.ensureInitialized();
    
    const startTime = Date.now();
    try {
      const result = await this.projectManager.insertProjectData(projectPath, data);
      
      const operationDuration = Date.now() - startTime;
      this.performanceMonitor.recordOperation('insertData', operationDuration);
      
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_INSERTED,
        source: this.getServiceType(),
        timestamp: new Date(),
        data: { 
          message: `Data inserted for project: ${projectPath}`,
          projectPath,
          dataSize: JSON.stringify(data).length,
          operationDuration
        }
      });

      this.emitEvent('data_inserted', { projectPath, dataSize: JSON.stringify(data).length, operationDuration });
      return result;
    } catch (error) {
      const operationDuration = Date.now() - startTime;
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        { 
          component: this.constructor.name, 
          operation: 'insertData',
          details: { projectPath, dataSize: JSON.stringify(data).length }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.ERROR_OCCURRED,
        source: this.getServiceType(),
        timestamp: new Date(),
        data: { 
          message: `Failed to insert data for project: ${projectPath}`,
          projectPath,
          operationDuration,
          error: dbError.message 
        },
        error: dbError
      });

      this.emitEvent('error', dbError);
      return false;
    }
  }

  /**
   * 更新数据
   */
  async updateData(projectPath: string, id: string, data: any): Promise<boolean> {
    this.ensureInitialized();
    
    const startTime = Date.now();
    try {
      const result = await this.projectManager.updateProjectData(projectPath, id, data);
      
      const operationDuration = Date.now() - startTime;
      this.performanceMonitor.recordOperation('updateData', operationDuration);
      
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_UPDATED,
        source: this.getServiceType(),
        timestamp: new Date(),
        data: { 
          message: `Data updated for project: ${projectPath}, id: ${id}`,
          projectPath,
          id,
          dataSize: JSON.stringify(data).length,
          operationDuration
        }
      });

      this.emitEvent('data_updated', { projectPath, id, dataSize: JSON.stringify(data).length, operationDuration });
      return result;
    } catch (error) {
      const operationDuration = Date.now() - startTime;
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        { 
          component: this.constructor.name, 
          operation: 'updateData',
          details: { projectPath, id, dataSize: JSON.stringify(data).length }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.ERROR_OCCURRED,
        source: this.getServiceType(),
        timestamp: new Date(),
        data: { 
          message: `Failed to update data for project: ${projectPath}, id: ${id}`,
          projectPath,
          id,
          operationDuration,
          error: dbError.message 
        },
        error: dbError
      });

      this.emitEvent('error', dbError);
      return false;
    }
  }

  /**
   * 删除数据
   */
  async deleteData(projectPath: string, id: string): Promise<boolean> {
    this.ensureInitialized();
    
    const startTime = Date.now();
    try {
      const result = await this.projectManager.deleteProjectData(projectPath, id);
      
      const operationDuration = Date.now() - startTime;
      this.performanceMonitor.recordOperation('deleteData', operationDuration);
      
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_DELETED,
        source: this.getServiceType(),
        timestamp: new Date(),
        data: { 
          message: `Data deleted for project: ${projectPath}, id: ${id}`,
          projectPath,
          id,
          operationDuration
        }
      });

      this.emitEvent('data_deleted', { projectPath, id, operationDuration });
      return result;
    } catch (error) {
      const operationDuration = Date.now() - startTime;
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        { 
          component: this.constructor.name, 
          operation: 'deleteData',
          details: { projectPath, id }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.ERROR_OCCURRED,
        source: this.getServiceType(),
        timestamp: new Date(),
        data: { 
          message: `Failed to delete data for project: ${projectPath}, id: ${id}`,
          projectPath,
          id,
          operationDuration,
          error: dbError.message 
        },
        error: dbError
      });

      this.emitEvent('error', dbError);
      return false;
    }
  }

  /**
   * 搜索数据
   */
  async searchData(projectPath: string, query: any): Promise<any[]> {
    this.ensureInitialized();
    
    const startTime = Date.now();
    try {
      const result = await this.projectManager.searchProjectData(projectPath, query);
      
      const operationDuration = Date.now() - startTime;
      this.performanceMonitor.recordOperation('searchData', operationDuration);
      
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_QUERIED,
        source: this.getServiceType(),
        timestamp: new Date(),
        data: { 
          message: `Data searched for project: ${projectPath}`,
          projectPath,
          querySize: JSON.stringify(query).length,
          resultCount: result.length,
          operationDuration
        }
      });

      this.emitEvent('data_queried', { projectPath, querySize: JSON.stringify(query).length, resultCount: result.length, operationDuration });
      return result;
    } catch (error) {
      const operationDuration = Date.now() - startTime;
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        { 
          component: this.constructor.name, 
          operation: 'searchData',
          details: { projectPath, querySize: JSON.stringify(query).length }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.ERROR_OCCURRED,
        source: this.getServiceType(),
        timestamp: new Date(),
        data: { 
          message: `Failed to search data for project: ${projectPath}`,
          projectPath,
          operationDuration,
          error: dbError.message 
        },
        error: dbError
      });

      this.emitEvent('error', dbError);
      throw error;
    }
  }

  /**
   * 根据ID获取数据
   */
  async getDataById(projectPath: string, id: string): Promise<any> {
    this.ensureInitialized();
    
    const startTime = Date.now();
    try {
      const result = await this.projectManager.getProjectDataById(projectPath, id);
      
      const operationDuration = Date.now() - startTime;
      this.performanceMonitor.recordOperation('getDataById', operationDuration);
      
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_RETRIEVED,
        source: this.getServiceType(),
        timestamp: new Date(),
        data: { 
          message: `Data retrieved by ID for project: ${projectPath}, id: ${id}`,
          projectPath,
          id,
          operationDuration
        }
      });

      this.emitEvent('data_retrieved', { projectPath, id, operationDuration });
      return result;
    } catch (error) {
      const operationDuration = Date.now() - startTime;
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        { 
          component: this.constructor.name, 
          operation: 'getDataById',
          details: { projectPath, id }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.ERROR_OCCURRED,
        source: this.getServiceType(),
        timestamp: new Date(),
        data: { 
          message: `Failed to get data by ID for project: ${projectPath}, id: ${id}`,
          projectPath,
          id,
          operationDuration,
          error: dbError.message 
        },
        error: dbError
      });

      this.emitEvent('error', dbError);
      throw error;
    }
  }

  /**
  * 订阅事件
  */
  subscribe(eventType: string, listener: EventListener): Subscription {
  if (!this.eventListeners.has(eventType)) {
  this.eventListeners.set(eventType, []);
  }
  this.eventListeners.get(eventType)!.push(listener);

  // 同时在子组件上创建订阅
  const connectionSub = this.connectionManager.subscribe(eventType, listener);
  const projectSub = this.projectManager.subscribe(eventType, listener);

    // 返回一个订阅对象，允许取消订阅
    const subscription: Subscription = {
     id: `${eventType}_${Date.now()}_${Math.random()}`,
     eventType,
      handler: listener,
    unsubscribe: () => {
      const listeners = this.eventListeners.get(eventType);
    if (listeners) {
      const index = listeners.indexOf(listener);
    if (index > -1) {
        listeners.splice(index, 1);
        }

        // 取消子组件的订阅
        connectionSub.unsubscribe();
        projectSub.unsubscribe();
        }
      }
    };

   return subscription;
  }

  /**
   * 健康检查
   */
  async healthCheck(): Promise<{
    status: 'healthy' | 'unhealthy';
    details?: any;
    error?: string;
  }> {
    try {
      const connectionStatus = this.isConnected();
      const projectSpaces = await this.listProjectSpaces();

      const healthStatus = {
        status: connectionStatus ? 'healthy' as const : 'unhealthy' as const,
        details: {
          connected: connectionStatus,
          projectSpacesCount: projectSpaces.length,
          initialized: this.initialized,
          serviceType: this.getServiceType()
        }
      };

      // 记录健康检查事件
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.HEALTH_CHECK,
        source: this.getServiceType(),
        timestamp: new Date(),
        data: { 
          message: `Health check completed with status: ${healthStatus.status}`,
          ...healthStatus.details
        }
      });

      return healthStatus;
    } catch (error) {
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        { 
          component: this.constructor.name, 
          operation: 'healthCheck'
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      return {
        status: 'unhealthy',
        error: dbError.message
      };
    }
  }

  /**
   * 列出项目空间
   */
  protected async listProjectSpaces(): Promise<any[]> {
    return this.projectManager.listProjectSpaces();
  }

  /**
   * 确保服务已初始化
   */
  protected ensureInitialized(): void {
    if (!this.initialized) {
      throw DatabaseError.internalError(
        `${this.constructor.name} is not initialized`,
        { 
          component: this.constructor.name, 
          operation: 'ensureInitialized'
        }
      );
    }
  }

  /**
   * 发出事件
   */
  protected emitEvent(eventType: string, data: any): void {
    const listeners = this.eventListeners.get(eventType);
    if (listeners) {
      listeners.forEach(listener => {
        try {
          listener(data);
        } catch (error) {
          // 记录事件监听器中的错误
          this.databaseLogger.logDatabaseEvent({
            type: DatabaseEventType.ERROR_OCCURRED,
            source: this.getServiceType(),
            timestamp: new Date(),
            data: {
              message: 'Error in event listener',
              eventType,
              error: error instanceof Error ? error.message : String(error)
            }
          }).catch(logError => {
            // 如果日志记录失败，我们不希望影响主流程
            console.error('Failed to log event listener error:', logError);
          });
        }
      });
    }
  }

  /**
   * 设置事件转发
   * 将连接管理器和项目管理器的事件转发到服务级别
   */
  private setupEventForwarding(): void {
    // 定义连接管理器事件映射
    const connectionEvents = [
      { source: 'connected', target: 'connection_connected' },
      { source: 'disconnected', target: 'connection_disconnected' },
      { source: 'error', target: 'connection_error' }
    ];

    // 定义项目管理器事件映射
    const projectEvents = [
      { source: 'space_created', target: 'project_space_created' },
      { source: 'space_deleted', target: 'project_space_deleted' },
      { source: 'data_inserted', target: 'data_inserted' },
      { source: 'data_updated', target: 'data_updated' },
      { source: 'data_deleted', target: 'data_deleted' },
      { source: 'error', target: 'project_error' }
    ];

    // 批量设置连接管理器事件转发
    this.setupEventForwardingForManager(this.connectionManager, connectionEvents);

    // 批量设置项目管理器事件转发
    this.setupEventForwardingForManager(this.projectManager, projectEvents);
  }

  /**
   * 为指定管理器批量设置事件转发
   * @param manager 事件管理器（连接管理器或项目管理器）
   * @param events 事件映射数组
   */
  private setupEventForwardingForManager(manager: any, events: Array<{source: string, target: string}>): void {
    events.forEach(({ source, target }) => {
      this.eventSubscriptions.push(
        manager.subscribe(source, (data: any) => {
          this.emitEvent(target, data);
        })
      );
    });
  }

  /**
   * 获取服务类型（由子类实现）
   */
  protected abstract getServiceType(): 'qdrant' | 'nebula' | 'common';
}