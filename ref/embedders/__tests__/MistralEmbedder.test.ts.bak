import { MistralEmbedder } from '../MistralEmbedder';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { EmbeddingCacheService } from '../EmbeddingCacheService';
import { LoggerService } from '../../utils/LoggerService';
import { ConfigService } from '../../config/ConfigService';

// Mock the environment variables
process.env.MISTRAL_API_KEY = 'test-mistral-api-key';
process.env.MISTRAL_BASE_URL = 'https://api.mistral.ai';
process.env.MISTRAL_MODEL = 'mistral-embed';
process.env.MISTRAL_DIMENSIONS = '1024';

describe('MistralEmbedder', () => {
  let mistralEmbedder: MistralEmbedder;
  let logger: LoggerService;
  let errorHandler: ErrorHandlerService;
  let cacheService: EmbeddingCacheService;

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

    // Spy on error handler to prevent actual error logging during tests
    jest.spyOn(errorHandler, 'handleError').mockImplementation((error, context) => {
      return {
        id: 'test-error-id',
        timestamp: new Date(),
        component: context.component,
        operation: context.operation,
        message: error.message,
        stack: error.stack,
        context: context
      };
    });

    // Create MistralEmbedder instance
    mistralEmbedder = new MistralEmbedder(logger, errorHandler, cacheService);
  });

  describe('嵌入器服务验收标准', () => {
    test('✅ Mistral嵌入器能够正常初始化', () => {
      expect(mistralEmbedder).toBeDefined();
      expect(mistralEmbedder.getModelName()).toBe('mistral-embed');
      expect(mistralEmbedder.getDimensions()).toBe(1024);
    });

    test('✅ Mistral嵌入器能够生成嵌入', async () => {
      const input = { text: 'test text for embedding' };
      const mockResult = {
        vector: [0.1, 0.2, 0.3],
        dimensions: 3,
        model: 'mistral-embed',
        processingTime: 0
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

      const result = await mistralEmbedder.embed(input);

      expect(result).toEqual(mockResult);
      expect(global.fetch).toHaveBeenCalledWith(
        'https://api.mistral.ai/v1/embeddings',
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Authorization': 'Bearer test-mistral-api-key',
            'Content-Type': 'application/json',
          }
        })
      );
    });

    test('✅ Mistral嵌入器可用性检查', async () => {
      // Mock the fetch call for availability check
      global.fetch = jest.fn().mockResolvedValue({
        ok: true
      } as any);

      const isAvailable = await mistralEmbedder.isAvailable();
      expect(isAvailable).toBe(true);
    });
  });

  describe('功能测试', () => {
    test('✅ 能够处理错误情况', async () => {
      const input = { text: 'test text' };

      // Mock failed request
      global.fetch = jest.fn().mockResolvedValue({
        ok: false,
        text: jest.fn().mockResolvedValue('API Error')
      } as any);

      await expect(mistralEmbedder.embed(input)).rejects.toThrow('Mistral API request failed');
    });

    test('✅ 未配置API密钥时抛出错误', async () => {
      // Temporarily remove API key
      const originalApiKey = process.env.MISTRAL_API_KEY;
      delete process.env.MISTRAL_API_KEY;

      // Create new instance without API key
      const embedder = new MistralEmbedder(logger, errorHandler, cacheService);

      const input = { text: 'test text' };
      await expect(embedder.embed(input)).rejects.toThrow('Mistral API key is not configured');

      // Restore API key
      process.env.MISTRAL_API_KEY = originalApiKey;
    });
  });

  describe('性能验收标准', () => {
    test('✅ 嵌入生成响应时间合理', async () => {
      const input = { text: 'test text for performance testing' };
      const mockResult = {
        vector: Array(1024).fill(0.5),
        dimensions: 1024,
        model: 'mistral-embed',
        processingTime: 0
      };

      global.fetch = jest.fn().mockResolvedValue({
        ok: true,
        json: jest.fn().mockResolvedValue({
          data: [{
            embedding: Array(1024).fill(0.5)
          }]
        })
      } as any);

      const startTime = Date.now();
      await mistralEmbedder.embed(input);
      const endTime = Date.now();

      const responseTime = endTime - startTime;
      expect(responseTime).toBeLessThan(5000); // 5秒内完成
    });
  });
});