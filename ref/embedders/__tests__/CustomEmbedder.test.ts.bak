import { CustomEmbedder } from '../CustomEmbedder';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { EmbeddingCacheService } from '../EmbeddingCacheService';
import { LoggerService } from '../../utils/LoggerService';
import { ConfigService } from '../../config/ConfigService';

// Mock the environment variables
process.env.CUSTOM_CUSTOM1_API_KEY = 'test-api-key';
process.env.CUSTOM_CUSTOM1_BASE_URL = 'https://api.test.com/embeddings';
process.env.CUSTOM_CUSTOM1_MODEL = 'test-model';
process.env.CUSTOM_CUSTOM1_DIMENSIONS = '768';

describe('CustomEmbedder', () => {
  let customEmbedder: CustomEmbedder;
  let logger: LoggerService;
  let errorHandler: ErrorHandlerService;
  let cacheService: EmbeddingCacheService;

  // Set timeout for all tests to prevent hanging
  jest.setTimeout(10000); // 10 seconds

  beforeEach(() => {
    jest.clearAllMocks();
    
    // Setup services
    // Create a mock ConfigService for testing
    const mockConfigService = {
      get: jest.fn().mockImplementation((key: string) => {
        if (key === 'logging') {
          return { level: 'info' };
        }
        return undefined;
      })
    } as unknown as ConfigService;
    
    logger = new LoggerService();
    errorHandler = new ErrorHandlerService(logger);
    cacheService = new EmbeddingCacheService(logger, errorHandler, {} as any);
    
    // Create CustomEmbedder instance
    customEmbedder = new CustomEmbedder(logger, errorHandler, cacheService, 'custom1');
  });

  afterEach(async () => {
    // Clean up logger to prevent async operations after test completion
    if (logger) {
      await logger.markAsNormalExit();
    }
    
    // Reset fetch mock
    global.fetch = undefined as any;
    
    // Wait a bit to ensure all async operations complete
    await new Promise(resolve => setTimeout(resolve, 100));
  });

  describe('嵌入器服务验收标准', () => {
    test('✅ 自定义嵌入器能够正常初始化', () => {
      expect(customEmbedder).toBeDefined();
      expect(customEmbedder.getModelName()).toBe('test-model');
      expect(customEmbedder.getDimensions()).toBe(768);
    });

    test('✅ 自定义嵌入器能够生成嵌入', async () => {
      const input = { text: 'test text for embedding' };
      const mockResult = {
        vector: [0.1, 0.2, 0.3],
        dimensions: 3,
        model: 'test-model',
        processingTime: expect.any(Number) // processingTime会由measureTime函数设置
      };

      // Mock the fetch call
      global.fetch = jest.fn().mockResolvedValue({
        ok: true,
        json: jest.fn().mockResolvedValue({
          data: [{
            embedding: [0.1, 0.2, 0.3]
          }]
        })
      } as any);

      const result = await customEmbedder.embed(input);
      
      expect(result).toEqual(mockResult);
      expect(global.fetch).toHaveBeenCalledWith(
        'https://api.test.com/embeddings',
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            'Authorization': 'Bearer test-api-key'
          }
        })
      );
    });

    test('✅ 自定义嵌入器可用性检查', async () => {
      // Mock the fetch call for availability check
      global.fetch = jest.fn().mockResolvedValue({
        ok: true
      } as any);

      const isAvailable = await customEmbedder.isAvailable();
      expect(isAvailable).toBe(true);
    });
  });

  describe('功能测试', () => {
    test('✅ 能够处理不同的API响应格式', async () => {
      const input = { text: 'test text' };

      // Test format 1: data array
      global.fetch = jest.fn().mockResolvedValue({
        ok: true,
        json: jest.fn().mockResolvedValue({
          data: [{
            embedding: [0.1, 0.2, 0.3],
            model: 'test-model'
          }]
        })
      } as any);

      let result = await customEmbedder.embed(input);
      expect(result).toEqual({
        vector: [0.1, 0.2, 0.3],
        dimensions: 3,
        model: 'test-model',
        processingTime: expect.any(Number)
      });

      // Test format 2: embeddings array
      global.fetch = jest.fn().mockResolvedValue({
        ok: true,
        json: jest.fn().mockResolvedValue({
          embeddings: [[0.4, 0.5, 0.6]]
        })
      } as any);

      result = await customEmbedder.embed(input);
      expect(result).toEqual({
        vector: [0.4, 0.5, 0.6],
        dimensions: 3,
        model: 'test-model',
        processingTime: expect.any(Number)
      });
    });

    test('✅ 能够处理错误情况', async () => {
      const input = { text: 'test text' };

      // Mock failed request
      global.fetch = jest.fn().mockResolvedValue({
        ok: false,
        text: jest.fn().mockResolvedValue('API Error'),
        status: 500
      } as any);

      await expect(customEmbedder.embed(input)).rejects.toThrow('Custom embedder API request failed');
    });
  });

  describe('性能验收标准', () => {
    test('✅ 嵌入生成响应时间合理', async () => {
      const input = { text: 'test text for performance testing' };
      const mockResult = {
        vector: Array(768).fill(0.5),
        dimensions: 768,
        model: 'test-model',
        processingTime: 100
      };

      global.fetch = jest.fn().mockResolvedValue({
        ok: true,
        json: jest.fn().mockResolvedValue({
          data: [{
            embedding: Array(768).fill(0.5)
          }]
        })
      } as any);

      const startTime = Date.now();
      await customEmbedder.embed(input);
      const endTime = Date.now();

      const responseTime = endTime - startTime;
      expect(responseTime).toBeLessThan(5000); // 5秒内完成
    });
  });
});