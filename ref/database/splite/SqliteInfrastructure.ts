import { injectable, inject } from 'inversify';
import { TYPES } from '../../types';
import { LoggerService } from '../../utils/LoggerService';
import { DatabaseType } from '../../infrastructure/types';
import { IDatabaseInfrastructure } from '../../infrastructure/InfrastructureManager';
import { ICacheService } from '../../infrastructure/caching/types';
import { IPerformanceMonitor } from '../../infrastructure/monitoring/types';
import { IBatchOptimizer } from '../../infrastructure/batching/types';
import { IHealthChecker } from '../../infrastructure/monitoring/types';
import { BatchProcessingService } from '../../infrastructure/batching/BatchProcessingService';
import { SqliteDatabaseService } from './SqliteDatabaseService';
import { SqliteConnectionManager } from './SqliteConnectionManager';

@injectable()
export class SqliteInfrastructure implements IDatabaseInfrastructure {
  readonly databaseType = DatabaseType.SQLITE;

  private logger: LoggerService;
  private cacheService: ICacheService;
  private performanceMonitor: IPerformanceMonitor;
  private batchProcessor: BatchProcessingService;
  private healthChecker: IHealthChecker;
  private sqliteService: SqliteDatabaseService;
  private sqliteConnectionManager: SqliteConnectionManager;
  private initialized = false;

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.CacheService) cacheService: ICacheService,
    @inject(TYPES.PerformanceMonitor) performanceMonitor: IPerformanceMonitor,
    @inject(TYPES.BatchProcessingService) batchOptimizer: BatchProcessingService,
    @inject(TYPES.HealthChecker) healthChecker: IHealthChecker,
    @inject(TYPES.SqliteDatabaseService) sqliteService: SqliteDatabaseService,
    @inject(TYPES.SqliteConnectionManager) sqliteConnectionManager: SqliteConnectionManager
  ) {
    this.logger = logger;
    this.cacheService = cacheService;
    this.performanceMonitor = performanceMonitor;
    this.batchProcessor = batchOptimizer;
    this.healthChecker = healthChecker;
    this.sqliteService = sqliteService;
    this.sqliteConnectionManager = sqliteConnectionManager;

    this.logger.info('Sqlite infrastructure created');
  }

  getCacheService(): ICacheService {
    this.ensureInitialized();
    return this.cacheService;
  }

  getPerformanceMonitor(): IPerformanceMonitor {
    this.ensureInitialized();
    return this.performanceMonitor;
  }

  getBatchOptimizer(): BatchProcessingService {
    this.ensureInitialized();
    return this.batchProcessor;
  }

  getHealthChecker(): IHealthChecker {
    this.ensureInitialized();
    return this.healthChecker;
  }

  async initialize(): Promise<void> {
    if (this.initialized) {
      this.logger.warn('Sqlite infrastructure already initialized');
      return;
    }

    this.logger.info('Initializing Sqlite infrastructure');

    try {
      // 初始化SQLite数据库服务
      await this.sqliteService.initialize();

      // 初始化SQLite连接管理器
      await this.sqliteConnectionManager.initialize();

      // 启动性能监控
      this.performanceMonitor.startPeriodicMonitoring(30000);

      // 执行健康检查
      await this.healthChecker.checkHealth();

      this.initialized = true;
      this.logger.info('Sqlite infrastructure initialized successfully');
    } catch (error) {
      this.logger.error('Failed to initialize Sqlite infrastructure', {
        error: (error as Error).message
      });
      throw error;
    }
  }

  async shutdown(): Promise<void> {
    if (!this.initialized) {
      this.logger.warn('Sqlite infrastructure not initialized, nothing to shutdown');
      return;
    }

    this.logger.info('Shutting down Sqlite infrastructure');

    try {
      // 停止性能监控
      this.performanceMonitor.stopPeriodicMonitoring();

      // 清理缓存
      this.cacheService.clearAllCache();

      // 重置性能指标
      this.performanceMonitor.resetMetrics();

      // 关闭SQLite数据库连接
      await this.sqliteService.close();

      this.initialized = false;
      this.logger.info('Sqlite infrastructure shutdown completed');
    } catch (error) {
      this.logger.error('Error during Sqlite infrastructure shutdown', {
        error: (error as Error).message
      });
      throw error;
    }
  }

  private ensureInitialized(): void {
    if (!this.initialized) {
      throw new Error('Sqlite infrastructure is not initialized. Call initialize() first.');
    }
  }

  // SQLite特定的辅助方法
  async executeSqlQuery(query: string, params?: any[]): Promise<any> {
    this.ensureInitialized();

    const startTime = Date.now();
    let success = false;

    try {
      const result = await this.sqliteService.executeQuery(query, params);
      success = true;
      return result;
    } finally {
      const duration = Date.now() - startTime;
      await this.recordSqlOperation('query', query, duration, success);
    }
  }

  async backupDatabase(backupPath: string): Promise<void> {
    this.ensureInitialized();
    await this.sqliteService.backup(backupPath);
  }

  async getDatabaseStats(): Promise<DatabaseStats> {
    this.ensureInitialized();
    const stats = this.sqliteService.getStats();
    return {
      projects: stats.projects,
      fileStates: stats.fileStates,
      projectStatus: stats.projectStatus,
      changeHistory: stats.changeHistory,
      databaseSize: stats.databaseSize,
      tableSizes: stats.tableSizes || {}
    };
  }

  async recordSqlOperation(
    operation: string,
    query: string,
    duration: number,
    success: boolean
  ): Promise<void> {
    await (this.performanceMonitor as any).recordSqlOperation?.(
      operation,
      query,
      duration,
      success
    );

    this.logger.debug('Recorded SQL operation', {
      operation,
      query: query.substring(0, 100), // 只记录前100个字符
      duration,
      success
    });
  }

  isInitialized(): boolean {
    return this.initialized;
  }
}

export interface DatabaseStats {
  projects: number;
  fileStates: number;
  projectStatus: number;
  changeHistory: number;
  databaseSize: number;
  tableSizes: Record<string, number>;
}