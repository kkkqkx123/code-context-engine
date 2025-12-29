import { injectable, inject } from 'inversify';
import { EventEmitter } from 'events';
import { TYPES } from '../../../types';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { NebulaConfigService } from '../../../config/service/NebulaConfigService';
import { PerformanceMonitor } from '../../../infrastructure/monitoring/PerformanceMonitor';
import { NebulaConfig } from '../NebulaTypes';
import { Connection, ConnectionState } from './Connection';
import { ConnectionHealthChecker, HealthCheckConfig } from './ConnectionHealthChecker';
import { ConnectionWarmer, ConnectionWarmingConfig } from './ConnectionWarmer';
import { LoadBalancer, LoadBalancerConfig, LoadBalanceStrategy } from './LoadBalancer';

// 连接池配置
export interface ConnectionPoolConfig {
  minConnections: number;
  maxConnections: number;
  acquireTimeout: number;
  idleTimeout: number;
  healthCheck: HealthCheckConfig;
  warming: ConnectionWarmingConfig;
  loadBalancing: LoadBalancerConfig;
}

// 连接池统计信息
export interface ConnectionPoolStats {
  totalConnections: number;
  activeConnections: number;
  idleConnections: number;
  pendingRequests: number;
  totalAcquires: number;
  totalReleases: number;
  totalErrors: number;
  averageAcquireTime: number;
  averageConnectionAge: number;
}

// 连接请求
interface ConnectionRequest {
  resolve: (connection: Connection) => void;
  reject: (error: Error) => void;
  timestamp: number;
  timeout: NodeJS.Timeout;
}

// 连接池接口
export interface IConnectionPool {
  initialize(config: NebulaConfig): Promise<void>;
  getConnection(): Promise<Connection>;
  releaseConnection(connection: Connection): void;
  close(): Promise<void>;
  startHealthCheck(): void;
  stopHealthCheck(): void;
  getPoolStats(): ConnectionPoolStats;
}

/**
 * Nebula Graph连接池
 * 管理数据库连接的创建、获取、释放和生命周期
 */
@injectable()
export class ConnectionPool extends EventEmitter implements IConnectionPool {
  private logger: LoggerService;
  private errorHandler: ErrorHandlerService;
  private configService: NebulaConfigService;
  private performanceMonitor: PerformanceMonitor;
  private config!: NebulaConfig;
  private poolConfig!: ConnectionPoolConfig;
  
  private connections: Connection[] = [];
  private pendingRequests: ConnectionRequest[] = [];
  private healthChecker!: ConnectionHealthChecker;
  private connectionWarmer: ConnectionWarmer;
  private loadBalancer: LoadBalancer;
  private isInitialized: boolean = false;
  private isClosing: boolean = false;
  
  // 统计信息
  private stats: ConnectionPoolStats = {
    totalConnections: 0,
    activeConnections: 0,
    idleConnections: 0,
    pendingRequests: 0,
    totalAcquires: 0,
    totalReleases: 0,
    totalErrors: 0,
    averageAcquireTime: 0,
    averageConnectionAge: 0
  };
  
  // 性能监控
  private acquireTimes: number[] = [];
  private connectionCreationTimes: number[] = [];

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.NebulaConfigService) configService: NebulaConfigService,
    @inject(TYPES.PerformanceMonitor) performanceMonitor: PerformanceMonitor,
    @inject(TYPES.ConnectionWarmer) connectionWarmer: ConnectionWarmer,
    @inject(TYPES.LoadBalancer) loadBalancer: LoadBalancer
  ) {
    super();
    this.logger = logger;
    this.errorHandler = errorHandler;
    this.configService = configService;
    this.performanceMonitor = performanceMonitor;
    this.connectionWarmer = connectionWarmer;
    this.loadBalancer = loadBalancer;
    
    // 从配置服务获取配置，而不是使用硬编码
    this.loadPoolConfig();
    // 此时 poolConfig 已经被初始化
    this.healthChecker = new ConnectionHealthChecker(this.poolConfig.healthCheck);
    
    this.setupHealthCheckerEvents();
    this.setupLoadBalancerEvents();
    this.connectionWarmer.updateConfig(this.poolConfig.warming);
    this.loadBalancer.updateConfig(this.poolConfig.loadBalancing);
  }
  
  /**
   * 从配置服务加载连接池配置
   */
  private loadPoolConfig(): void {
    const nebulaConfig = this.configService.loadConfig();
    
    // 使用配置服务的值，提供合理的默认值
    this.poolConfig = {
      minConnections: nebulaConfig.connectionPool?.minConnections || 2,
      maxConnections: nebulaConfig.connectionPool?.maxConnections || nebulaConfig.maxConnections || 10,
      acquireTimeout: nebulaConfig.connectionPool?.acquireTimeout || 30000,
      idleTimeout: nebulaConfig.connectionPool?.idleTimeout || 300000,
      healthCheck: {
        interval: nebulaConfig.connectionPool?.healthCheckInterval || 30000,
        timeout: nebulaConfig.connectionPool?.healthCheckTimeout || 5000,
        maxFailures: nebulaConfig.connectionPool?.maxFailures || 3,
        retryDelay: nebulaConfig.retryDelay || 1000,
      },
      warming: {
        enabled: true,
        warmupQueries: ['YIELD 1 AS warmup_test;', 'SHOW SPACES;', 'SHOW HOSTS;'],
        warmupConcurrency: 3,
        warmupTimeout: 10000,
        retryAttempts: nebulaConfig.retryAttempts || 2,
        retryDelay: nebulaConfig.retryDelay || 1000,
      },
      loadBalancing: {
        strategy: LoadBalanceStrategy.LEAST_RESPONSE_TIME,
        healthCheckWeight: 0.3,
        responseTimeWeight: 0.4,
        errorRateWeight: 0.2,
        connectionCountWeight: 0.1,
        weightUpdateInterval: 30000,
        maxWeight: 100,
        minWeight: 1,
      },
    };
  }

  /**
   * 初始化连接池
   */
  async initialize(config: NebulaConfig): Promise<void> {
    if (this.isInitialized) {
      this.logger.warn('Connection pool already initialized');
      return;
    }

    try {
      this.config = config;
      this.logger.info('Initializing connection pool', {
        minConnections: this.poolConfig.minConnections,
        maxConnections: this.poolConfig.maxConnections,
        host: config.host,
        port: config.port
      });

      // 创建最小连接数
      await this.createInitialConnections();
      
      this.isInitialized = true;
      this.emit('initialized');
      this.logger.info('Connection pool initialized successfully');
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Failed to initialize connection pool'),
        { component: 'ConnectionPool', operation: 'initialize' }
      );
      throw error;
    }
  }

  /**
   * 获取连接
   */
  async getConnection(): Promise<Connection> {
    if (!this.isInitialized) {
      throw new Error('Connection pool not initialized');
    }

    if (this.isClosing) {
      throw new Error('Connection pool is closing');
    }

    const startTime = Date.now();
    this.stats.totalAcquires++;

    try {
      // 尝试从空闲连接中获取
      const idleConnection = this.getIdleConnection();
      if (idleConnection) {
        this.stats.activeConnections++;
        this.stats.idleConnections--;
        
        const acquireTime = Date.now() - startTime;
        this.recordAcquireTime(acquireTime);
        
        this.emit('connectionAcquired', { connectionId: idleConnection.getId() });
        return idleConnection;
      }

      // 如果没有空闲连接，尝试创建新连接
      if (this.connections.length < this.poolConfig.maxConnections) {
        const newConnection = await this.createConnection();
        this.stats.activeConnections++;
        this.stats.totalConnections++;
        
        const acquireTime = Date.now() - startTime;
        this.recordAcquireTime(acquireTime);
        
        this.emit('connectionAcquired', { connectionId: newConnection.getId() });
        return newConnection;
      }

      // 如果达到最大连接数，等待连接释放
      return await this.waitForConnection(startTime);
    } catch (error) {
      this.stats.totalErrors++;
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Failed to acquire connection'),
        { component: 'ConnectionPool', operation: 'getConnection' }
      );
      throw error;
    }
  }

  /**
   * 释放连接
   */
  releaseConnection(connection: Connection): void {
    if (!this.isInitialized || this.isClosing) {
      return;
    }

    const connectionIndex = this.connections.indexOf(connection);
    if (connectionIndex === -1) {
      this.logger.warn('Attempted to release connection not managed by pool', {
        connectionId: connection.getId()
      });
      return;
    }

    this.stats.totalReleases++;
    this.stats.activeConnections--;
    this.stats.idleConnections++;

    // 检查连接状态
    if (connection.getState() === ConnectionState.ERROR) {
      this.removeConnection(connection);
      this.tryCreateReplacementConnection();
    } else {
      this.emit('connectionReleased', { connectionId: connection.getId() });
      this.processPendingRequests();
    }
  }

  /**
   * 关闭连接池
   */
  async close(): Promise<void> {
    if (this.isClosing) {
      return;
    }

    this.isClosing = true;
    this.logger.info('Closing connection pool');

    // 停止健康检查
    this.healthChecker.stop();

    // 拒绝所有待处理的请求
    this.rejectAllPendingRequests();

    // 关闭所有连接
    const closePromises = this.connections.map(connection => 
      connection.close().catch(error => {
        this.logger.error('Error closing connection', {
          connectionId: connection.getId(),
          error: error instanceof Error ? error.message : String(error)
        });
      })
    );

    await Promise.allSettled(closePromises);
    
    this.connections = [];
    this.isInitialized = false;
    this.isClosing = false;
    
    this.emit('closed');
    this.logger.info('Connection pool closed');
  }

  /**
   * 启动健康检查
   */
  startHealthCheck(): void {
    if (!this.isInitialized) {
      throw new Error('Connection pool not initialized');
    }

    this.healthChecker.start();
    this.logger.info('Health check started');
  }

  /**
   * 停止健康检查
   */
  stopHealthCheck(): void {
    this.healthChecker.stop();
    this.logger.info('Health check stopped');
  }

  /**
   * 获取连接池统计信息
   */
  getPoolStats(): ConnectionPoolStats {
    this.updateStats();
    return { ...this.stats };
  }

  /**
   * 更新连接池配置
   */
  updatePoolConfig(config: Partial<ConnectionPoolConfig>): void {
    this.poolConfig = { ...this.poolConfig, ...config };
    
    // 更新健康检查配置
    if (config.healthCheck) {
      this.healthChecker.updateConfig(config.healthCheck);
    }

    // 更新连接预热配置
    if (config.warming) {
      this.connectionWarmer.updateConfig(config.warming);
    }

    // 更新负载均衡配置
    if (config.loadBalancing) {
      this.loadBalancer.updateConfig(config.loadBalancing);
    }

    this.emit('configUpdated', { config: this.poolConfig });
  }

  /**
   * 创建初始连接
   */
  private async createInitialConnections(): Promise<void> {
    const creationPromises = [];
    
    for (let i = 0; i < this.poolConfig.minConnections; i++) {
      creationPromises.push(this.createConnection());
    }

    try {
      const connections = await Promise.all(creationPromises);
      this.connections.push(...connections);
      this.stats.totalConnections = connections.length;
      this.stats.idleConnections = connections.length;
      
      this.logger.info(`Created ${connections.length} initial connections`);
    } catch (error) {
      this.logger.error('Failed to create initial connections', { error });
      throw error;
    }
  }

  /**
   * 创建新连接
   */
  private async createConnection(): Promise<Connection> {
    const startTime = Date.now();
    
    try {
      // TODO: 实现实际的Nebula Graph客户端创建
      // 这里需要使用项目原生客户端实现
      const mockClient = {
        execute: async (query: string) => {
          // 模拟查询执行
          return { table: {}, results: [], rows: [], data: [] };
        },
        close: async () => {
          // 模拟关闭连接
        }
      };

      const connection = new Connection(this.config, mockClient);
      
      // 监听连接事件
      this.setupConnectionEvents(connection);
      
      // 添加到健康检查
      this.healthChecker.addConnection(connection);
      
      // 预热连接
      if (this.poolConfig.warming.enabled) {
        await this.warmupConnection(connection);
      }
      
      const creationTime = Date.now() - startTime;
      this.connectionCreationTimes.push(creationTime);
      
      // 保持创建时间数组在合理大小
      if (this.connectionCreationTimes.length > 100) {
        this.connectionCreationTimes = this.connectionCreationTimes.slice(-100);
      }
      
      this.logger.debug('Created new connection', {
        connectionId: connection.getId(),
        creationTime
      });
      
      return connection;
    } catch (error) {
      this.logger.error('Failed to create connection', { error });
      throw error;
    }
  }

  /**
   * 预热连接
   */
  private async warmupConnection(connection: Connection): Promise<void> {
    try {
      const result = await this.connectionWarmer.warmConnection(connection);
      
      if (!result.success) {
        this.logger.warn('Connection warming failed', {
          connectionId: connection.getId(),
          error: result.error
        });
      }
    } catch (error) {
      this.logger.error('Connection warming error', {
        connectionId: connection.getId(),
        error
      });
    }
  }

  /**
   * 获取空闲连接（使用负载均衡器）
   */
  private getIdleConnection(): Connection | null {
    const idleConnections = this.connections.filter(conn =>
      conn.getState() === ConnectionState.IDLE
    );
    
    if (idleConnections.length === 0) {
      return null;
    }
    
    // 使用负载均衡器选择最优连接
    return this.loadBalancer.selectConnection(idleConnections);
  }

  /**
   * 等待连接释放
   */
  private async waitForConnection(startTime: number): Promise<Connection> {
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        const index = this.pendingRequests.findIndex(req => req.resolve === resolve);
        if (index !== -1) {
          this.pendingRequests.splice(index, 1);
          this.stats.pendingRequests--;
        }
        reject(new Error('Connection acquire timeout'));
      }, this.poolConfig.acquireTimeout);

      const request: ConnectionRequest = {
        resolve,
        reject,
        timestamp: startTime,
        timeout
      };

      this.pendingRequests.push(request);
      this.stats.pendingRequests++;
    });
  }

  /**
   * 处理待处理的请求
   */
  private processPendingRequests(): void {
    while (this.pendingRequests.length > 0) {
      const idleConnection = this.getIdleConnection();
      if (!idleConnection) {
        break;
      }

      const request = this.pendingRequests.shift()!;
      clearTimeout(request.timeout);
      this.stats.pendingRequests--;
      this.stats.activeConnections++;
      this.stats.idleConnections--;

      const acquireTime = Date.now() - request.timestamp;
      this.recordAcquireTime(acquireTime);

      request.resolve(idleConnection);
      this.emit('connectionAcquired', { connectionId: idleConnection.getId() });
    }
  }

  /**
   * 拒绝所有待处理的请求
   */
  private rejectAllPendingRequests(): void {
    for (const request of this.pendingRequests) {
      clearTimeout(request.timeout);
      request.reject(new Error('Connection pool is closing'));
    }
    this.pendingRequests = [];
    this.stats.pendingRequests = 0;
  }

  /**
   * 移除连接
   */
  private removeConnection(connection: Connection): void {
    const index = this.connections.indexOf(connection);
    if (index !== -1) {
      this.connections.splice(index, 1);
      this.stats.totalConnections--;
      
      if (connection.getState() === ConnectionState.IDLE) {
        this.stats.idleConnections--;
      } else {
        this.stats.activeConnections--;
      }
      
      this.healthChecker.removeConnection(connection.getId());
      this.emit('connectionRemoved', { connectionId: connection.getId() });
    }
  }

  /**
   * 尝试创建替换连接
   */
  private tryCreateReplacementConnection(): void {
    if (this.connections.length < this.poolConfig.minConnections) {
      this.createConnection()
        .then(connection => {
          this.connections.push(connection);
          this.stats.totalConnections++;
          this.stats.idleConnections++;
          this.processPendingRequests();
        })
        .catch(error => {
          this.logger.error('Failed to create replacement connection', { error });
        });
    }
  }

  /**
   * 设置连接事件监听
   */
  private setupConnectionEvents(connection: Connection): void {
    connection.on('error', (error) => {
      this.logger.error('Connection error', {
        connectionId: connection.getId(),
        error: error instanceof Error ? error.message : String(error)
      });
      
      if (connection.getState() === ConnectionState.IDLE) {
        this.removeConnection(connection);
        this.tryCreateReplacementConnection();
      }
    });

    connection.on('closed', () => {
      this.removeConnection(connection);
      this.tryCreateReplacementConnection();
    });
  }

  /**
   * 设置健康检查器事件监听
   */
  private setupHealthCheckerEvents(): void {
    this.healthChecker.on('connectionUnhealthy', (result) => {
      this.logger.warn('Connection marked as unhealthy', {
        connectionId: result.connectionId,
        error: result.error?.message
      });
    });

    this.healthChecker.on('connectionHealthy', (result) => {
      this.logger.debug('Connection is healthy', {
        connectionId: result.connectionId,
        responseTime: result.responseTime
      });
    });
  }

  /**
   * 设置负载均衡器事件监听
   */
  private setupLoadBalancerEvents(): void {
    this.loadBalancer.on('connectionSelected', (data) => {
      this.logger.debug('Connection selected by load balancer', {
        connectionId: data.connectionId,
        strategy: data.strategy,
        totalConnections: data.totalConnections
      });
    });

    this.loadBalancer.on('updateWeightsRequested', () => {
      // 更新所有连接的权重
      this.connections.forEach(connection => {
        this.loadBalancer.updateConnectionWeight(connection);
      });
    });
  }

  /**
   * 记录获取连接时间
   */
  private recordAcquireTime(time: number): void {
    this.acquireTimes.push(time);
    
    // 保持数组在合理大小
    if (this.acquireTimes.length > 1000) {
      this.acquireTimes = this.acquireTimes.slice(-1000);
    }
    
    // 计算平均获取时间
    if (this.acquireTimes.length > 0) {
      const sum = this.acquireTimes.reduce((acc, time) => acc + time, 0);
      this.stats.averageAcquireTime = sum / this.acquireTimes.length;
    }
  }

  /**
   * 更新统计信息
   */
  private updateStats(): void {
    // 计算平均连接年龄
    if (this.connections.length > 0) {
      const now = Date.now();
      const totalAge = this.connections.reduce((sum, conn) => {
        const stats = conn.getStats();
        return sum + (now - stats.created.getTime());
      }, 0);
      this.stats.averageConnectionAge = totalAge / this.connections.length;
    }
    
    // 记录连接池性能指标
    const operationId = this.performanceMonitor.startOperation('connection_pool_stats', {
      totalConnections: this.stats.totalConnections,
      activeConnections: this.stats.activeConnections,
      idleConnections: this.stats.idleConnections,
      pendingRequests: this.stats.pendingRequests,
      averageAcquireTime: this.stats.averageAcquireTime,
      averageConnectionAge: this.stats.averageConnectionAge
    });
    this.performanceMonitor.endOperation(operationId);
  }
}