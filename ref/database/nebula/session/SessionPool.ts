import { EventEmitter } from 'events';
import { Connection, ConnectionState } from '../connection/Connection';
import { Session, SessionState } from './Session';

// 会话池配置
export interface SessionPoolConfig {
  maxSessionsPerConnection: number;
  sessionTimeout: number;
  idleTimeout: number;
  cleanupInterval: number;
}

// 会话池统计信息
export interface SessionPoolStats {
  totalSessions: number;
  activeSessions: number;
  idleSessions: number;
  closedSessions: number;
  errorSessions: number;
  totalAcquires: number;
  totalReleases: number;
  totalCreated: number;
  totalDestroyed: number;
  averageSessionAge: number;
}

// 空间会话映射
interface SpaceSessionMap {
  [spaceName: string]: Session[];
}

// 默认会话池配置
const DEFAULT_SESSION_POOL_CONFIG: SessionPoolConfig = {
  maxSessionsPerConnection: 5,
  sessionTimeout: 1800000, // 30分钟
  idleTimeout: 300000,     // 5分钟
  cleanupInterval: 60000  // 1分钟
};

/**
 * 会话池
 * 管理会话的创建、获取、释放和生命周期
 */
export class SessionPool extends EventEmitter {
  private config: SessionPoolConfig;
  private connection: Connection;
  private sessions: Session[] = [];
  private spaceSessions: SpaceSessionMap = {};
  private cleanupInterval: NodeJS.Timeout | null = null;
  private isClosing: boolean = false;
  
  // 统计信息
  private stats: SessionPoolStats = {
    totalSessions: 0,
    activeSessions: 0,
    idleSessions: 0,
    closedSessions: 0,
    errorSessions: 0,
    totalAcquires: 0,
    totalReleases: 0,
    totalCreated: 0,
    totalDestroyed: 0,
    averageSessionAge: 0
  };

  constructor(connection: Connection, config: Partial<SessionPoolConfig> = {}) {
    super();
    this.connection = connection;
    this.config = { ...DEFAULT_SESSION_POOL_CONFIG, ...config };
    
    // 启动清理任务
    this.startCleanupTask();
    
    // 监听连接事件
    this.setupConnectionEvents();
  }

  /**
   * 获取会话
   */
  async getSession(spaceName?: string): Promise<Session> {
    if (this.isClosing) {
      throw new Error('Session pool is closing');
    }

    this.stats.totalAcquires++;

    try {
      // 尝试从指定空间的会话池中获取空闲会话
      if (spaceName) {
        const spaceSession = this.getIdleSessionFromSpace(spaceName);
        if (spaceSession) {
          this.stats.activeSessions++;
          this.stats.idleSessions--;
          
          this.emit('sessionAcquired', { 
            sessionId: spaceSession.getId(), 
            spaceName 
          });
          
          return spaceSession;
        }
      }

      // 尝试从通用会话池中获取空闲会话
      const idleSession = this.getIdleSession();
      if (idleSession) {
        this.stats.activeSessions++;
        this.stats.idleSessions--;
        
        // 如果指定了空间且会话不在该空间，切换空间
        if (spaceName && idleSession.getSpaceName() !== spaceName) {
          await idleSession.switchSpace(spaceName);
          this.addSessionToSpace(spaceName, idleSession);
        }
        
        this.emit('sessionAcquired', { 
          sessionId: idleSession.getId(), 
          spaceName: idleSession.getSpaceName() 
        });
        
        return idleSession;
      }

      // 如果没有空闲会话，创建新会话
      if (this.sessions.length < this.config.maxSessionsPerConnection) {
        const newSession = await this.createSession(spaceName);
        this.stats.activeSessions++;
        this.stats.totalSessions++;
        
        this.emit('sessionAcquired', { 
          sessionId: newSession.getId(), 
          spaceName: newSession.getSpaceName() 
        });
        
        return newSession;
      }

      throw new Error('Maximum session limit reached');
    } catch (error) {
      this.emit('acquireError', { spaceName, error });
      throw error;
    }
  }

  /**
   * 释放会话
   */
  releaseSession(session: Session): void {
    if (this.isClosing) {
      return;
    }

    const sessionIndex = this.sessions.indexOf(session);
    if (sessionIndex === -1) {
      this.emit('releaseError', { 
        sessionId: session.getId(), 
        error: new Error('Session not managed by pool') 
      });
      return;
    }

    this.stats.totalReleases++;
    this.stats.activeSessions--;

    // 检查会话状态
    const sessionState = session.getState();
    if (sessionState === SessionState.ERROR) {
      this.destroySession(session);
    } else if (sessionState === SessionState.CLOSED) {
      this.removeSession(session);
      this.stats.closedSessions++;
    } else {
      this.stats.idleSessions++;
      this.emit('sessionReleased', { 
        sessionId: session.getId(), 
        spaceName: session.getSpaceName() 
      });
    }
  }

  /**
   * 销毁会话
   */
  async destroySession(session: Session): Promise<void> {
    const sessionIndex = this.sessions.indexOf(session);
    if (sessionIndex === -1) {
      return;
    }

    try {
      await session.close();
      this.removeSession(session);
      this.stats.totalDestroyed++;
      
      this.emit('sessionDestroyed', { 
        sessionId: session.getId(), 
        spaceName: session.getSpaceName() 
      });
    } catch (error) {
      this.emit('destroyError', { 
        sessionId: session.getId(), 
        error 
      });
    }
  }

  /**
   * 使会话失效
   */
  invalidateSession(sessionId: string): void {
    const session = this.sessions.find(s => s.getId() === sessionId);
    if (session) {
      session.markAsError(new Error('Session invalidated'));
      this.destroySession(session);
    }
  }

  /**
   * 关闭会话池
   */
  async close(): Promise<void> {
    if (this.isClosing) {
      return;
    }

    this.isClosing = true;
    
    // 停止清理任务
    this.stopCleanupTask();

    // 关闭所有会话
    const closePromises = this.sessions.map(session => 
      session.close().catch(error => {
        this.emit('closeError', { 
          sessionId: session.getId(), 
          error 
        });
      })
    );

    await Promise.allSettled(closePromises);
    
    this.sessions = [];
    this.spaceSessions = {};
    this.isClosing = false;
    
    this.emit('closed');
  }

  /**
   * 获取会话池统计信息
   */
  getPoolStats(): SessionPoolStats {
    this.updateStats();
    return { ...this.stats };
  }

  /**
   * 获取指定空间的会话数量
   */
  getSpaceSessionCount(spaceName: string): number {
    return this.spaceSessions[spaceName]?.length || 0;
  }

  /**
   * 获取所有空间名称
   */
  getSpaceNames(): string[] {
    return Object.keys(this.spaceSessions);
  }

  /**
   * 创建新会话
   */
  private async createSession(spaceName?: string): Promise<Session> {
    const session = new Session(this.connection);
    
    // 设置会话事件监听
    this.setupSessionEvents(session);
    
    // 添加到会话列表
    this.sessions.push(session);
    this.stats.totalCreated++;
    
    // 如果指定了空间，切换到该空间
    if (spaceName) {
      await session.switchSpace(spaceName);
      this.addSessionToSpace(spaceName, session);
    }
    
    this.emit('sessionCreated', { 
      sessionId: session.getId(), 
      spaceName: session.getSpaceName() 
    });
    
    return session;
  }

  /**
   * 从指定空间获取空闲会话
   */
  private getIdleSessionFromSpace(spaceName: string): Session | null {
    const spaceSessionList = this.spaceSessions[spaceName];
    if (!spaceSessionList) {
      return null;
    }

    for (let i = 0; i < spaceSessionList.length; i++) {
      const session = spaceSessionList[i];
      if (session.getState() === SessionState.IDLE) {
        return session;
      }
    }

    return null;
  }

  /**
   * 获取空闲会话
   */
  private getIdleSession(): Session | null {
    for (const session of this.sessions) {
      if (session.getState() === SessionState.IDLE) {
        return session;
      }
    }
    return null;
  }

  /**
   * 添加会话到空间映射
   */
  private addSessionToSpace(spaceName: string, session: Session): void {
    if (!this.spaceSessions[spaceName]) {
      this.spaceSessions[spaceName] = [];
    }
    
    const spaceSessionList = this.spaceSessions[spaceName];
    if (!spaceSessionList.includes(session)) {
      spaceSessionList.push(session);
    }
  }

  /**
   * 从空间映射中移除会话
   */
  private removeSessionFromSpace(session: Session): void {
    const spaceName = session.getSpaceName();
    if (spaceName && this.spaceSessions[spaceName]) {
      const spaceSessionList = this.spaceSessions[spaceName];
      const index = spaceSessionList.indexOf(session);
      if (index !== -1) {
        spaceSessionList.splice(index, 1);
      }
      
      // 如果空间没有会话了，删除空间映射
      if (spaceSessionList.length === 0) {
        delete this.spaceSessions[spaceName];
      }
    }
  }

  /**
   * 移除会话
   */
  private removeSession(session: Session): void {
    const sessionIndex = this.sessions.indexOf(session);
    if (sessionIndex !== -1) {
      this.sessions.splice(sessionIndex, 1);
      this.stats.totalSessions--;
      
      // 更新状态统计
      const sessionState = session.getState();
      if (sessionState === SessionState.ACTIVE) {
        this.stats.activeSessions--;
      } else if (sessionState === SessionState.IDLE) {
        this.stats.idleSessions--;
      } else if (sessionState === SessionState.ERROR) {
        this.stats.errorSessions--;
      }
      
      // 从空间映射中移除
      this.removeSessionFromSpace(session);
    }
  }

  /**
   * 启动清理任务
   */
  private startCleanupTask(): void {
    this.cleanupInterval = setInterval(() => {
      this.performCleanup();
    }, this.config.cleanupInterval);

    // 确保定时器不会阻止进程退出
    if (this.cleanupInterval.unref) {
      this.cleanupInterval.unref();
    }
  }

  /**
   * 停止清理任务
   */
  private stopCleanupTask(): void {
    if (this.cleanupInterval) {
      clearInterval(this.cleanupInterval);
      this.cleanupInterval = null;
    }
  }

  /**
   * 执行清理任务
   */
  private async performCleanup(): Promise<void> {
    const now = Date.now();
    const sessionsToDestroy: Session[] = [];

    for (const session of this.sessions) {
      const sessionState = session.getState();
      
      // 清理超时的会话
      if (session.getAge() > this.config.sessionTimeout) {
        sessionsToDestroy.push(session);
        continue;
      }

      // 清理长时间空闲的会话
      if (sessionState === SessionState.IDLE && 
          session.getIdleTime() > this.config.idleTimeout) {
        sessionsToDestroy.push(session);
        continue;
      }

      // 清理错误状态的会话
      if (sessionState === SessionState.ERROR) {
        sessionsToDestroy.push(session);
        continue;
      }
    }

    // 销毁需要清理的会话
    for (const session of sessionsToDestroy) {
      await this.destroySession(session);
    }

    if (sessionsToDestroy.length > 0) {
      this.emit('cleanup', { 
        destroyedSessions: sessionsToDestroy.length 
      });
    }
  }

  /**
   * 设置会话事件监听
   */
  private setupSessionEvents(session: Session): void {
    session.on('error', (error) => {
      this.emit('sessionError', { 
        sessionId: session.getId(), 
        error 
      });
    });

    session.on('closed', () => {
      this.removeSession(session);
      this.stats.closedSessions++;
    });
  }

  /**
   * 设置连接事件监听
   */
  private setupConnectionEvents(): void {
    this.connection.on('error', (error) => {
      // 连接错误时标记所有会话为错误状态
      for (const session of this.sessions) {
        session.markAsError(error);
      }
    });

    this.connection.on('closed', () => {
      // 连接关闭时关闭所有会话
      this.close();
    });
  }

  /**
   * 更新统计信息
   */
  private updateStats(): void {
    // 计算平均会话年龄
    if (this.sessions.length > 0) {
      const now = Date.now();
      const totalAge = this.sessions.reduce((sum, session) => {
        return sum + session.getAge();
      }, 0);
      this.stats.averageSessionAge = totalAge / this.sessions.length;
    }

    // 更新状态统计
    this.stats.activeSessions = 0;
    this.stats.idleSessions = 0;
    this.stats.errorSessions = 0;

    for (const session of this.sessions) {
      const state = session.getState();
      if (state === SessionState.ACTIVE) {
        this.stats.activeSessions++;
      } else if (state === SessionState.IDLE) {
        this.stats.idleSessions++;
      } else if (state === SessionState.ERROR) {
        this.stats.errorSessions++;
      }
    }
  }
}