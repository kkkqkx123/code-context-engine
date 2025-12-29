import { OllamaEmbedder } from '../OllamaEmbedder';
import { EmbeddingCacheService } from '../EmbeddingCacheService';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { ConfigService } from '../../config/ConfigService';

// Mock fetch for API calls
global.fetch = jest.fn();

describe('OllamaEmbedder', () => {
  let ollamaEmbedder: OllamaEmbedder;
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
    
    // Create OllamaEmbedder instance
    ollamaEmbedder = new OllamaEmbedder(logger, errorHandler, cacheService);
  });

  describe('嵌入器服务验收标准', () => {
    test('✅ Ollama嵌入器能够生成嵌入', async () => {
      const input = { text: 'test text for embedding' };
      const mockResponse = {
        embedding: [0.1, 0.2, 0.3, 0.4, 0.5]
      };
      
      (global.fetch as jest.Mock).mockResolvedValue({
        ok: true,
        json: jest.fn().mockResolvedValue(mockResponse)
      });
      
      const result = await ollamaEmbedder.embed(input);
      
      // Check if result is an array and handle accordingly
      if (Array.isArray(result)) {
        expect(result[0].vector).toEqual([0.1, 0.2, 0.3, 0.4, 0.5]);
        expect(result[0].dimensions).toBe(5);
        expect(result[0].model).toBe('nomic-embed-text');
      } else {
        expect(result.vector).toEqual([0.1, 0.2, 0.3, 0.4, 0.5]);
        expect(result.dimensions).toBe(5);
        expect(result.model).toBe('nomic-embed-text');
      }
      expect(global.fetch).toHaveBeenCalledWith(
        'http://localhost:11434/api/embeddings',
        expect.objectContaining({
          method: 'POST',
          headers: expect.objectContaining({
            'Content-Type': 'application/json'
          }),
          body: JSON.stringify({
            prompt: 'test text for embedding',
            model: 'nomic-embed-text'
          })
        })
      );
    });
  });

  describe('可用性检查', () => {
    test('✅ 能够检查Ollama服务可用性', async () => {
      (global.fetch as jest.Mock).mockResolvedValue({
        ok: true,
        json: jest.fn().mockResolvedValue({
          models: [
            { name: 'nomic-embed-text' },
            { name: 'llama2' }
          ]
        })
      });
      
      const isAvailable = await ollamaEmbedder.isAvailable();
      expect(isAvailable).toBe(true);
    });

    test('✅ 当模型不存在时返回不可用', async () => {
      (global.fetch as jest.Mock).mockResolvedValue({
        ok: true,
        json: jest.fn().mockResolvedValue({
          models: [
            { name: 'llama2' },
            { name: 'mistral' }
          ]
        })
      });
      
      const isAvailable = await ollamaEmbedder.isAvailable();
      expect(isAvailable).toBe(false);
    });
  });

  describe('模型管理', () => {
    test('✅ 能够获取模型维度', () => {
      const dimensions = ollamaEmbedder.getDimensions();
      expect(dimensions).toBe(768); // Default dimension
    });

    test('✅ 能够获取模型名称', () => {
      const modelName = ollamaEmbedder.getModelName();
      expect(modelName).toBe('nomic-embed-text'); // Default model
    });

    test('✅ 能够设置模型', () => {
      ollamaEmbedder.setModel('llama2');
      const modelName = ollamaEmbedder.getModelName();
      expect(modelName).toBe('llama2');
    });

    test('✅ 能够获取可用模型列表', async () => {
      (global.fetch as jest.Mock).mockResolvedValue({
        ok: true,
        json: jest.fn().mockResolvedValue({
          models: [
            { name: 'nomic-embed-text' },
            { name: 'llama2' },
            { name: 'mistral' }
          ]
        })
      });
      
      const models = await ollamaEmbedder.getAvailableModels();
      expect(models).toContain('nomic-embed-text');
      expect(models).toContain('llama2');
      expect(models).toContain('mistral');
    });
  });

  describe('性能验收标准', () => {
    test('✅ 嵌入生成响应时间 < 10秒', async () => {
      const input = { text: 'test text for performance testing' };
      const mockResponse = {
        embedding: Array(768).fill(0.5)
      };
      
      (global.fetch as jest.Mock).mockResolvedValue({
        ok: true,
        json: jest.fn().mockResolvedValue(mockResponse)
      });
      
      const startTime = Date.now();
      await ollamaEmbedder.embed(input);
      const endTime = Date.now();
      
      const responseTime = endTime - startTime;
      expect(responseTime).toBeLessThan(10000); // 10秒 = 10000毫秒
    });

    test('✅ 并发处理能力 ≥ 5个请求', async () => {
      const inputs = Array.from({ length: 5 }, (_, i) => ({
        text: `test text ${i}`
      }));
      
      const mockResponse = {
        embedding: [0.1, 0.2, 0.3]
      };
      
      (global.fetch as jest.Mock).mockResolvedValue({
        ok: true,
        json: jest.fn().mockResolvedValue(mockResponse)
      });
      
      // Create multiple concurrent requests
      const promises = inputs.map(input => ollamaEmbedder.embed(input));
      const results = await Promise.all(promises);
      
      expect(results).toHaveLength(5);
      results.forEach(result => {
        // Check if result is an array and handle accordingly
        if (Array.isArray(result)) {
          expect(result[0].vector).toEqual([0.1, 0.2, 0.3]);
        } else {
          expect(result.vector).toEqual([0.1, 0.2, 0.3]);
        }
      });
      
      // Verify all requests were processed
      expect(global.fetch).toHaveBeenCalledTimes(5);
    });
  });

  describe('错误处理', () => {
    test('✅ 能够处理API错误', async () => {
      const input = { text: 'test text for error handling' };
      
      (global.fetch as jest.Mock).mockResolvedValue({
        ok: false,
        text: jest.fn().mockResolvedValue('API Error')
      });
      
      await expect(ollamaEmbedder.embed(input)).rejects.toThrow();
    });

    test('✅ 能够处理网络错误', async () => {
      const input = { text: 'test text for network error' };
      
      (global.fetch as jest.Mock).mockRejectedValue(new Error('Network error'));
      
      await expect(ollamaEmbedder.embed(input)).rejects.toThrow();
    });
  });
});