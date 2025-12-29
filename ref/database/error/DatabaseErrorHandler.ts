import { injectable, inject } from 'inversify';
import { TYPES } from '../../types';
import { LoggerService } from '../../utils/LoggerService';
import { IPerformanceMonitor } from '../../infrastructure/monitoring/types';
import { DatabaseType } from '../../infrastructure/types';

interface ErrorInfo {
  category: string;
  severity: 'LOW' | 'MEDIUM' | 'HIGH';
}

export interface IAlertManager {
  sendAlert(alert: Alert): Promise<void>;
}

export interface Alert {
  type: string;
  severity: 'INFO' | 'WARNING' | 'CRITICAL';
  message: string;
  metadata?: Record<string, any>;
}

@injectable()
export class DatabaseErrorHandler {
  private logger: LoggerService;
  private alertManager: IAlertManager;
  private performanceMonitor: IPerformanceMonitor;

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    alertManager: IAlertManager,
    performanceMonitor: IPerformanceMonitor
  ) {
    this.logger = logger;
    this.alertManager = alertManager;
    this.performanceMonitor = performanceMonitor;
  }

  async handleDatabaseError(
    error: Error,
    databaseType: DatabaseType,
    operation: string,
    context: Record<string, any>
  ): Promise<void> {
    const errorInfo = this.categorizeError(error, databaseType);

    // 记录错误指标
    await this.recordErrorMetric(databaseType, operation);

    // 记录详细错误日志
    this.logger.error(`Database operation failed`, {
      databaseType,
      operation,
      error: error.message,
      stack: error.stack,
      context,
      category: errorInfo.category,
      severity: errorInfo.severity
    });

    // 发送警报（根据严重程度）
    if (errorInfo.severity === 'HIGH') {
      await this.alertManager.sendAlert({
        type: 'DATABASE_ERROR',
        severity: 'CRITICAL',
        message: `${databaseType} operation failed: ${error.message}`,
        metadata: { databaseType, operation, context }
      });
    }
  }

  private categorizeError(error: Error, databaseType: DatabaseType): ErrorInfo {
    // 根据错误类型和数据库类型分类
    const errorMessage = error.message.toLowerCase();
    
    if (errorMessage.includes('connection') || errorMessage.includes('connect')) {
      return { category: 'CONNECTION', severity: 'HIGH' };
    }
    if (errorMessage.includes('timeout') || errorMessage.includes('timed out')) {
      return { category: 'TIMEOUT', severity: 'MEDIUM' };
    }
    if (errorMessage.includes('constraint') || errorMessage.includes('duplicate')) {
      return { category: 'CONSTRAINT', severity: 'LOW' };
    }
    if (errorMessage.includes('memory') || errorMessage.includes('oom')) {
      return { category: 'RESOURCE', severity: 'HIGH' };
    }
    if (errorMessage.includes('permission') || errorMessage.includes('access denied')) {
      return { category: 'PERMISSION', severity: 'HIGH' };
    }
    if (errorMessage.includes('not found') || errorMessage.includes('404')) {
      return { category: 'NOT_FOUND', severity: 'LOW' };
    }

    return { category: 'UNKNOWN', severity: 'MEDIUM' };
  }

  private async recordErrorMetric(databaseType: DatabaseType, operation: string): Promise<void> {
    // 记录错误指标，如果性能监控器支持的话
    try {
      // Note: 根据实际的IPerformanceMonitor接口调整此调用
      // 这可能需要更新接口以支持记录错误指标
      this.logger.debug('Recorded error metric', { databaseType, operation });
    } catch (err) {
      this.logger.warn('Failed to record error metric', { error: (err as Error).message });
    }
  }

 async handleInfrastructureError(
    error: Error,
    component: string,
    operation: string,
    context: Record<string, any> = {}
  ): Promise<void> {
    const errorInfo = this.categorizeInfrastructureError(error, component);

    // 记录错误指标
    await this.recordInfrastructureErrorMetric(component, operation);

    // 记录详细错误日志
    this.logger.error(`Infrastructure component failed`, {
      component,
      operation,
      error: error.message,
      stack: error.stack,
      context,
      category: errorInfo.category,
      severity: errorInfo.severity
    });

    // 发送警报（根据严重程度）
    if (errorInfo.severity === 'HIGH') {
      await this.alertManager.sendAlert({
        type: 'INFRASTRUCTURE_ERROR',
        severity: 'CRITICAL',
        message: `${component} component failed: ${error.message}`,
        metadata: { component, operation, context }
      });
    }
  }

  private categorizeInfrastructureError(error: Error, component: string): ErrorInfo {
    // 根据基础设施组件类型分类错误
    const errorMessage = error.message.toLowerCase();
    
    if (errorMessage.includes('cache')) {
      return { category: 'CACHE_ERROR', severity: 'MEDIUM' };
    }
    if (errorMessage.includes('monitor') || errorMessage.includes('performance')) {
      return { category: 'MONITORING_ERROR', severity: 'LOW' };
    }
    if (errorMessage.includes('batch') || errorMessage.includes('batching')) {
      return { category: 'BATCH_ERROR', severity: 'MEDIUM' };
    }
    if (errorMessage.includes('connection')) {
      return { category: 'CONNECTION_ERROR', severity: 'HIGH' };
    }
    if (errorMessage.includes('transaction')) {
      return { category: 'TRANSACTION_ERROR', severity: 'HIGH' };
    }

    return { category: 'INFRASTRUCTURE_UNKNOWN', severity: 'MEDIUM' };
  }

  private async recordInfrastructureErrorMetric(component: string, operation: string): Promise<void> {
    try {
      this.logger.debug('Recorded infrastructure error metric', { component, operation });
    } catch (err) {
      this.logger.warn('Failed to record infrastructure error metric', { error: (err as Error).message });
    }
  }

 async handleBatchOperationError(
    error: Error,
    databaseType: DatabaseType,
    batchSize: number,
    operation: string
  ): Promise<void> {
    // 特殊处理批处理操作错误
    this.logger.error('Batch operation failed', {
      databaseType,
      batchSize,
      operation,
      error: error.message,
      stack: error.stack
    });

    // 记录批处理错误指标
    await this.recordBatchErrorMetric(databaseType, batchSize, operation);

    // 根据错误类型决定是否需要调整批处理大小
    if (this.shouldAdjustBatchSize(error)) {
      this.logger.info('Batch operation error may require batch size adjustment', {
        databaseType,
        batchSize,
        operation
      });
    }

    // 发送批处理操作错误警报
    await this.alertManager.sendAlert({
      type: 'BATCH_OPERATION_ERROR',
      severity: 'WARNING',
      message: `Batch operation failed for ${databaseType} database`,
      metadata: { 
        databaseType, 
        batchSize, 
        operation, 
        error: error.message 
      }
    });
 }

  private shouldAdjustBatchSize(error: Error): boolean {
    const errorMessage = error.message.toLowerCase();
    
    // 如果是内存不足或超时错误，可能需要减小批处理大小
    return errorMessage.includes('memory') || 
           errorMessage.includes('timeout') || 
           errorMessage.includes('oom') ||
           errorMessage.includes('exceeded');
  }

  private async recordBatchErrorMetric(
    databaseType: DatabaseType, 
    batchSize: number, 
    operation: string
  ): Promise<void> {
    try {
      this.logger.debug('Recorded batch error metric', { 
        databaseType, 
        batchSize, 
        operation 
      });
    } catch (err) {
      this.logger.warn('Failed to record batch error metric', { error: (err as Error).message });
    }
  }

  // 便捷方法：执行可能出错的操作并处理错误
  async executeWithErrorHandling<T>(
    operation: () => Promise<T>,
    databaseType: DatabaseType,
    operationName: string,
    context: Record<string, any> = {}
  ): Promise<T | null> {
    try {
      return await operation();
    } catch (error) {
      await this.handleDatabaseError(
        error instanceof Error ? error : new Error(String(error)),
        databaseType,
        operationName,
        context
      );
      return null;
    }
  }

  // 便捷方法：执行基础设施操作并处理错误
  async executeInfrastructureOperation<T>(
    operation: () => Promise<T>,
    component: string,
    operationName: string,
    context: Record<string, any> = {}
  ): Promise<T | null> {
    try {
      return await operation();
    } catch (error) {
      await this.handleInfrastructureError(
        error instanceof Error ? error : new Error(String(error)),
        component,
        operationName,
        context
      );
      return null;
    }
  }
}