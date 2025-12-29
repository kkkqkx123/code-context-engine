import { injectable, inject } from 'inversify';
import { EventListener } from '../../types';
import { IDatabaseService, IConnectionManager, IProjectManager } from './IDatabaseService';
import { Subscription } from './DatabaseEventTypes';

/**
 * 数据库服务基础实现类
 * 提供公共逻辑和默认实现
 */
@injectable()
export abstract class BaseDatabaseService implements IDatabaseService {
  protected connectionManager: IConnectionManager;
  protected projectManager: IProjectManager;
  protected eventListeners: Map<string, EventListener[]> = new Map();
  protected initialized: boolean = false;

  constructor(
    @inject('IConnectionManager') connectionManager: IConnectionManager,
    @inject('IProjectManager') projectManager: IProjectManager
  ) {
    this.connectionManager = connectionManager;
    this.projectManager = projectManager;
    this.setupEventForwarding();
  }

  /**
   * 初始化数据库服务
   */
  async initialize(): Promise<boolean> {
    if (this.initialized) {
      return true;
    }

    try {
      // 初始化连接管理器
      const connectionInitialized = await this.connectionManager.connect();
      if (!connectionInitialized) {
        this.emitEvent('error', new Error('Failed to initialize connection manager'));
        return false;
      }

      this.initialized = true;
      this.emitEvent('initialized', { timestamp: new Date() });
      return true;
    } catch (error) {
      this.emitEvent('error', error);
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
      this.emitEvent('closed', { timestamp: new Date() });
    } catch (error) {
      this.emitEvent('error', error);
      throw error;
    }
  }

  /**
   * 创建项目空间
   */
  async createProjectSpace(projectPath: string, config?: any): Promise<boolean> {
    this.ensureInitialized();
    return this.projectManager.createProjectSpace(projectPath, config);
  }

  /**
   * 删除项目空间
   */
  async deleteProjectSpace(projectPath: string): Promise<boolean> {
    this.ensureInitialized();
    return this.projectManager.deleteProjectSpace(projectPath);
  }

  /**
   * 获取项目空间信息
   */
  async getProjectSpaceInfo(projectPath: string): Promise<any> {
    this.ensureInitialized();
    return this.projectManager.getProjectSpaceInfo(projectPath);
  }

  /**
   * 插入数据
   */
  async insertData(projectPath: string, data: any): Promise<boolean> {
    this.ensureInitialized();
    return this.projectManager.insertProjectData(projectPath, data);
  }

  /**
   * 更新数据
   */
  async updateData(projectPath: string, id: string, data: any): Promise<boolean> {
    this.ensureInitialized();
    return this.projectManager.updateProjectData(projectPath, id, data);
  }

  /**
   * 删除数据
   */
  async deleteData(projectPath: string, id: string): Promise<boolean> {
    this.ensureInitialized();
    return this.projectManager.deleteProjectData(projectPath, id);
  }

  /**
   * 搜索数据
   */
  async searchData(projectPath: string, query: any): Promise<any[]> {
    this.ensureInitialized();
    return this.projectManager.searchProjectData(projectPath, query);
  }

  /**
   * 根据ID获取数据
   */
  async getDataById(projectPath: string, id: string): Promise<any> {
    this.ensureInitialized();
    return this.projectManager.getProjectDataById(projectPath, id);
  }

  

  /**
  * 订阅事件
  */
  subscribe(eventType: string, listener: EventListener): Subscription {
    if (!this.eventListeners.has(eventType)) {
      this.eventListeners.set(eventType, []);
    }
    this.eventListeners.get(eventType)!.push(listener);

    // 同时在子组件上创建订阅并保存引用
    const connectionSub = this.connectionManager.subscribe(eventType, listener);
    const projectSub = this.projectManager.subscribe(eventType, listener);

    // 返回一个订阅对象，允许取消订阅
    const subscription = {
      id: `${eventType}_${Date.now()}`, // 简单的ID生成
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
          initialized: this.initialized
        }
      };

      return healthStatus;
    } catch (error) {
      return {
        status: 'unhealthy',
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * 列出所有项目空间
   */
  protected async listProjectSpaces(): Promise<any[]> {
    return this.projectManager.listProjectSpaces();
  }

  /**
   * 确保服务已初始化
   */
  protected ensureInitialized(): void {
    if (!this.initialized) {
      throw new Error('Database service is not initialized');
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
          console.error(`Error in event listener for ${eventType}:`, error);
        }
      });
    }
  }

  /**
   * 设置事件转发
   * 将连接管理器和项目管理器的事件转发到服务级别
   */
  private setupEventForwarding(): void {
    // 转发连接管理器事件
    this.connectionManager.subscribe('connected', (data: any) => {
      this.emitEvent('connection_connected', data);
    });

    this.connectionManager.subscribe('disconnected', (data: any) => {
      this.emitEvent('connection_disconnected', data);
    });

    this.connectionManager.subscribe('error', (error: any) => {
      this.emitEvent('connection_error', error);
    });

    // 转发项目管理器事件
    this.projectManager.subscribe('space_created', (data: any) => {
      this.emitEvent('project_space_created', data);
    });

    this.projectManager.subscribe('space_deleted', (data: any) => {
      this.emitEvent('project_space_deleted', data);
    });

    this.projectManager.subscribe('data_inserted', (data: any) => {
      this.emitEvent('data_inserted', data);
    });

    this.projectManager.subscribe('data_updated', (data: any) => {
      this.emitEvent('data_updated', data);
    });

    this.projectManager.subscribe('data_deleted', (data: any) => {
      this.emitEvent('data_deleted', data);
    });

    this.projectManager.subscribe('error', (error: any) => {
      this.emitEvent('project_error', error);
    });
  }
}