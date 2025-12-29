import { injectable, inject } from 'inversify';
import { EventEmitter } from 'events';
import { TYPES } from '../../../types';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { NebulaConfigService } from '../../../config/service/NebulaConfigService';
import { PerformanceMonitor } from '../../../infrastructure/monitoring/PerformanceMonitor';
import { LRUCache } from '../../../utils/cache/LRUCache';
import { IConnectionPool, ConnectionPoolStats } from '../connection/ConnectionPool';
import { Session, SessionState } from './Session';
import { SessionPool, SessionPoolStats } from './SessionPool';

// 会话管理器配置
export interface SessionManagerConfig {
  maxSessions: number;
  sessionTimeout: number;
  idleTimeout: number;
  cleanupInterval: number;
  spaceCacheSize: number;
}

// 会话管理器统计信息
export interface SessionManagerStats {
  totalSessions: number;
  activeSessions: number;
  idleSessions: number;
  spaceCount: number;
  totalAcquires: number;
  totalReleases: number;
  cacheHits: number;
  cacheMisses: number;
  averageSessionAge: number;
  connectionPoolStats: ConnectionPoolStats;
  sessionPoolStats: SessionPoolStats[];
}

// 默认会话管理器配置
const DEFAULT_SESSION_MANAGER_CONFIG: SessionManagerConfig = {
  maxSessions: 50,
  sessionTimeout: 1800000, // 30分钟
  idleTimeout: 300000,     // 5分钟
  cleanupInterval: 60000,  // 1分钟
  spaceCacheSize: 100
};

// 会话管理器接口
export interface ISessionManager {
  // 会话管理
  getSession(spaceName?: string): Promise<Session>;
  releaseSession(session: Session): void;
  invalidateSession(sessionId: string): void;

  // 会话池管理
  startSessionCleanup(): void;
  stopSessionCleanup(): void;

  // 空间管理
  switchSpace(session: Session, spaceName: string): Promise<void>;

  // 状态监控
  getSessionStats(): SessionManagerStats;
}

/**
 * 会话管理器
 * 管理所有会话的生命周期，与连接管理解耦
 */
@injectable()
export class SessionManager extends EventEmitter implements ISessionManager {
  private logger: LoggerService;
  private errorHandler: ErrorHandlerService;
  private configService: NebulaConfigService;
  private performanceMonitor: PerformanceMonitor;
  private config: SessionManagerConfig;

  private connectionPool: IConnectionPool;
  private sessionPools: Map<string, SessionPool> = new Map();
  private spaceSessionCache: LRUCache<string, Session>;
  private cleanupInterval: NodeJS.Timeout | null = null;
  private isInitialized: boolean = false;

  // 统计信息
  private stats: SessionManagerStats = {
    totalSessions: 0,
    activeSessions: 0,
    idleSessions: 0,
    spaceCount: 0,
    totalAcquires: 0,
    totalReleases: 0,
    cacheHits: 0,
    cacheMisses: 0,
    averageSessionAge: 0,
    connectionPoolStats: {} as ConnectionPoolStats,
    sessionPoolStats: []
  };

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.NebulaConfigService) configService: NebulaConfigService,
    @inject(TYPES.PerformanceMonitor) performanceMonitor: PerformanceMonitor,
    @inject(TYPES.IConnectionPool) connectionPool: IConnectionPool
  ) {
    super();
    this.logger = logger;
    this.errorHandler = errorHandler;
    this.configService = configService;
    this.performanceMonitor = performanceMonitor;
    this.connectionPool = connectionPool;
    this.config = { ...DEFAULT_SESSION_MANAGER_CONFIG };

    // 初始化空间-会话缓存
    this.spaceSessionCache = new LRUCache<string, Session>(this.config.spaceCacheSize, {
      enableStats: true
    });
  }

  /**
   * 初始化会话管理器
   */
  async initialize(): Promise<void> {
    if (this.isInitialized) {
      this.logger.warn('Session manager already initialized');
      return;
    }

    try {
      this.logger.info('Initializing session manager', {
        maxSessions: this.config.maxSessions,
        sessionTimeout: this.config.sessionTimeout,
        spaceCacheSize: this.config.spaceCacheSize
      });

      // 启动会话清理任务
      this.startSessionCleanup();

      this.isInitialized = true;
      this.emit('initialized');
      this.logger.info('Session manager initialized successfully');
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Failed to initialize session manager'),
        { component: 'SessionManager', operation: 'initialize' }
      );
      throw error;
    }
  }

  /**
   * 获取会话
   */
  async getSession(spaceName?: string): Promise<Session> {
    if (!this.isInitialized) {
      throw new Error('Session manager not initialized');
    }

    const startTime = Date.now();
    this.stats.totalAcquires++;

    try {
      // 开始操作监控
      const operationId = this.performanceMonitor.startOperation('session_acquire', {
        metadata: {
          spaceName,
          startTime
        }
      });

      let session: Session;

      // 如果指定了空间名称，先尝试从缓存获取
      if (spaceName) {
        const cachedSession = this.spaceSessionCache.get(spaceName);
        if (cachedSession && cachedSession.getState() === SessionState.IDLE) {
          this.stats.cacheHits++;
          session = cachedSession;
          this.logger.debug('Session cache hit', { spaceName });
        } else {
          this.stats.cacheMisses++;
          session = await this.createSessionForSpace(spaceName);
          this.spaceSessionCache.set(spaceName, session);
          this.logger.debug('Session cache miss, created new session', { spaceName });
        }
      } else {
        // 没有指定空间，获取通用会话
        session = await this.createGenericSession();
      }

      const acquireTime = Date.now() - startTime;
      this.performanceMonitor.endOperation(operationId, {
        duration: acquireTime,
        metadata: {
          spaceName,
          cacheHit: !!spaceName && this.stats.cacheHits > this.stats.cacheMisses
        }
      });

      this.emit('sessionAcquired', {
        sessionId: session.getId(),
        spaceName: session.getSpaceName()
      });

      return session;
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Failed to acquire session'),
        { component: 'SessionManager', operation: 'getSession', spaceName }
      );
      throw error;
    }
  }

  /**
   * 释放会话
   */
  releaseSession(session: Session): void {
    if (!this.isInitialized) {
      return;
    }

    try {
      const spaceName = session.getSpaceName();

      // 如果会话属于某个空间，释放回对应的会话池
      if (spaceName) {
        const sessionPool = this.sessionPools.get(spaceName);
        if (sessionPool) {
          sessionPool.releaseSession(session);
        }
      } else {
        // 通用会话，释放回默认会话池
        const defaultPool = this.sessionPools.get('default');
        if (defaultPool) {
          defaultPool.releaseSession(session);
        }
      }

      this.stats.totalReleases++;

      this.emit('sessionReleased', {
        sessionId: session.getId(),
        spaceName
      });
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Failed to release session'),
        { component: 'SessionManager', operation: 'releaseSession', sessionId: session.getId() }
      );
    }
  }

  /**
   * 使会话失效
   */
  invalidateSession(sessionId: string): void {
    if (!this.isInitialized) {
      return;
    }

    try {
      // 从缓存中移除
      for (const spaceName of this.spaceSessionCache.keys()) {
        const session = this.spaceSessionCache.get(spaceName);
        if (!session) continue;
        if (session.getId() === sessionId) {
          this.spaceSessionCache.delete(spaceName);
          break;
        }
      }

      // 通知所有会话池使会话失效
      for (const sessionPool of this.sessionPools.values()) {
        sessionPool.invalidateSession(sessionId);
      }

      this.emit('sessionInvalidated', { sessionId });
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Failed to invalidate session'),
        { component: 'SessionManager', operation: 'invalidateSession', sessionId }
      );
    }
  }

  /**
   * 切换会话空间
   */
  async switchSpace(session: Session, spaceName: string): Promise<void> {
    if (!this.isInitialized) {
      throw new Error('Session manager not initialized');
    }

    try {
      const oldSpaceName = session.getSpaceName();

      // 从旧空间的缓存中移除
      if (oldSpaceName) {
        this.spaceSessionCache.delete(oldSpaceName);
      }

      // 切换空间
      await session.switchSpace(spaceName);

      // 添加到新空间的缓存
      this.spaceSessionCache.set(spaceName, session);

      this.emit('sessionSpaceSwitched', {
        sessionId: session.getId(),
        oldSpaceName,
        newSpaceName: spaceName
      });
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Failed to switch session space'),
        {
          component: 'SessionManager',
          operation: 'switchSpace',
          sessionId: session.getId(),
          spaceName
        }
      );
      throw error;
    }
  }

  /**
   * 启动会话清理任务
   */
  startSessionCleanup(): void {
    if (this.cleanupInterval) {
      return;
    }

    this.cleanupInterval = setInterval(async () => {
      try {
        await this.performSessionCleanup();
      } catch (error) {
        this.errorHandler.handleError(
          error instanceof Error ? error : new Error('Session cleanup task failed'),
          { component: 'SessionManager', operation: 'sessionCleanup' }
        );
      }
    }, this.config.cleanupInterval);

    // 确保定时器不会阻止进程退出
    if (this.cleanupInterval.unref) {
      this.cleanupInterval.unref();
    }

    this.logger.info('Session cleanup task started');
  }

  /**
   * 停止会话清理任务
   */
  stopSessionCleanup(): void {
    if (this.cleanupInterval) {
      clearInterval(this.cleanupInterval);
      this.cleanupInterval = null;
      this.logger.info('Session cleanup task stopped');
    }
  }

  /**
   * 获取会话统计信息
   */
  getSessionStats(): SessionManagerStats {
    this.updateStats();

    // 获取缓存统计
    const cacheStats = this.spaceSessionCache.getStats();
    if (cacheStats) {
      this.stats.cacheHits = cacheStats.hits;
      this.stats.cacheMisses = cacheStats.misses;
    }

    return { ...this.stats };
  }

  /**
   * 关闭会话管理器
   */
  async close(): Promise<void> {
    if (!this.isInitialized) {
      return;
    }

    this.logger.info('Closing session manager');

    // 停止清理任务
    this.stopSessionCleanup();

    // 关闭所有会话池
    const closePromises = Array.from(this.sessionPools.values()).map(pool =>
      pool.close().catch(error => {
        this.logger.error('Error closing session pool', { error });
      })
    );

    await Promise.allSettled(closePromises);

    this.sessionPools.clear();
    this.spaceSessionCache.clear();
    this.isInitialized = false;

    this.emit('closed');
    this.logger.info('Session manager closed');
  }

  /**
   * 为指定空间创建会话
   */
  private async createSessionForSpace(spaceName: string): Promise<Session> {
    let sessionPool = this.sessionPools.get(spaceName);

    if (!sessionPool) {
      // 获取连接
      const connection = await this.connectionPool.getConnection();

      // 创建会话池
      sessionPool = new SessionPool(connection, {
        maxSessionsPerConnection: Math.floor(this.config.maxSessions / 10), // 每个连接最多10个会话
        sessionTimeout: this.config.sessionTimeout,
        idleTimeout: this.config.idleTimeout,
        cleanupInterval: this.config.cleanupInterval
      });

      // 设置会话池事件监听
      this.setupSessionPoolEvents(sessionPool, spaceName);

      this.sessionPools.set(spaceName, sessionPool);
      this.stats.spaceCount++;

      this.emit('sessionPoolCreated', { spaceName });
    }

    return await sessionPool.getSession(spaceName);
  }

  /**
   * 创建通用会话
   */
  private async createGenericSession(): Promise<Session> {
    let sessionPool = this.sessionPools.get('default');

    if (!sessionPool) {
      // 获取连接
      const connection = await this.connectionPool.getConnection();

      // 创建默认会话池
      sessionPool = new SessionPool(connection, {
        maxSessionsPerConnection: Math.floor(this.config.maxSessions / 10),
        sessionTimeout: this.config.sessionTimeout,
        idleTimeout: this.config.idleTimeout,
        cleanupInterval: this.config.cleanupInterval
      });

      // 设置会话池事件监听
      this.setupSessionPoolEvents(sessionPool, 'default');

      this.sessionPools.set('default', sessionPool);

      this.emit('sessionPoolCreated', { spaceName: 'default' });
    }

    return await sessionPool.getSession();
  }

  /**
   * 执行会话清理
   */
  private async performSessionCleanup(): Promise<void> {
    // 清理过期的缓存条目
    this.spaceSessionCache.cleanup();

    // 更新统计信息
    this.updateStats();

    this.emit('cleanupCompleted');
  }

  /**
   * 设置会话池事件监听
   */
  private setupSessionPoolEvents(sessionPool: SessionPool, spaceName: string): void {
    sessionPool.on('sessionCreated', (event) => {
      this.stats.totalSessions++;
      this.emit('sessionCreated', { ...event, spaceName });
    });

    sessionPool.on('sessionDestroyed', (event) => {
      this.stats.totalSessions--;
      this.emit('sessionDestroyed', { ...event, spaceName });
    });

    sessionPool.on('sessionError', (event) => {
      this.emit('sessionError', { ...event, spaceName });
    });
  }

  /**
   * 更新统计信息
   */
  private updateStats(): void {
    // 重置计数
    this.stats.activeSessions = 0;
    this.stats.idleSessions = 0;
    this.stats.sessionPoolStats = [];

    // 聚合所有会话池的统计信息
    for (const [spaceName, sessionPool] of this.sessionPools.entries()) {
      const poolStats = sessionPool.getPoolStats();
      this.stats.activeSessions += poolStats.activeSessions;
      this.stats.idleSessions += poolStats.idleSessions;

      this.stats.sessionPoolStats.push({
        ...poolStats,
        spaceName
      } as any);
    }

    // 获取连接池统计
    this.stats.connectionPoolStats = this.connectionPool.getPoolStats();

    // 计算平均会话年龄
    if (this.stats.totalSessions > 0) {
      let totalAge = 0;
      let sessionCount = 0;

      for (const sessionPool of this.sessionPools.values()) {
        const poolStats = sessionPool.getPoolStats();
        totalAge += poolStats.averageSessionAge * (poolStats.totalSessions || 0);
        sessionCount += poolStats.totalSessions || 0;
      }

      if (sessionCount > 0) {
        this.stats.averageSessionAge = totalAge / sessionCount;
      }
    }

    // 记录会话管理器性能指标
    const operationId = this.performanceMonitor.startOperation('session_manager_stats', {
      metadata: {
        totalSessions: this.stats.totalSessions,
        activeSessions: this.stats.activeSessions,
        idleSessions: this.stats.idleSessions,
        spaceCount: this.stats.spaceCount,
        totalAcquires: this.stats.totalAcquires,
        totalReleases: this.stats.totalReleases,
        cacheHits: this.stats.cacheHits,
        cacheMisses: this.stats.cacheMisses,
        averageSessionAge: this.stats.averageSessionAge,
        spaceNames: this.getSpaceNames()
      }
    });
    
    this.performanceMonitor.endOperation(operationId, {
      duration: 0,
      metadata: {
        totalSessions: this.stats.totalSessions,
        activeSessions: this.stats.activeSessions,
        idleSessions: this.stats.idleSessions,
        spaceCount: this.stats.spaceCount,
        totalAcquires: this.stats.totalAcquires,
        totalReleases: this.stats.totalReleases,
        cacheHits: this.stats.cacheHits,
        cacheMisses: this.stats.cacheMisses,
        averageSessionAge: this.stats.averageSessionAge,
        spaceNames: this.getSpaceNames()
      }
    });
  }

  /**
   * 获取所有空间名称
   */
  private getSpaceNames(): string[] {
    return Array.from(this.sessionPools.keys());
  }
}