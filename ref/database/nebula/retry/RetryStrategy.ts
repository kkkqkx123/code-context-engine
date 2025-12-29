import { injectable, inject } from 'inversify';
import { TYPES } from '../../../types';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { NebulaConfigService } from '../../../config/service/NebulaConfigService';

// 重试策略配置
export interface RetryStrategyConfig {
  maxAttempts: number;
  baseDelay: number;
  maxDelay: number;
  backoffFactor: number;
  jitter: boolean;
  retryableErrors: string[];
}

// 重试上下文
export interface RetryContext {
  operation: string;
  attempt: number;
  error?: Error;
  metadata?: Record<string, any>;
}

// 重试策略接口
export interface IRetryStrategy {
  shouldRetry(context: RetryContext): boolean;
  getDelay(context: RetryContext): number;
  executeWithRetry<T>(operation: () => Promise<T>, context?: Partial<RetryContext>): Promise<T>;
}

// 默认重试策略配置
const DEFAULT_RETRY_STRATEGY_CONFIG: RetryStrategyConfig = {
  maxAttempts: 3,
  baseDelay: 1000, // 1秒
  maxDelay: 30000, // 30秒
  backoffFactor: 2,
  jitter: true,
  retryableErrors: [
    'ECONNREFUSED',
    'ECONNRESET',
    'ETIMEDOUT',
    'ENOTFOUND',
    'Network Error',
    'Connection Lost',
    'Session Invalid',
    'Storage Error',
    'Meta Error'
  ]
};

/**
 * 指数退避重试策略
 * 为Nebula Graph操作提供智能重试机制
 */
@injectable()
export class ExponentialBackoffRetryStrategy implements IRetryStrategy {
  private logger: LoggerService;
  private errorHandler: ErrorHandlerService;
  private configService: NebulaConfigService;
  private config: RetryStrategyConfig;

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.NebulaConfigService) configService: NebulaConfigService,
    config: Partial<RetryStrategyConfig> = {}
  ) {
    this.logger = logger;
    this.errorHandler = errorHandler;
    this.configService = configService;
    this.config = { ...DEFAULT_RETRY_STRATEGY_CONFIG, ...config };

    this.logger.info('ExponentialBackoffRetryStrategy initialized', {
      maxAttempts: this.config.maxAttempts,
      baseDelay: this.config.baseDelay,
      maxDelay: this.config.maxDelay,
      backoffFactor: this.config.backoffFactor,
      jitter: this.config.jitter
    });
  }

  /**
   * 判断是否应该重试
   */
  shouldRetry(context: RetryContext): boolean {
    // 检查是否达到最大重试次数
    if (context.attempt >= this.config.maxAttempts) {
      return false;
    }

    // 如果没有错误，不需要重试
    if (!context.error) {
      return false;
    }

    // 检查错误是否可重试
    const error = context.error;
    const errorMessage = error.message.toLowerCase();
    const errorName = error.name.toLowerCase();

    // 检查错误消息是否匹配可重试错误列表
    const isRetryableByMessage = this.config.retryableErrors.some(retryableError =>
      errorMessage.includes(retryableError.toLowerCase()) ||
      errorName.includes(retryableError.toLowerCase())
    );

    // 检查错误代码
    const errorCode = (error as any).code;
    let isRetryableByCode = false;
    if (errorCode && typeof errorCode === 'string') {
      isRetryableByCode = this.config.retryableErrors.some(retryableError =>
        errorCode.toLowerCase().includes(retryableError.toLowerCase())
      );
    }

    // 只要消息或代码任一匹配就认为可重试
    return isRetryableByMessage || isRetryableByCode;
  }

  /**
   * 获取重试延迟时间
   */
  getDelay(context: RetryContext): number {
    // 计算基础延迟（指数退避）
    // attempt 从 1 开始，所以使用 context.attempt - 1
    let delay = this.config.baseDelay * Math.pow(this.config.backoffFactor, context.attempt - 1);

    // 限制最大延迟
    delay = Math.min(delay, this.config.maxDelay);

    // 添加随机抖动以避免雷群效应
    if (this.config.jitter) {
      // 增加抖动范围到 25% 以更好地避免雷群效应
      const jitter = Math.random() * 0.25; // 0-25%的抖动
      delay = delay * (1 + jitter); // 应用抖动
    }

    return Math.floor(delay);
  }

  /**
   * 执行带重试的操作
   */
  async executeWithRetry<T>(operation: () => Promise<T>, context?: Partial<RetryContext>): Promise<T> {
    const operationContext: RetryContext = {
      operation: context?.operation || 'unknown',
      attempt: 1,
      error: context?.error,
      metadata: context?.metadata || {}
    };

    let lastError: Error | null = null;

    while (true) {
      try {
        this.logger.debug('Executing operation with retry strategy', {
          operation: operationContext.operation,
          attempt: operationContext.attempt,
          maxAttempts: this.config.maxAttempts
        });

        const result = await operation();
        
        // 记录成功
        this.logger.debug('Operation succeeded', {
          operation: operationContext.operation,
          attempts: operationContext.attempt
        });

        return result;
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));
        operationContext.error = lastError;

        this.logger.warn('Operation failed, checking if should retry', {
          operation: operationContext.operation,
          attempt: operationContext.attempt,
          maxAttempts: this.config.maxAttempts,
          error: lastError.message,
          shouldRetry: this.shouldRetry(operationContext)
        });

        // 如果不应该重试，抛出错误
        if (!this.shouldRetry(operationContext)) {
          this.errorHandler.handleError(
            lastError,
            { 
              component: 'ExponentialBackoffRetryStrategy', 
              operation: operationContext.operation, 
              attempt: operationContext.attempt,
              finalAttempt: true
            }
          );
          throw lastError;
        }

        // 增加重试次数
        operationContext.attempt++;

        // 如果超过最大重试次数，抛出错误
        if (operationContext.attempt > this.config.maxAttempts) {
          this.errorHandler.handleError(
            lastError,
            { 
              component: 'ExponentialBackoffRetryStrategy', 
              operation: operationContext.operation, 
              attempts: this.config.maxAttempts,
              finalAttempt: true
            }
          );
          throw lastError;
        }

        // 计算延迟时间
        const delay = this.getDelay(operationContext);

        this.logger.debug('Retrying operation after delay', {
          operation: operationContext.operation,
          attempt: operationContext.attempt,
          delay: delay
        });

        // 等待延迟时间
        await this.delay(delay);
      }
    }

    // 理论上不会到达这里，但为了类型安全
    const error = lastError || new Error(`Operation failed after ${this.config.maxAttempts} attempts`);
    this.errorHandler.handleError(
      error,
      { 
        component: 'ExponentialBackoffRetryStrategy', 
        operation: operationContext.operation, 
        attempts: this.config.maxAttempts,
        finalAttempt: true
      }
    );
    throw error;
  }

  /**
   * 延迟函数
   */
  private delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}