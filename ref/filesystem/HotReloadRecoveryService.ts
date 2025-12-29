// 定义错误上下文接口
interface ErrorContext {
  component: string;
 operation: string;
 metadata?: Record<string, any>;
}

import { injectable, inject } from 'inversify';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { TYPES } from '../../types';
import { HotReloadError, HotReloadErrorCode } from './HotReloadError';

interface RecoveryStrategy {
  maxRetries: number;
  retryDelay: number;
 shouldRetry: (error: HotReloadError) => boolean;
 recoveryAction: (error: HotReloadError, context: ErrorContext) => Promise<void>;
}

@injectable()
export class HotReloadRecoveryService {
  private recoveryStrategies: Map<HotReloadErrorCode, RecoveryStrategy> = new Map();
  
  constructor(
    @inject(TYPES.ErrorHandlerService) private errorHandler: ErrorHandlerService,
    @inject(TYPES.LoggerService) private logger: LoggerService
  ) {
    this.setupRecoveryStrategies();
  }
  
  private setupRecoveryStrategies(): void {
    // 文件监视失败恢复策略
    this.recoveryStrategies.set(HotReloadErrorCode.FILE_WATCH_FAILED, {
      maxRetries: 3,
      retryDelay: 1000,
      shouldRetry: (error) => true,
      recoveryAction: async (error, context) => {
        this.logger.warn('Attempting to restart file watcher after failure', context);
        // 这里将通过IndexService重新启动文件监视
        // 具体实现将在IndexService中调用
      }
    });
    
    // 变更检测失败恢复策略
    this.recoveryStrategies.set(HotReloadErrorCode.CHANGE_DETECTION_FAILED, {
      maxRetries: 2,
      retryDelay: 2000,
      shouldRetry: (error) => true,
      recoveryAction: async (error, context) => {
        this.logger.warn('Attempting to restart change detection after failure', context);
        // 这里将通过ChangeDetectionService重新启动
      }
    });
    
    // 索引更新失败恢复策略
    this.recoveryStrategies.set(HotReloadErrorCode.INDEX_UPDATE_FAILED, {
      maxRetries: 1,
      retryDelay: 1000,
      shouldRetry: (error) => true,
      recoveryAction: async (error, context) => {
        this.logger.warn('Attempting to retry index update after failure', context);
        // 这里将通过IndexService重新尝试索引更新
      }
    });
    
    // 权限错误恢复策略
    this.recoveryStrategies.set(HotReloadErrorCode.PERMISSION_DENIED, {
      maxRetries: 1,
      retryDelay: 0,
      shouldRetry: (error) => false, // 权限错误不重试
      recoveryAction: async (error, context) => {
        // 记录错误并通知用户
        this.logger.warn('Permission denied for file monitoring - please check file permissions', context);
      }
    });
    
    // 文件过大错误恢复策略
    this.recoveryStrategies.set(HotReloadErrorCode.FILE_TOO_LARGE, {
      maxRetries: 0,
      retryDelay: 0,
      shouldRetry: (error) => false, // 文件过大不重试
      recoveryAction: async (error, context) => {
        this.logger.info('File too large for monitoring - skipping', context);
      }
    });
    
    // 项目未找到错误恢复策略
    this.recoveryStrategies.set(HotReloadErrorCode.PROJECT_NOT_FOUND, {
      maxRetries: 1,
      retryDelay: 1000,
      shouldRetry: (error) => true,
      recoveryAction: async (error, context) => {
        this.logger.warn('Project not found - may need to re-index', context);
      }
    });
 }
  
  async handleError(error: HotReloadError, context: ErrorContext): Promise<void> {
    const strategy = this.recoveryStrategies.get(error.code);
    if (strategy) {
      await strategy.recoveryAction(error, context);
    }
  }
  
  getRecoveryStrategy(errorCode: HotReloadErrorCode): RecoveryStrategy | undefined {
    return this.recoveryStrategies.get(errorCode);
  }
}