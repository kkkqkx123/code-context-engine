import { EventEmitter } from 'events';
import { Connection } from '../connection/Connection';
import { NebulaQueryResult } from '../NebulaTypes';

// 会话状态枚举
export enum SessionState {
  ACTIVE = 'active',
  IDLE = 'idle',
  CLOSING = 'closing',
  CLOSED = 'closed',
  ERROR = 'error'
}

// 会话统计信息
export interface SessionStats {
  id: string;
  state: SessionState;
  spaceName?: string;
  created: Date;
  lastUsed: Date;
  queryCount: number;
  errorCount: number;
  totalQueryTime: number;
  connectionId: string;
}

/**
 * Nebula Graph会话类
 * 封装数据库会话的生命周期管理
 */
export class Session extends EventEmitter {
  private id: string;
  private connection: Connection;
  private spaceName?: string;
  private state: SessionState;
  private stats: SessionStats;
  private lastActivity: Date;

  constructor(connection: Connection) {
    super();
    this.id = `session_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    this.connection = connection;
    this.state = SessionState.IDLE;
    this.lastActivity = new Date();
    
    this.stats = {
      id: this.id,
      state: this.state,
      created: new Date(),
      lastUsed: new Date(),
      queryCount: 0,
      errorCount: 0,
      totalQueryTime: 0,
      connectionId: connection.getId()
    };

    // 监听连接事件
    this.setupConnectionEvents();
  }

  /**
   * 获取会话ID
   */
  getId(): string {
    return this.id;
  }

  /**
   * 获取会话状态
   */
  getState(): SessionState {
    return this.state;
  }

  /**
   * 获取会话统计信息
   */
  getStats(): SessionStats {
    return {
      ...this.stats,
      state: this.state,
      lastUsed: this.lastActivity,
      spaceName: this.spaceName
    };
  }

  /**
   * 获取当前空间名称
   */
  getSpaceName(): string | undefined {
    return this.spaceName;
  }

  /**
   * 获取关联的连接
   */
  getConnection(): Connection {
    return this.connection;
  }

  /**
   * 执行查询
   */
  async execute(query: string): Promise<NebulaQueryResult> {
    if (this.state !== SessionState.IDLE && this.state !== SessionState.ACTIVE) {
      throw new Error(`Session is not available for queries. Current state: ${this.state}`);
    }

    this.state = SessionState.ACTIVE;
    this.lastActivity = new Date();
    const startTime = Date.now();

    try {
      this.emit('queryStart', { sessionId: this.id, query });
      
      const result = await this.connection.execute(query);
      
      const executionTime = Date.now() - startTime;
      this.stats.queryCount++;
      this.stats.totalQueryTime += executionTime;
      this.stats.lastUsed = new Date();
      
      this.state = SessionState.IDLE;
      
      this.emit('querySuccess', { 
        sessionId: this.id, 
        query, 
        executionTime,
        result 
      });
      
      return result;
    } catch (error) {
      this.stats.errorCount++;
      this.state = SessionState.ERROR;
      
      this.emit('queryError', { 
        sessionId: this.id, 
        query, 
        error 
      });
      
      throw error;
    }
  }

  /**
   * 切换到指定空间
   */
  async switchSpace(spaceName: string): Promise<void> {
    if (this.state === SessionState.CLOSED || this.state === SessionState.CLOSING) {
      throw new Error('Cannot switch space on a closed session');
    }

    try {
      this.emit('spaceSwitchStart', { sessionId: this.id, spaceName });
      
      const useQuery = `USE \`${spaceName}\`;`;
      await this.execute(useQuery);
      
      this.spaceName = spaceName;
      this.emit('spaceSwitched', { sessionId: this.id, spaceName });
    } catch (error) {
      this.emit('spaceSwitchError', { 
        sessionId: this.id, 
        spaceName, 
        error 
      });
      throw error;
    }
  }

  /**
   * 检查会话是否健康
   */
  async isHealthy(): Promise<boolean> {
    try {
      // 检查连接是否健康
      const connectionHealthy = await this.connection.isHealthy();
      if (!connectionHealthy) {
        return false;
      }

      // 如果有空间名称，验证空间是否仍然可用
      if (this.spaceName) {
        await this.connection.execute(`USE \`${this.spaceName}\`;`);
      }

      return true;
    } catch (error) {
      this.emit('healthCheckFailed', { sessionId: this.id, error });
      return false;
    }
  }

  /**
   * 关闭会话
   */
  async close(): Promise<void> {
    if (this.state === SessionState.CLOSED || this.state === SessionState.CLOSING) {
      return;
    }

    this.state = SessionState.CLOSING;

    try {
      this.emit('closing', { sessionId: this.id });
      
      // 释放连接回连接池
      // 注意：这里不关闭连接，而是释放它
      // 连接的关闭由连接池管理
      
      this.state = SessionState.CLOSED;
      this.emit('closed', { sessionId: this.id });
    } catch (error) {
      this.state = SessionState.ERROR;
      this.emit('closeError', { sessionId: this.id, error });
      throw error;
    }
  }

  /**
   * 标记会话为错误状态
   */
  markAsError(error?: Error): void {
    this.state = SessionState.ERROR;
    this.stats.errorCount++;
    this.emit('error', { sessionId: this.id, error });
  }

  /**
   * 重置会话状态
   */
  reset(): void {
    if (this.state === SessionState.CLOSED) {
      throw new Error('Cannot reset a closed session');
    }
    
    this.state = SessionState.IDLE;
    this.emit('reset', { sessionId: this.id });
  }

  /**
   * 获取空闲时间（毫秒）
   */
  getIdleTime(): number {
    return Date.now() - this.lastActivity.getTime();
  }

  /**
   * 获取会话年龄（毫秒）
   */
  getAge(): number {
    return Date.now() - this.stats.created.getTime();
  }

  /**
   * 设置连接事件监听
   */
  private setupConnectionEvents(): void {
    this.connection.on('error', (error) => {
      this.markAsError(error);
    });

    this.connection.on('closed', () => {
      this.state = SessionState.CLOSED;
      this.emit('connectionClosed', { sessionId: this.id });
    });
  }
}