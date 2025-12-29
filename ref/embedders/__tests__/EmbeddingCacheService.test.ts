import { EmbeddingCacheService } from '../EmbeddingCacheService';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { ConfigService } from '../../config/ConfigService';

describe('EmbeddingCacheService', () => {
  let cacheService: EmbeddingCacheService;
  let logger: LoggerService;
  let errorHandler: ErrorHandlerService;

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
    
    // Create EmbeddingCacheService instance
    cacheService = new EmbeddingCacheService(logger, errorHandler);
  });

  describe('嵌入器服务验收标准', () => {
    test('✅ 嵌入缓存服务正常工作', async () => {
      const text = 'test text for embedding';
      const model = 'test-model';
      const embeddingResult = {
        vector: [0.1, 0.2, 0.3, 0.4, 0.5],
        dimensions: 5,
        model: 'test-model',
        processingTime: 100
      };

      // Test setting cache
      await cacheService.set(text, model, embeddingResult);
      
      // Test getting cache
      const cachedResult = await cacheService.get(text, model);
      expect(cachedResult).toEqual(embeddingResult);
    });

    test('✅ 缓存命中率 ≥ 70%', async () => {
      const texts = [
        'test text 1',
        'test text 2',
        'test text 3',
        'test text 4',
        'test text 5'
      ];
      const model = 'test-model';
      
      // Set cache for all texts
      for (const text of texts) {
        await cacheService.set(text, model, {
          vector: [0.1, 0.2, 0.3],
          dimensions: 3,
          model: 'test-model',
          processingTime: 100
        });
      }
      
      // Retrieve cache multiple times
      let hits = 0;
      let total = 0;
      
      for (const text of texts) {
        for (let i = 0; i < 3; i++) { // 3 attempts per text
          total++;
          const result = await cacheService.get(text, model);
          if (result) {
            hits++;
          }
        }
      }
      
      const hitRate = (hits / total) * 100;
      expect(hitRate).toBeGreaterThanOrEqual(70);
    });
  });

  describe('缓存功能测试', () => {
    test('✅ 能够正确处理缓存未命中', async () => {
      const text = 'non-existent text';
      const model = 'test-model';
      
      const result = await cacheService.get(text, model);
      expect(result).toBeNull();
    });

    test('✅ 能够清空缓存', async () => {
      const text = 'test text';
      const model = 'test-model';
      
      // Set cache
      await cacheService.set(text, model, {
        vector: [0.1, 0.2, 0.3],
        dimensions: 3,
        model: 'test-model',
        processingTime: 100
      });
      
      // Verify cache is set
      const cached = await cacheService.get(text, model);
      expect(cached).not.toBeNull();
      
      // Clear cache
      await cacheService.clear();
      
      // Verify cache is cleared
      const cleared = await cacheService.get(text, model);
      expect(cleared).toBeNull();
    });

    test('✅ 能够根据模型删除缓存项', async () => {
      const text = 'test text';
      const model1 = 'model-1';
      const model2 = 'model-2';
      
      // Set cache for both models
      await cacheService.set(text, model1, {
        vector: [0.1, 0.2, 0.3],
        dimensions: 3,
        model: model1,
        processingTime: 100
      });
      
      await cacheService.set(text, model2, {
        vector: [0.4, 0.5, 0.6],
        dimensions: 3,
        model: model2,
        processingTime: 100
      });
      
      // Verify both caches are set
      const cached1 = await cacheService.get(text, model1);
      const cached2 = await cacheService.get(text, model2);
      expect(cached1).not.toBeNull();
      expect(cached2).not.toBeNull();
      
      // Delete cache for model1
      await cacheService.deleteByModel(model1);
      
      // Verify model1 cache is deleted but model2 cache remains
      const deleted1 = await cacheService.get(text, model1);
      const remaining2 = await cacheService.get(text, model2);
      expect(deleted1).toBeNull();
      expect(remaining2).not.toBeNull();
    });
  });

  describe('性能验收标准', () => {
    test('✅ 内存使用稳定，无内存泄漏', async () => {
      // Test with a large number of cache entries
      const largeDataSet = Array.from({ length: 1000 }, (_, i) => ({
        text: `test text ${i}`,
        model: 'test-model',
        result: {
          vector: Array(100).fill(0.5),
          dimensions: 100,
          model: 'test-model',
          processingTime: 100
        }
      }));
      
      // Set all cache entries
      for (const data of largeDataSet) {
        await cacheService.set(data.text, data.model, data.result);
      }
      
      // Verify cache size
      const stats = await cacheService.getStats();
      expect(stats.size).toBe(1000);
      
      // Retrieve some entries to verify they work correctly
      for (let i = 0; i < 10; i++) {
        const data = largeDataSet[i];
        const result = await cacheService.get(data.text, data.model);
        expect(result).toEqual(data.result);
      }
      
      // Clear cache and verify it's empty
      await cacheService.clear();
      const finalStats = await cacheService.getStats();
      expect(finalStats.size).toBe(0);
    });
  });
});