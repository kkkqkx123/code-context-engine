import { injectable, inject } from 'inversify';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { TYPES } from '../../types';
import { HotReloadMetrics } from './ProjectHotReloadService';
import { HotReloadError } from './HotReloadError';

export interface HotReloadMonitoringConfig {
  enableMetricsCollection: boolean;
  metricsCollectionInterval: number; // milliseconds
  enableDetailedLogging: boolean;
  maxMetricsHistory: number;
  alertThresholds: {
    errorRate: number; // errors per minute
    processingTime: number; // milliseconds
    memoryUsage: number; // percentage
  };
}

export interface MetricsHistoryPoint {
  timestamp: Date;
  metrics: HotReloadMetrics;
}

export interface SystemMetrics {
  memoryUsage: {
    rss: number;
    heapTotal: number;
    heapUsed: number;
    external: number;
    arrayBuffers: number;
  };
  cpuUsage: number | null;
  uptime: number;
}

@injectable()
export class HotReloadMonitoringService {
  private config: HotReloadMonitoringConfig;
  private projectMetrics: Map<string, HotReloadMetrics> = new Map();
  private metricsHistory: Map<string, MetricsHistoryPoint[]> = new Map();
  private monitoringInterval: NodeJS.Timeout | null = null;
  private readonly defaultMetrics: HotReloadMetrics;
  private systemMetrics: SystemMetrics | null = null;
  private lastMetricsTime: Map<string, number> = new Map(); // 记录上次指标更新时间
  private systemMetricsInterval: NodeJS.Timeout | null = null;

  constructor(
    @inject(TYPES.LoggerService) private logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) private errorHandler: ErrorHandlerService
  ) {
    this.defaultMetrics = {
      filesProcessed: 0,
      changesDetected: 0,
      averageProcessingTime: 0,
      lastUpdated: new Date(),
      errorCount: 0,
      errorBreakdown: {},
      recoveryStats: {
        autoRecovered: 0,
        manualIntervention: 0,
        failedRecoveries: 0
      }
    };

    this.config = {
      enableMetricsCollection: true,
      metricsCollectionInterval: 10000, // 10 seconds
      enableDetailedLogging: false,
      maxMetricsHistory: 100, // Keep last 100 metrics points
      alertThresholds: {
        errorRate: 10, // 10 errors per minute
        processingTime: 5000, // 5 seconds
        memoryUsage: 80 // 80%
      }
    };

    this.startSystemMetricsCollection();
  }

  private startSystemMetricsCollection(): void {
    // 定期收集系统指标
    this.systemMetricsInterval = setInterval(() => {
      this.systemMetrics = {
        memoryUsage: process.memoryUsage(),
        cpuUsage: null, // Node.js doesn't provide direct CPU usage, would need additional library
        uptime: process.uptime()
      };
    }, 5000); // 每5秒更新一次系统指标
  }

  /**
    * 更新配置
    */
  updateConfig(config: Partial<HotReloadMonitoringConfig>): void {
    this.config = {
      ...this.config,
      ...config,
      alertThresholds: {
        ...this.config.alertThresholds,
        ...(config.alertThresholds || {})
      }
    };

    // 如果启用了指标收集但没有启动监控，则启动
    if (this.config.enableMetricsCollection && !this.monitoringInterval) {
      this.startMetricsCollection();
    } else if (!this.config.enableMetricsCollection && this.monitoringInterval) {
      this.stopMetricsCollection();
    }

    this.logger.info('Hot reload monitoring configuration updated', { config: this.config });
  }

  /**
    * 启动指标收集
    */
  private startMetricsCollection(): void {
    if (this.monitoringInterval) {
      clearInterval(this.monitoringInterval);
    }

    this.monitoringInterval = setInterval(() => {
      this.collectMetrics();
    }, this.config.metricsCollectionInterval);

    this.logger.info('Hot reload metrics collection started');
  }

  /**
   * 停止指标收集
   */
  private stopMetricsCollection(): void {
    if (this.monitoringInterval) {
      clearInterval(this.monitoringInterval);
      this.monitoringInterval = null;
      this.logger.info('Hot reload metrics collection stopped');
    }
  }

  /**
   * 收集指标
   */
  private collectMetrics(): void {
    if (!this.config.enableMetricsCollection) {
      return;
    }

    // 为每个项目收集指标
    for (const [projectPath, metrics] of this.projectMetrics.entries()) {
      this.addMetricsToHistory(projectPath, metrics);
    }

    if (this.config.enableDetailedLogging) {
      this.logger.debug('Hot reload metrics collected', {
        projectCount: this.projectMetrics.size,
        systemMetrics: this.systemMetrics
      });
    }
  }

  /**
   * 添加指标到历史记录
   */
  private addMetricsToHistory(projectPath: string, metrics: HotReloadMetrics): void {
    if (!this.metricsHistory.has(projectPath)) {
      this.metricsHistory.set(projectPath, []);
    }

    const history = this.metricsHistory.get(projectPath)!;
    history.push({
      timestamp: new Date(),
      metrics: { ...metrics } // 创建副本以避免引用问题
    });

    // 限制历史记录大小
    if (history.length > this.config.maxMetricsHistory) {
      history.shift(); // 移除最旧的记录
    }
  }

  /**
    * 更新项目指标
    */
  updateProjectMetrics(projectPath: string, metricsUpdate: Partial<HotReloadMetrics>): void {
    const currentMetrics = this.getProjectMetrics(projectPath);
    const updatedMetrics: HotReloadMetrics = {
      ...currentMetrics,
      ...metricsUpdate,
      errorBreakdown: {
        ...currentMetrics.errorBreakdown,
        ...(metricsUpdate.errorBreakdown || {})
      },
      recoveryStats: {
        ...currentMetrics.recoveryStats,
        ...(metricsUpdate.recoveryStats || {})
      }
    };

    // 计算平均处理时间（如果提供了新的处理时间）
    if (metricsUpdate.averageProcessingTime !== undefined) {
      updatedMetrics.averageProcessingTime = metricsUpdate.averageProcessingTime;
    } else if (metricsUpdate.lastUpdated) {
      // 如果提供了新的更新时间，可能需要重新计算平均时间
      const currentTime = Date.now();
      const lastTime = this.lastMetricsTime.get(projectPath) || currentTime;
      this.lastMetricsTime.set(projectPath, currentTime);
    }

    updatedMetrics.lastUpdated = new Date();

    this.projectMetrics.set(projectPath, updatedMetrics);

    // 检查是否需要触发警报
    this.checkAlerts(projectPath, updatedMetrics);
  }

  /**
   * 获取项目指标
   */
  getProjectMetrics(projectPath: string): HotReloadMetrics {
    const existingMetrics = this.projectMetrics.get(projectPath);
    if (existingMetrics) {
      return { ...existingMetrics }; // 返回副本
    }

    // 返回默认指标
    return { ...this.defaultMetrics };
  }

  /**
    * 获取所有项目指标
    */
  getAllProjectMetrics(): Map<string, HotReloadMetrics> {
    const result = new Map<string, HotReloadMetrics>();
    for (const [projectPath, metrics] of this.projectMetrics.entries()) {
      result.set(projectPath, { ...metrics });
    }
    return result;
  }

  /**
   * 获取项目指标历史
   */
  getProjectMetricsHistory(projectPath: string): MetricsHistoryPoint[] {
    const history = this.metricsHistory.get(projectPath);
    return history ? [...history] : [];
  }

  /**
   * 获取系统指标
   */
  getSystemMetrics(): SystemMetrics | null {
    return this.systemMetrics ? { ...this.systemMetrics } : null;
  }

  /**
   * 检查警报条件
   */
  private checkAlerts(projectPath: string, metrics: HotReloadMetrics): void {
    // 检查错误率（基于每分钟的错误数）
    // 这里我们检查自上次指标更新以来的错误率
    const now = Date.now();
    const timeSinceLastUpdate = (now - metrics.lastUpdated.getTime()) / 1000 / 60; // 转换为分钟

    // 计算错误率：每分钟的错误数
    // 如果时间差为0（或接近0），则使用当前错误总数作为错误率
    const errorRate = timeSinceLastUpdate > 0 ? metrics.errorCount / Math.max(timeSinceLastUpdate, 1) : metrics.errorCount;

    if (errorRate > this.config.alertThresholds.errorRate) {
      this.logger.warn(`High error rate detected for project ${projectPath}`, {
        errorRate,
        threshold: this.config.alertThresholds.errorRate,
        metrics
      });
    }

    // 检查处理时间
    if (metrics.averageProcessingTime > this.config.alertThresholds.processingTime) {
      this.logger.warn(`High average processing time detected for project ${projectPath}`, {
        averageProcessingTime: metrics.averageProcessingTime,
        threshold: this.config.alertThresholds.processingTime,
        metrics
      });
    }
  }

  /**
   * 记录错误并更新指标
   */
  recordError(projectPath: string, error: HotReloadError): void {
    const currentMetrics = this.getProjectMetrics(projectPath);
    const errorType = error.code || 'UNKNOWN_ERROR';

    currentMetrics.errorCount++;
    currentMetrics.errorBreakdown[errorType] = (currentMetrics.errorBreakdown[errorType] || 0) + 1;
    currentMetrics.lastUpdated = new Date();

    this.projectMetrics.set(projectPath, currentMetrics);

    // 检查是否需要触发警报
    this.checkAlerts(projectPath, currentMetrics);
  }

  /**
   * 记录文件处理完成
   */
  recordFileProcessed(projectPath: string, processingTimeMs: number): void {
    const currentMetrics = this.getProjectMetrics(projectPath);

    currentMetrics.filesProcessed++;
    // 更新平均处理时间
    const totalProcessingTime = currentMetrics.averageProcessingTime * (currentMetrics.filesProcessed - 1) + processingTimeMs;
    currentMetrics.averageProcessingTime = totalProcessingTime / currentMetrics.filesProcessed;
    currentMetrics.lastUpdated = new Date();

    this.projectMetrics.set(projectPath, currentMetrics);
  }

  /**
   * 记录变更检测
   */
  recordChangeDetected(projectPath: string): void {
    const currentMetrics = this.getProjectMetrics(projectPath);

    currentMetrics.changesDetected++;
    currentMetrics.lastUpdated = new Date();

    this.projectMetrics.set(projectPath, currentMetrics);
  }

  /**
   * 记录恢复统计
   */
  recordRecovery(projectPath: string, success: boolean): void {
    const currentMetrics = this.getProjectMetrics(projectPath);

    if (success) {
      currentMetrics.recoveryStats.autoRecovered++;
    } else {
      currentMetrics.recoveryStats.failedRecoveries++;
    }
    currentMetrics.lastUpdated = new Date();

    this.projectMetrics.set(projectPath, currentMetrics);
  }

  /**
   * 重置项目指标
   */
  resetProjectMetrics(projectPath: string): void {
    this.projectMetrics.set(projectPath, { ...this.defaultMetrics });
    this.metricsHistory.delete(projectPath);
    this.logger.info(`Hot reload metrics reset for project: ${projectPath}`);
  }

  /**
   * 获取指标摘要
   */
  getMetricsSummary(): {
    totalProjects: number;
    totalChanges: number;
    totalErrors: number;
    averageProcessingTime: number;
    systemMetrics: SystemMetrics | null;
  } {
    let totalChanges = 0;
    let totalErrors = 0;
    let totalProcessingTime = 0;
    let totalFilesProcessed = 0;

    for (const metrics of this.projectMetrics.values()) {
      totalChanges += metrics.changesDetected;
      totalErrors += metrics.errorCount;
      totalProcessingTime += metrics.averageProcessingTime;
      totalFilesProcessed += metrics.filesProcessed > 0 ? 1 : 0; // 计算有多少个项目有处理时间记录
    }

    const averageProcessingTime = totalFilesProcessed > 0 ? totalProcessingTime / totalFilesProcessed : 0;

    return {
      totalProjects: this.projectMetrics.size,
      totalChanges,
      totalErrors,
      averageProcessingTime,
      systemMetrics: this.systemMetrics
    };
  }

  /**
   * 销毁监控服务
   */
  destroy(): void {
    if (this.monitoringInterval) {
      clearInterval(this.monitoringInterval);
      this.monitoringInterval = null;
    }

    if (this.systemMetricsInterval) {
      clearInterval(this.systemMetricsInterval);
      this.systemMetricsInterval = null;
    }

    this.logger.info('HotReloadMonitoringService destroyed');
  }
}