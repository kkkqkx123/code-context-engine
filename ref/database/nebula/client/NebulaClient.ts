import { injectable, inject } from 'inversify';
import { EventEmitter } from 'events';
import { TYPES } from '../../../types';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { NebulaConfigService } from '../../../config/service/NebulaConfigService';
import { PerformanceMonitor } from '../../../infrastructure/monitoring/PerformanceMonitor';
import { NebulaConfig, NebulaConnectionStatus, NebulaQueryResult } from '../NebulaTypes';
import { IQueryRunner } from '../query/QueryRunner';

// 查询批次接口
export interface QueryBatch {
  query: string;
  params?: Record<string, any>;
}

// 查询选项接口
export interface QueryOptions {
  timeout?: number;
  retryAttempts?: number;
  useCache?: boolean;
}

// NebulaClient 接口
export interface INebulaClient {
  // 基础操作
  initialize(config?: any): Promise<boolean>;
  isConnected(): boolean;
  isInitialized(): boolean;
  close(): Promise<void>;
  reconnect(): Promise<boolean>;

  // 连接管理
  connect(): Promise<void>;
  disconnect(): Promise<void>;

  // 查询执行
  execute(query: string, params?: Record<string, any>, options?: QueryOptions): Promise<any>;
  executeQuery(query: string, params?: Record<string, any>, options?: QueryOptions): Promise<any>;
  executeBatch(queries: QueryBatch[]): Promise<any[]>;

  // 统计信息
  getStats(): any;

  // 配置管理
  updateConfig(config: Partial<any>): void;
  getConfig(): any;

  // 事件订阅
  on(event: string, listener: Function): void;
  off(event: string, listener: Function): void;
  emit(event: string, ...args: any[]): boolean;

  // 健康检查
  healthCheck(): Promise<{
    status: 'healthy' | 'unhealthy';
    details?: any;
    error?: string;
  }>;
}

/**
 * Nebula Graph 底层客户端
 * 职责：连接管理、查询执行、基本事件
 */
@injectable()
export class NebulaClient extends EventEmitter implements INebulaClient {
  private logger: LoggerService;
  private errorHandler: ErrorHandlerService;
  private configService: NebulaConfigService;
  private performanceMonitor: PerformanceMonitor;
  private queryRunner: IQueryRunner;
  private config: NebulaConfig;
  private connectionStatus: NebulaConnectionStatus;
  private isConnectedFlag: boolean = false;
  private connectionPool: any;
  private sessionManager: any;

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.NebulaConfigService) configService: NebulaConfigService,
    @inject(TYPES.PerformanceMonitor) performanceMonitor: PerformanceMonitor,
    @inject(TYPES.IQueryRunner) queryRunner: IQueryRunner,
    @inject(TYPES.IConnectionPool) connectionPool: any,
    @inject(TYPES.ISessionManager) sessionManager: any
  ) {
    super();
    this.logger = logger;
    this.errorHandler = errorHandler;
    this.configService = configService;
    this.performanceMonitor = performanceMonitor;
    this.queryRunner = queryRunner;
    this.connectionPool = connectionPool;
    this.sessionManager = sessionManager;

    // 加载配置
    this.config = this.configService.loadConfig();
    this.connectionStatus = {
      connected: false,
      host: this.config.host,
      port: this.config.port,
      username: this.config.username,
      space: this.config.space
    };

    this.logger.info('NebulaClient initialized', {
      host: this.config.host,
      port: this.config.port,
      username: this.config.username,
      space: this.config.space
    });
  }

  /**
   * 连接到Nebula Graph数据库
   */
  async connect(): Promise<void> {
    if (this.isConnectedFlag) {
      this.logger.warn('Already connected to Nebula Graph');
      return;
    }

    try {
      this.logger.info('Connecting to Nebula Graph', {
        host: this.config.host,
        port: this.config.port,
        username: this.config.username
      });

      // TODO: 实现实际的连接逻辑
      // 这里将在后续实现ConnectionPool后完成
      this.isConnectedFlag = true;
      this.connectionStatus.connected = true;
      this.connectionStatus.lastConnected = new Date();

      this.emit('connected', this.connectionStatus);
      this.logger.info('Successfully connected to Nebula Graph');
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.connectionStatus.error = errorMessage;

      this.errorHandler.handleError(
        new Error(`Failed to connect to Nebula Graph: ${errorMessage}`),
        { component: 'NebulaClient', operation: 'connect' }
      );

      this.emit('error', error);
      throw error;
    }
  }

  /**
   * 断开与Nebula Graph数据库的连接
   */
  async disconnect(): Promise<void> {
    if (!this.isConnectedFlag) {
      this.logger.warn('Not connected to Nebula Graph');
      return;
    }

    try {
      this.logger.info('Disconnecting from Nebula Graph');

      // TODO: 实现实际的断开连接逻辑
      // 这里将在后续实现ConnectionPool后完成
      this.isConnectedFlag = false;
      this.connectionStatus.connected = false;
      this.connectionStatus.space = undefined;

      this.emit('disconnected');
      this.logger.info('Successfully disconnected from Nebula Graph');
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);

      this.errorHandler.handleError(
        new Error(`Failed to disconnect from Nebula Graph: ${errorMessage}`),
        { component: 'NebulaClient', operation: 'disconnect' }
      );

      this.emit('error', error);
      throw error;
    }
  }

  /**
   * 检查是否已连接
   */
  isConnected(): boolean {
    return this.isConnectedFlag;
  }

  /**
   * 执行查询
   */
  async execute(
    query: string,
    params?: Record<string, any>,
    options?: QueryOptions
  ): Promise<NebulaQueryResult> {
    if (!this.isConnectedFlag) {
      throw new Error('Not connected to Nebula Graph');
    }

    const startTime = Date.now();
    const operationId = this.performanceMonitor.startOperation('nebula_query', {
      query: query.substring(0, 100),
      hasParams: !!params,
      options
    });

    try {
      this.logger.debug('Executing query', {
        query: query.substring(0, 100),
        hasParams: !!params
      });

      // 使用QueryRunner执行查询
      const result = await this.queryRunner.execute(query, params, options);

      const executionTime = Date.now() - startTime;
      this.performanceMonitor.endOperation(operationId, {
        success: true,
        resultCount: result.data?.length || 0
      });

      this.emit('queryExecuted', { query, params, result, executionTime });
      return result;
    } catch (error) {
      const executionTime = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);

      this.performanceMonitor.endOperation(operationId, {
        success: false,
        error: errorMessage
      });

      this.errorHandler.handleError(
        new Error(`Failed to execute query: ${errorMessage}`),
        { component: 'NebulaClient', operation: 'execute', query, params }
      );

      this.emit('queryError', { query, params, error, executionTime });
      throw error;
    }
  }

  /**
   * 执行批量查询
   */
  async executeBatch(queries: QueryBatch[]): Promise<NebulaQueryResult[]> {
    if (!this.isConnectedFlag) {
      throw new Error('Not connected to Nebula Graph');
    }

    const startTime = Date.now();
    const operationId = this.performanceMonitor.startOperation('nebula_batch_query', {
      queryCount: queries.length
    });

    try {
      this.logger.debug('Executing batch queries', { queryCount: queries.length });

      // 使用QueryRunner执行批量查询
      const results = await this.queryRunner.executeBatch(queries);

      const executionTime = Date.now() - startTime;
      this.performanceMonitor.endOperation(operationId, {
        success: true,
        resultCount: results.length
      });

      this.emit('batchQueryExecuted', { queries, results, executionTime });
      return results;
    } catch (error) {
      const executionTime = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);

      this.performanceMonitor.endOperation(operationId, {
        success: false,
        error: errorMessage
      });

      this.errorHandler.handleError(
        new Error(`Failed to execute batch queries: ${errorMessage}`),
        { component: 'NebulaClient', operation: 'executeBatch', queries }
      );

      this.emit('batchQueryError', { queries, error, executionTime });
      throw error;
    }
  }


  /**
   * 更新配置
   */
  updateConfig(config: Partial<NebulaConfig>): void {
    this.config = { ...this.config, ...config };
    this.connectionStatus.host = this.config.host;
    this.connectionStatus.port = this.config.port;
    this.connectionStatus.username = this.config.username;
    this.connectionStatus.space = this.config.space;

    this.logger.info('Configuration updated', { config });
    this.emit('configUpdated', { config: this.config });
  }

  /**
   * 获取当前配置
   */
  getConfig(): NebulaConfig {
    return { ...this.config };
  }

  /**
   * 获取连接状态
   */
  getConnectionStatus(): NebulaConnectionStatus {
    return { ...this.connectionStatus };
  }

  /**
   * 初始化客户端
   */
  async initialize(config?: NebulaConfig): Promise<boolean> {
    try {
      if (config) {
        this.logger.info('Initializing NebulaClient', config);

        // 更新配置
        this.config = { ...this.config, ...config };
        this.connectionStatus.host = this.config.host;
        this.connectionStatus.port = this.config.port;
        this.connectionStatus.username = this.config.username;
        this.connectionStatus.space = this.config.space;

        // 初始化连接池和会话管理器
        await this.connectionPool.initialize(config);
        await this.sessionManager.initialize(config);
      } else {
        // 使用现有配置初始化
        await this.connectionPool.initialize(this.config);
        await this.sessionManager.initialize(this.config);
      }

      // 尝试连接
      await this.connect();

      this.logger.info('NebulaClient initialized successfully');
      return true;
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Failed to initialize NebulaClient'),
        { component: 'NebulaClient', operation: 'initialize' }
      );
      return false;
    }
  }

  /**
   * 检查服务是否已初始化
   */
  isInitialized(): boolean {
    return this.isConnected();
  }

  /**
   * 尝试重新连接到Nebula数据库
   */
  async reconnect(): Promise<boolean> {
    try {
      this.logger.info('Attempting to reconnect to Nebula Graph');

      // 断开现有连接
      await this.disconnect();

      // 等待一段时间再重连
      await new Promise(resolve => setTimeout(resolve, 1000));

      // 重新连接
      await this.connect();

      this.logger.info('Successfully reconnected to Nebula Graph');
      return true;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to reconnect to Nebula Graph: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'NebulaClient', operation: 'reconnect' }
      );
      return false;
    }
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
      const stats = this.getStats();
      
      return {
        status: connectionStatus ? 'healthy' : 'unhealthy',
        details: {
          connected: connectionStatus,
          stats,
          lastCheck: new Date()
        }
      };
    } catch (error) {
      return {
        status: 'unhealthy',
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }


  /**
   * 执行查询（别名方法）
   */
  async executeQuery(
    query: string,
    params?: Record<string, any>,
    options?: QueryOptions
  ): Promise<NebulaQueryResult> {
    return this.execute(query, params, options);
  }

  /**
   * 获取统计信息
   */
  getStats(): any {
    return {
      connectionPool: this.connectionPool?.getStats?.() || {},
      sessionManager: this.sessionManager?.getStats?.() || {},
      queryRunner: this.queryRunner?.getStats?.() || {},
      transactionManager: {} // 如果有事务管理器的话
    };
  }

  /**
   * 关闭客户端
   */
  async close(): Promise<void> {
    try {
      this.logger.info('Closing NebulaClient');

      // 关闭会话管理器
      await this.sessionManager.close();

      // 关闭连接池
      await this.connectionPool.close();

      // 断开连接
      await this.disconnect();

      this.logger.info('NebulaClient closed successfully');
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Failed to close NebulaClient'),
        { component: 'NebulaClient', operation: 'close' }
      );
      throw error;
    }
  }

 /**
 * 确保已连接
 */
private ensureConnected(): void {
  if (!this.isConnectedFlag) {
    throw new Error('Not connected to Nebula Graph');
  }
}

/**
 * 获取数据库统计信息
 */
async getDatabaseStats(): Promise<any> {
  this.ensureConnected();
  
  // 获取当前连接状态和基本统计
  const stats = this.getStats();
  
  // 尝试获取更多数据库特定的统计信息
  try {
    const spacesResult = await this.execute('SHOW SPACES');
    const spaces = spacesResult?.data || [];
    
    return {
      ...stats,
      spaces: spaces.length,
      connected: this.isConnectedFlag,
      connectionStatus: this.connectionStatus
    };
  } catch (error) {
    this.logger.error('Failed to get database stats', error);
    return {
      ...stats,
      error: error instanceof Error ? error.message : String(error),
      connected: this.isConnectedFlag
    };
  }
}

/**
 * 删除项目空间
 */
async deleteSpaceForProject(projectPath: string): Promise<boolean> {
  try {
    this.logger.info('Deleting space for project', { projectPath });
    
    // 生成空间名称
    const spaceName = projectPath.startsWith('project_')
      ? projectPath
      : `project_${projectPath.replace(/[^a-zA-Z0-9_]/g, '_').toLowerCase()}`;
    
    // 执行删除空间的查询
    await this.execute(`DROP SPACE IF EXISTS ${spaceName}`);
    
    this.logger.info('Successfully deleted space for project', { projectPath, spaceName });
    return true;
  } catch (error) {
    this.errorHandler.handleError(
      error instanceof Error ? error : new Error(String(error)),
      { component: 'NebulaClient', operation: 'deleteSpaceForProject', projectPath }
    );
    return false;
  }
}
}