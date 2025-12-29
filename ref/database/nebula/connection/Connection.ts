import { EventEmitter } from 'events';
import { NebulaConfig } from '../NebulaTypes';

// 连接状态枚举
export enum ConnectionState {
  IDLE = 'idle',
  BUSY = 'busy',
  CLOSING = 'closing',
  CLOSED = 'closed',
  ERROR = 'error'
}

// 连接统计信息
export interface ConnectionStats {
  id: string;
  state: ConnectionState;
  created: Date;
  lastUsed: Date;
  queryCount: number;
  errorCount: number;
  totalQueryTime: number;
}

/**
 * Nebula Graph连接类
 * 封装单个数据库连接的生命周期管理
 */
export class Connection extends EventEmitter {
  private id: string;
  private config: NebulaConfig;
  private client: any; // Nebula Graph客户端实例
  private state: ConnectionState;
  private stats: ConnectionStats;
  private healthCheckInterval: NodeJS.Timeout | null = null;
  private lastActivity: Date;

  constructor(config: NebulaConfig, client: any) {
    super();
    this.id = `conn_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    this.config = config;
    this.client = client;
    this.state = ConnectionState.IDLE;
    this.lastActivity = new Date();
    
    this.stats = {
      id: this.id,
      state: this.state,
      created: new Date(),
      lastUsed: new Date(),
      queryCount: 0,
      errorCount: 0,
      totalQueryTime: 0
    };

    // 启动健康检查
    this.startHealthCheck();
  }

  /**
   * 获取连接ID
   */
  getId(): string {
    return this.id;
  }

  /**
   * 获取连接状态
   */
  getState(): ConnectionState {
    return this.state;
  }

  /**
   * 获取连接统计信息
   */
  getStats(): ConnectionStats {
    return {
      ...this.stats,
      state: this.state,
      lastUsed: this.lastActivity
    };
  }

  /**
   * 获取底层客户端
   */
  getClient(): any {
    return this.client;
  }

  /**
   * 执行查询
   */
  async execute(query: string): Promise<any> {
    if (this.state !== ConnectionState.IDLE) {
      throw new Error(`Connection is not available for queries. Current state: ${this.state}`);
    }

    this.state = ConnectionState.BUSY;
    this.lastActivity = new Date();
    const startTime = Date.now();

    try {
      this.emit('queryStart', { connectionId: this.id, query });
      
      const result = await this.client.execute(query);
      
      const executionTime = Date.now() - startTime;
      this.stats.queryCount++;
      this.stats.totalQueryTime += executionTime;
      this.stats.lastUsed = new Date();
      
      this.emit('querySuccess', { 
        connectionId: this.id, 
        query, 
        executionTime,
        result 
      });
      
      return result;
    } catch (error) {
      this.stats.errorCount++;
      this.emit('queryError', { 
        connectionId: this.id, 
        query, 
        error 
      });
      throw error;
    } finally {
      this.state = ConnectionState.IDLE;
    }
  }

  /**
   * 检查连接是否健康
   */
  async isHealthy(): Promise<boolean> {
    try {
      // 执行简单的健康检查查询
      await this.client.execute('YIELD 1 AS health_check;');
      return true;
    } catch (error) {
      this.emit('healthCheckFailed', { connectionId: this.id, error });
      return false;
    }
  }

  /**
   * 关闭连接
   */
  async close(): Promise<void> {
    if (this.state === ConnectionState.CLOSED || this.state === ConnectionState.CLOSING) {
      return;
    }

    this.state = ConnectionState.CLOSING;
    
    // 停止健康检查
    this.stopHealthCheck();

    try {
      if (this.client && typeof this.client.close === 'function') {
        await this.client.close();
      }
      
      this.state = ConnectionState.CLOSED;
      this.emit('closed', { connectionId: this.id });
    } catch (error) {
      this.state = ConnectionState.ERROR;
      this.emit('closeError', { connectionId: this.id, error });
      throw error;
    }
  }

  /**
   * 标记连接为错误状态
   */
  markAsError(error?: Error): void {
    this.state = ConnectionState.ERROR;
    this.stats.errorCount++;
    this.emit('error', { connectionId: this.id, error });
  }

  /**
   * 重置连接状态
   */
  reset(): void {
    if (this.state === ConnectionState.CLOSED) {
      throw new Error('Cannot reset a closed connection');
    }
    
    this.state = ConnectionState.IDLE;
    this.emit('reset', { connectionId: this.id });
  }

  /**
   * 获取空闲时间（毫秒）
   */
  getIdleTime(): number {
    return Date.now() - this.lastActivity.getTime();
  }

  /**
   * 启动健康检查
   */
  private startHealthCheck(): void {
    // 每30秒检查一次连接健康状态
    this.healthCheckInterval = setInterval(async () => {
      if (this.state === ConnectionState.IDLE) {
        const isHealthy = await this.isHealthy();
        if (!isHealthy) {
          this.markAsError(new Error('Health check failed'));
        }
      }
    }, 30000);

    // 确保定时器不会阻止进程退出
    if (this.healthCheckInterval.unref) {
      this.healthCheckInterval.unref();
    }
  }

  /**
   * 停止健康检查
   */
  private stopHealthCheck(): void {
    if (this.healthCheckInterval) {
      clearInterval(this.healthCheckInterval);
      this.healthCheckInterval = null;
    }
  }
}