import { ExponentialBackoffRetryStrategy } from '../RetryStrategy';
import { LoggerService } from '../../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../../utils/ErrorHandlerService';
import { NebulaConfigService } from '../../../../config/service/NebulaConfigService';

// Mock dependencies
jest.mock('../../../../utils/LoggerService');
jest.mock('../../../../utils/ErrorHandlerService');
jest.mock('../../../../config/service/NebulaConfigService');

describe('ExponentialBackoffRetryStrategy', () => {
  let retryStrategy: ExponentialBackoffRetryStrategy;
  let mockLogger: jest.Mocked<LoggerService>;
  let mockErrorHandler: jest.Mocked<ErrorHandlerService>;
  let mockConfigService: jest.Mocked<NebulaConfigService>;

  beforeEach(() => {
    // Create mock instances with proper dependencies
    mockLogger = {
      info: jest.fn(),
      debug: jest.fn(),
      warn: jest.fn(),
      error: jest.fn()
    } as any;

    mockErrorHandler = {
      handleError: jest.fn()
    } as any;

    mockConfigService = {} as any;

    retryStrategy = new ExponentialBackoffRetryStrategy(
      mockLogger,
      mockErrorHandler,
      mockConfigService
    );
  });

  describe('shouldRetry', () => {
    it('should return false when attempt exceeds maxAttempts', () => {
      const context = {
        operation: 'test',
        attempt: 4, // 超过默认的 maxAttempts: 3
        error: new Error('ECONNREFUSED')
      };

      expect(retryStrategy.shouldRetry(context)).toBe(false);
    });

    it('should return false when no error is provided', () => {
      const context = {
        operation: 'test',
        attempt: 1
      };

      expect(retryStrategy.shouldRetry(context)).toBe(false);
    });

    it('should return true for retryable error messages', () => {
      const context = {
        operation: 'test',
        attempt: 1,
        error: new Error('ECONNREFUSED')
      };

      expect(retryStrategy.shouldRetry(context)).toBe(true);
    });

    it('should return true for retryable error codes', () => {
      const error = new Error('Connection failed') as any;
      error.code = 'ETIMEDOUT';
      
      const context = {
        operation: 'test',
        attempt: 1,
        error
      };

      expect(retryStrategy.shouldRetry(context)).toBe(true);
    });

    it('should return false for non-retryable errors', () => {
      const context = {
        operation: 'test',
        attempt: 1,
        error: new Error('Invalid syntax')
      };

      expect(retryStrategy.shouldRetry(context)).toBe(false);
    });

    it('should return true if either error message or code matches', () => {
      const error = new Error('Some unknown error') as any;
      error.code = 'ECONNRESET';
      
      const context = {
        operation: 'test',
        attempt: 1,
        error
      };

      expect(retryStrategy.shouldRetry(context)).toBe(true);
    });
  });

  describe('getDelay', () => {
    it('should calculate exponential backoff correctly', () => {
      // 创建一个不带抖动的策略来测试基础计算
      const noJitterStrategy = new ExponentialBackoffRetryStrategy(
        mockLogger,
        mockErrorHandler,
        mockConfigService,
        { jitter: false }
      );

      const context1 = { operation: 'test', attempt: 1 };
      const context2 = { operation: 'test', attempt: 2 };
      const context3 = { operation: 'test', attempt: 3 };

      const delay1 = noJitterStrategy.getDelay(context1);
      const delay2 = noJitterStrategy.getDelay(context2);
      const delay3 = noJitterStrategy.getDelay(context3);

      // 第一次重试应该是基础延迟
      expect(delay1).toBe(1000);
      // 第二次重试应该是基础延迟 * backoffFactor
      expect(delay2).toBe(2000);
      // 第三次重试应该是基础延迟 * backoffFactor^2
      expect(delay3).toBe(4000);
    });

    it('should respect maxDelay limit', () => {
      // 使用一个很小的 maxDelay 来测试限制
      const customStrategy = new ExponentialBackoffRetryStrategy(
        mockLogger,
        mockErrorHandler,
        mockConfigService,
        { maxDelay: 1500, jitter: false }
      );

      const context = { operation: 'test', attempt: 5 }; // 高次重试
      const delay = customStrategy.getDelay(context);

      expect(delay).toBeLessThanOrEqual(1500);
    });

    it('should apply jitter when enabled', () => {
      const context = { operation: 'test', attempt: 1 };
      
      // 多次调用应该产生不同的延迟（由于抖动）
      const delays = Array.from({ length: 10 }, () => retryStrategy.getDelay(context));
      const uniqueDelays = new Set(delays);
      
      // 应该有多个不同的延迟值
      expect(uniqueDelays.size).toBeGreaterThan(1);
    });

    it('should handle custom backoff factor', () => {
      const customStrategy = new ExponentialBackoffRetryStrategy(
        mockLogger,
        mockErrorHandler,
        mockConfigService,
        { backoffFactor: 3, jitter: false }
      );

      const context1 = { operation: 'test', attempt: 1 };
      const context2 = { operation: 'test', attempt: 2 };

      const delay1 = customStrategy.getDelay(context1);
      const delay2 = customStrategy.getDelay(context2);

      expect(delay1).toBe(1000);
      expect(delay2).toBe(3000); // 1000 * 3
    });
  });

  describe('executeWithRetry', () => {
    it('should succeed on first attempt', async () => {
      const mockOperation = jest.fn().mockResolvedValue('success');
      
      const result = await retryStrategy.executeWithRetry(mockOperation, {
        operation: 'test-operation'
      });

      expect(result).toBe('success');
      expect(mockOperation).toHaveBeenCalledTimes(1);
    });

    it('should retry on retryable errors', async () => {
      // 使用更短的延迟来加速测试
      const fastRetryStrategy = new ExponentialBackoffRetryStrategy(
        mockLogger,
        mockErrorHandler,
        mockConfigService,
        { baseDelay: 1, maxDelay: 10, jitter: false }
      );

      const mockOperation = jest.fn()
        .mockRejectedValueOnce(new Error('ECONNREFUSED'))
        .mockRejectedValueOnce(new Error('ECONNRESET'))
        .mockResolvedValue('success');

      const startTime = Date.now();
      const result = await fastRetryStrategy.executeWithRetry(mockOperation, {
        operation: 'test-operation'
      });
      const endTime = Date.now();

      expect(result).toBe('success');
      expect(mockOperation).toHaveBeenCalledTimes(3);
      // 应该有一些延迟时间
      expect(endTime - startTime).toBeGreaterThan(0);
    });

    it('should not retry on non-retryable errors', async () => {
      const mockOperation = jest.fn().mockRejectedValue(new Error('Invalid syntax'));
      
      await expect(
        retryStrategy.executeWithRetry(mockOperation, {
          operation: 'test-operation'
        })
      ).rejects.toThrow('Invalid syntax');

      expect(mockOperation).toHaveBeenCalledTimes(1);
    });

    it('should stop after max attempts', async () => {
      // 使用更短的延迟来加速测试
      const fastRetryStrategy = new ExponentialBackoffRetryStrategy(
        mockLogger,
        mockErrorHandler,
        mockConfigService,
        { baseDelay: 1, maxDelay: 10, jitter: false }
      );

      const mockOperation = jest.fn().mockRejectedValue(new Error('ECONNREFUSED'));
      
      const startTime = Date.now();
      await expect(
        fastRetryStrategy.executeWithRetry(mockOperation, {
          operation: 'test-operation'
        })
      ).rejects.toThrow('ECONNREFUSED');
      const endTime = Date.now();

      expect(mockOperation).toHaveBeenCalledTimes(3); // 默认 maxAttempts
      // 应该有一些延迟时间但不会太长
      expect(endTime - startTime).toBeLessThan(100);
    });

    it('should handle custom maxAttempts', async () => {
      const customStrategy = new ExponentialBackoffRetryStrategy(
        mockLogger,
        mockErrorHandler,
        mockConfigService,
        { maxAttempts: 2, baseDelay: 1, maxDelay: 10, jitter: false }
      );

      const mockOperation = jest.fn().mockRejectedValue(new Error('ECONNREFUSED'));
      
      await expect(
        customStrategy.executeWithRetry(mockOperation, {
          operation: 'test-operation'
        })
      ).rejects.toThrow('ECONNREFUSED');

      expect(mockOperation).toHaveBeenCalledTimes(2); // 自定义 maxAttempts
    });

    it('should pass context metadata correctly', async () => {
      const mockOperation = jest.fn().mockResolvedValue('success');
      const metadata = { userId: '123', requestId: '456' };
      
      await retryStrategy.executeWithRetry(mockOperation, {
        operation: 'test-operation',
        metadata
      });

      expect(mockOperation).toHaveBeenCalledTimes(1);
    });

    it('should handle errors without message', async () => {
      const mockOperation = jest.fn().mockRejectedValue('String error');
      
      await expect(
        retryStrategy.executeWithRetry(mockOperation, {
          operation: 'test-operation'
        })
      ).rejects.toThrow('String error');

      expect(mockOperation).toHaveBeenCalledTimes(1);
    });
  });

  describe('constructor', () => {
    it('should use default config when no custom config provided', () => {
      const strategy = new ExponentialBackoffRetryStrategy(
        mockLogger,
        mockErrorHandler,
        mockConfigService
      );

      expect(mockLogger.info).toHaveBeenCalledWith('ExponentialBackoffRetryStrategy initialized', {
        maxAttempts: 3,
        baseDelay: 1000,
        maxDelay: 30000,
        backoffFactor: 2,
        jitter: true
      });
    });

    it('should merge custom config with defaults', () => {
      const customConfig = {
        maxAttempts: 5,
        baseDelay: 500
      };

      const strategy = new ExponentialBackoffRetryStrategy(
        mockLogger,
        mockErrorHandler,
        mockConfigService,
        customConfig
      );

      expect(mockLogger.info).toHaveBeenCalledWith('ExponentialBackoffRetryStrategy initialized', {
        maxAttempts: 5,
        baseDelay: 500,
        maxDelay: 30000, // 默认值
        backoffFactor: 2, // 默认值
        jitter: true // 默认值
      });
    });
  });
});