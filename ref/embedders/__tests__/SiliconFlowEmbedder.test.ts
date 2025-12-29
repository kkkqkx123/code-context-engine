import { SiliconFlowEmbedder } from '../SiliconFlowEmbedder';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { EmbeddingCacheService } from '../EmbeddingCacheService';
import { LoggerService } from '../../utils/LoggerService';
import { ConfigService } from '../../config/ConfigService';

// Mock the environment variables
process.env.SILICONFLOW_API_KEY = 'test-siliconflow-api-key';
process.env.SILICONFLOW_BASE_URL = 'https://api.siliconflow.cn/v1';
process.env.SILICONFLOW_MODEL = 'BAAI/bge-m3';
process.env.SILICONFLOW_DIMENSIONS = '1024';

describe('SiliconFlowEmbedder', () => {
  let siliconFlowEmbedder: SiliconFlowEmbedder;
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
    cacheService = new EmbeddingCacheService(logger, errorHandler);
    
    // Create SiliconFlowEmbedder instance
    siliconFlowEmbedder = new SiliconFlowEmbedder(logger, errorHandler, cacheService);
  });

  describe('嵌入器服务验收标准', () => {
    test('✅ SiliconFlow嵌入器能够正常初始化', () => {
      expect(siliconFlowEmbedder).toBeDefined();
      expect(siliconFlowEmbedder.getModelName()).toBe('BAAI/bge-m3');
      expect(siliconFlowEmbedder.getDimensions()).toBe(1024);
    });

    test('✅ SiliconFlow嵌入器能够生成嵌入', async () => {
      const input = { text: 'test text for embedding' };
      const mockResult = {
        vector: [0.1, 0.2, 0.3],
        dimensions: 3,
        model: 'BAAI/bge-m3',
        processingTime: expect.any(Number) // 动态处理时间
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

      const result = await siliconFlowEmbedder.embed(input);
      
      expect(result).toEqual(mockResult);
      // 确保处理时间合理 - result 是单个 EmbeddingResult
      expect((result as any).processingTime).toBeGreaterThanOrEqual(0);
      expect(global.fetch).toHaveBeenCalledWith(
        'https://api.siliconflow.cn/v1/embeddings',
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            'Authorization': 'Bearer test-siliconflow-api-key'
          }
        })
      );
    });

    test('✅ SiliconFlow嵌入器可用性检查', async () => {
      // Mock the fetch call for availability check
      global.fetch = jest.fn().mockResolvedValue({
        ok: true
      } as any);

      const isAvailable = await siliconFlowEmbedder.isAvailable();
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

      await expect(siliconFlowEmbedder.embed(input)).rejects.toThrow('SiliconFlow API request failed');
    });

    test('✅ 未配置API密钥时抛出错误', async () => {
      // Temporarily remove API key
      const originalApiKey = process.env.SILICONFLOW_API_KEY;
      delete process.env.SILICONFLOW_API_KEY;

      // Create new instance without API key
      const embedder = new SiliconFlowEmbedder(logger, errorHandler, cacheService);
      
      const input = { text: 'test text' };
      await expect(embedder.embed(input)).rejects.toThrow('SiliconFlow API key is not configured');

      // Restore API key
      process.env.SILICONFLOW_API_KEY = originalApiKey;
    });
  });

  describe('性能验收标准', () => {
    test('✅ 嵌入生成响应时间合理', async () => {
      const input = { text: 'test text for performance testing' };
      const mockResult = {
        vector: Array(1024).fill(0.5),
        dimensions: 1024,
        model: 'BAAI/bge-m3',
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
      await siliconFlowEmbedder.embed(input);
      const endTime = Date.now();

      const responseTime = endTime - startTime;
      expect(responseTime).toBeLessThan(5000); // 5秒内完成
    });
  });
});