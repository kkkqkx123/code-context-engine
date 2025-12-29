import { EventEmitter } from 'events';
import { Connection, ConnectionState } from './Connection';

// 健康检查配置
export interface HealthCheckConfig {
  interval: number; // 检查间隔（毫秒）
  timeout: number; // 检查超时（毫秒）
  maxFailures: number; // 最大失败次数
  retryDelay: number; // 重试延迟（毫秒）
}

// 健康检查结果
export interface HealthCheckResult {
  connectionId: string;
  healthy: boolean;
  responseTime: number;
  error?: Error;
  timestamp: Date;
}

// 默认健康检查配置
const DEFAULT_HEALTH_CHECK_CONFIG: HealthCheckConfig = {
  interval: 30000, // 30秒
  timeout: 5000,   // 5秒
  maxFailures: 3,  // 最多失败3次
  retryDelay: 1000 // 1秒
};

/**
 * 连接健康检查器
 * 负责定期检查连接池中连接的健康状态
 */
export class ConnectionHealthChecker extends EventEmitter {
  private config: HealthCheckConfig;
  private connections: Map<string, Connection> = new Map();
  private healthStatus: Map<string, { failures: number; lastCheck: Date }> = new Map();
  private checkInterval: NodeJS.Timeout | null = null;
  private isRunning: boolean = false;

  constructor(config: Partial<HealthCheckConfig> = {}) {
    super();
    this.config = { ...DEFAULT_HEALTH_CHECK_CONFIG, ...config };
  }

  /**
   * 启动健康检查
   */
  start(): void {
    if (this.isRunning) {
      return;
    }

    this.isRunning = true;
    this.checkInterval = setInterval(() => {
      this.performHealthCheck();
    }, this.config.interval);

    // 确保定时器不会阻止进程退出
    if (this.checkInterval.unref) {
      this.checkInterval.unref();
    }

    this.emit('started');
  }

  /**
   * 停止健康检查
   */
  stop(): void {
    if (!this.isRunning) {
      return;
    }

    this.isRunning = false;
    
    if (this.checkInterval) {
      clearInterval(this.checkInterval);
      this.checkInterval = null;
    }

    this.emit('stopped');
  }

  /**
   * 添加连接到健康检查
   */
  addConnection(connection: Connection): void {
    this.connections.set(connection.getId(), connection);
    this.healthStatus.set(connection.getId(), { failures: 0, lastCheck: new Date() });
    
    // 监听连接事件
    connection.on('closed', () => {
      this.removeConnection(connection.getId());
    });

    connection.on('error', () => {
      this.markUnhealthy(connection.getId(), new Error('Connection error'));
    });

    this.emit('connectionAdded', { connectionId: connection.getId() });
  }

  /**
   * 从健康检查中移除连接
   */
  removeConnection(connectionId: string): void {
    this.connections.delete(connectionId);
    this.healthStatus.delete(connectionId);
    this.emit('connectionRemoved', { connectionId });
  }

  /**
   * 手动检查单个连接的健康状态
   */
  async checkConnection(connectionId: string): Promise<HealthCheckResult> {
    const connection = this.connections.get(connectionId);
    if (!connection) {
      throw new Error(`Connection ${connectionId} not found`);
    }

    const startTime = Date.now();
    
    try {
      const isHealthy = await this.performConnectionCheck(connection);
      const responseTime = Date.now() - startTime;
      
      const result: HealthCheckResult = {
        connectionId,
        healthy: isHealthy,
        responseTime,
        timestamp: new Date()
      };

      if (isHealthy) {
        this.resetFailures(connectionId);
        this.emit('connectionHealthy', result);
      } else {
        this.incrementFailures(connectionId);
        this.emit('connectionUnhealthy', result);
      }

      return result;
    } catch (error) {
      const responseTime = Date.now() - startTime;
      const result: HealthCheckResult = {
        connectionId,
        healthy: false,
        responseTime,
        error: error instanceof Error ? error : new Error(String(error)),
        timestamp: new Date()
      };

      this.incrementFailures(connectionId);
      this.emit('connectionUnhealthy', result);
      
      return result;
    }
  }

  /**
   * 获取所有连接的健康状态
   */
  getHealthStatus(): Map<string, { failures: number; lastCheck: Date }> {
    return new Map(this.healthStatus);
  }

  /**
   * 获取健康检查配置
   */
  getConfig(): HealthCheckConfig {
    return { ...this.config };
  }

  /**
   * 更新健康检查配置
   */
  updateConfig(config: Partial<HealthCheckConfig>): void {
    this.config = { ...this.config, ...config };
    
    // 如果正在运行，重启以应用新配置
    if (this.isRunning) {
      this.stop();
      this.start();
    }

    this.emit('configUpdated', { config: this.config });
  }

  /**
   * 标记连接为不健康
   */
  markUnhealthy(connectionId: string, error: Error): void {
    this.incrementFailures(connectionId);
    this.emit('connectionUnhealthy', {
      connectionId,
      healthy: false,
      responseTime: 0,
      error,
      timestamp: new Date()
    });
  }

  /**
   * 执行批量健康检查
   */
  private async performHealthCheck(): Promise<void> {
    const checkPromises = Array.from(this.connections.keys()).map(connectionId => 
      this.checkConnection(connectionId).catch(error => {
        // 记录错误但不抛出，避免影响其他连接的检查
        this.emit('checkError', { connectionId, error });
      })
    );

    await Promise.allSettled(checkPromises);
  }

  /**
   * 执行单个连接的健康检查
   */
  private async performConnectionCheck(connection: Connection): Promise<boolean> {
    // 如果连接已经关闭或处于错误状态，直接返回false
    if (connection.getState() === ConnectionState.CLOSED || 
        connection.getState() === ConnectionState.ERROR) {
      return false;
    }

    try {
      // 设置超时
      const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => reject(new Error('Health check timeout')), this.config.timeout);
      });

      // 执行健康检查
      const healthCheckPromise = connection.isHealthy();
      
      return await Promise.race([healthCheckPromise, timeoutPromise]);
    } catch (error) {
      return false;
    }
  }

  /**
   * 增加连接的失败次数
   */
  private incrementFailures(connectionId: string): void {
    const status = this.healthStatus.get(connectionId);
    if (status) {
      status.failures++;
      status.lastCheck = new Date();
      
      // 如果失败次数超过阈值，标记连接为错误状态
      if (status.failures >= this.config.maxFailures) {
        const connection = this.connections.get(connectionId);
        if (connection) {
          connection.markAsError(new Error(`Health check failed ${status.failures} times`));
        }
      }
    }
  }

  /**
   * 重置连接的失败次数
   */
  private resetFailures(connectionId: string): void {
    const status = this.healthStatus.get(connectionId);
    if (status) {
      status.failures = 0;
      status.lastCheck = new Date();
      
      // 如果连接之前处于错误状态，重置它
      const connection = this.connections.get(connectionId);
      if (connection && connection.getState() === ConnectionState.ERROR) {
        connection.reset();
      }
    }
  }
}