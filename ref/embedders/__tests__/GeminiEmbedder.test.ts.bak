import { GeminiEmbedder } from '../GeminiEmbedder';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { EmbeddingCacheService } from '../EmbeddingCacheService';
import { LoggerService } from '../../utils/LoggerService';
import { ConfigService } from '../../config/ConfigService';

// Mock the environment variables
process.env.GEMINI_API_KEY = 'test-gemini-api-key';
process.env.GEMINI_BASE_URL = 'https://generativelanguage.googleapis.com';
process.env.GEMINI_MODEL = 'embedding-001';
process.env.GEMINI_DIMENSIONS = '768';

describe('GeminiEmbedder', () => {
  let geminiEmbedder: GeminiEmbedder;
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
    
    // Create GeminiEmbedder instance
    geminiEmbedder = new GeminiEmbedder(logger, errorHandler, cacheService);
  });

  describe('嵌入器服务验收标准', () => {
    test('✅ Gemini嵌入器能够正常初始化', () => {
      expect(geminiEmbedder).toBeDefined();
      expect(geminiEmbedder.getModelName()).toBe('embedding-001');
      expect(geminiEmbedder.getDimensions()).toBe(768);
    });

    test('✅ Gemini嵌入器能够生成嵌入', async () => {
      const input = { text: 'test text for embedding' };
      const mockResult = {
        vector: [0.1, 0.2, 0.3],
        dimensions: 3,
        model: 'embedding-001',
        processingTime: 0
      };

      // Mock the fetch call
      global.fetch = jest.fn()
        .mockResolvedValueOnce({ // First call for embedding
          ok: true,
          json: jest.fn().mockResolvedValue({
            embedding: {
              values: [0.1, 0.2, 0.3]
            }
          })
        } as any);

      const result = await geminiEmbedder.embed(input);
      
      expect(result).toEqual({
        ...mockResult,
        processingTime: expect.any(Number)
      });
      expect(global.fetch).toHaveBeenCalledWith(
        'https://generativelanguage.googleapis.com/v1beta/models/embedding-001:embedContent?key=test-gemini-api-key',
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          }
        })
      );
    });

    test('✅ Gemini嵌入器可用性检查', async () => {
      // Mock the fetch call for availability check
      global.fetch = jest.fn().mockResolvedValue({
        ok: true,
        json: jest.fn().mockResolvedValue({ models: [] })
      } as any);

      const isAvailable = await geminiEmbedder.isAvailable();
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

      await expect(geminiEmbedder.embed(input)).rejects.toThrow('Gemini API request failed');
    });

    test('✅ 未配置API密钥时抛出错误', async () => {
      // Temporarily remove API key
      const originalApiKey = process.env.GEMINI_API_KEY;
      delete process.env.GEMINI_API_KEY;

      // Create new instance without API key
      const embedder = new GeminiEmbedder(logger, errorHandler, cacheService);
      
      const input = { text: 'test text' };
      await expect(embedder.embed(input)).rejects.toThrow('Gemini API key is not configured');

      // Restore API key
      process.env.GEMINI_API_KEY = originalApiKey;
    });
  });

  describe('性能验收标准', () => {
    test('✅ 嵌入生成响应时间合理', async () => {
      const input = { text: 'test text for performance testing' };
      const mockResult = {
        vector: Array(768).fill(0.5),
        dimensions: 768,
        model: 'embedding-001',
        processingTime: 0
      };

      global.fetch = jest.fn()
        .mockResolvedValueOnce({ // Availability check
          ok: true,
          json: jest.fn().mockResolvedValue({ models: [] })
        })
        .mockResolvedValueOnce({ // Embedding request
          ok: true,
          json: jest.fn().mockResolvedValue({
            embedding: {
              values: Array(768).fill(0.5)
            }
          })
        } as any);

      const startTime = Date.now();
      await geminiEmbedder.embed(input);
      const endTime = Date.now();

      const responseTime = endTime - startTime;
      expect(responseTime).toBeLessThan(5000); // 5秒内完成
    });
  });
});