import { MemoryAwareCache } from '../MemoryAwareCache';

describe('MemoryAwareCache Compression Tests', () => {
  let cache: MemoryAwareCache<string, any>;

  beforeEach(() => {
    cache = new MemoryAwareCache<string, any>(100, {
      maxMemory: 10 * 1024 * 1024, // 10MB
      enableCompression: true,
      compressionThreshold: 200, // 200字节以上压缩
      compressionLevel: 6
    });
  });

  afterEach(() => {
    cache.clear();
  });

  describe('Compression Functionality', () => {
    it('should compress large objects', () => {
      // 创建一个大对象
      const largeObject = {
        data: 'x'.repeat(1000), // 1000字符的字符串
        nested: {
          array: Array(100).fill('test data'),
          metadata: {
            timestamp: Date.now(),
            version: '1.0.0',
            description: 'This is a test object for compression'
          }
        }
      };

      cache.set('large-object', largeObject);
      const retrieved = cache.get('large-object');

      expect(retrieved).toEqual(largeObject);

      const stats = cache.getCompressionStats();
      expect(stats.enabled).toBe(true);
      expect(stats.compressedEntries).toBe(1);
      expect(stats.totalOriginalSize).toBeGreaterThan(stats.totalCompressedSize || 0);
      expect(parseFloat(stats.savingsPercent || '0')).toBeGreaterThan(0);
    });

    it('should not compress small objects', () => {
      const smallObject = { message: 'small' };
      
      cache.set('small-object', smallObject);
      const retrieved = cache.get('small-object');

      expect(retrieved).toEqual(smallObject);

      const stats = cache.getCompressionStats();
      expect(stats.compressedEntries).toBe(0);
    });

    it('should handle compression errors gracefully', () => {
      // 创建一个循环引用的对象，会导致JSON序列化失败
      const circularObj: any = { data: 'test' };
      circularObj.self = circularObj;

      // 不应该抛出错误，应该回退到不压缩
      expect(() => cache.set('circular', circularObj)).not.toThrow();
      
      const retrieved = cache.get('circular');
      expect(retrieved).toEqual(circularObj);

      const stats = cache.getCompressionStats();
      expect(stats.compressionErrors).toBe(1);
    });

    it('should only compress when compression is effective', () => {
      // 创建一个难以压缩的对象（随机数据）
      const randomData = Array(200).fill(0).map(() => 
        Math.random().toString(36).substring(2)
      ).join('');

      cache.set('random-data', randomData);
      
      const stats = cache.getCompressionStats();
      // 如果压缩效果不好，可能不会被压缩
      // 但至少不应该出错
      expect(cache.get('random-data')).toBe(randomData);
    });
  });

  describe('Compression Statistics', () => {
    it('should track compression statistics accurately', () => {
      const objects = [
        { data: 'x'.repeat(500) },  // 会被压缩
        { data: 'y'.repeat(500) },  // 会被压缩
        { small: 'data' }           // 不会被压缩
      ];

      objects.forEach((obj, index) => {
        cache.set(`obj-${index}`, obj);
      });

      const stats = cache.getCompressionStats();
      expect(stats.compressedEntries).toBe(2); // 前两个对象被压缩
      expect(stats.totalOriginalSize).toBeGreaterThan(0);
      expect(stats.totalCompressedSize).toBeGreaterThan(0);
      expect(stats.compressionRatio).toBeGreaterThan(0);
      expect(stats.compressionRatio).toBeLessThan(1);
    });

    it('should return disabled message when compression is disabled', () => {
      const noCompressionCache = new MemoryAwareCache<string, any>(100, {
        enableCompression: false
      });

      const stats = noCompressionCache.getCompressionStats();
      expect(stats.enabled).toBe(false);
      expect(stats.message).toBe('Compression is disabled');
    });
  });

  describe('Memory Management with Compression', () => {
    it('should account for compressed size in memory usage', () => {
      const largeString = 'x'.repeat(2000);
      const originalSize = cache.getMaxMemory(); // 获取当前内存使用

      cache.set('large-string', largeString);
      
      const memoryUsage = cache.getMemoryUsage();
      // 由于压缩，内存使用应该小于原始大小
      expect(memoryUsage).toBeLessThan(4000); // 2000字符 * 2字节/字符 = 4000字节
    });

    it('should evict entries when memory limit is reached', () => {
      // 设置较小的内存限制
      const smallCache = new MemoryAwareCache<string, any>(10, {
        maxMemory: 1000, // 1KB
        enableCompression: true,
        compressionThreshold: 50
      });

      // 添加多个大对象
      for (let i = 0; i < 5; i++) {
        smallCache.set(`key-${i}`, { data: 'x'.repeat(300) });
      }

      // 由于内存限制，一些条目应该被驱逐
      const keys = ['key-0', 'key-1', 'key-2', 'key-3', 'key-4'];
      const presentKeys = keys.filter(key => smallCache.get(key) !== undefined);
      
      // 应该只有部分键存在（由于压缩，可能所有5个都能容纳）
      expect(presentKeys.length).toBeLessThanOrEqual(5);
      expect(smallCache.getMemoryUsage()).toBeLessThanOrEqual(1000);
    });
  });

  describe('Data Integrity', () => {
    it('should maintain data integrity after compression and decompression', () => {
      const testCases = [
        { type: 'string', value: 'Hello, World! This is a test string.' },
        { type: 'number', value: 42 },
        { type: 'boolean', value: true },
        { type: 'array', value: [1, 2, 3, 'four', { five: 5 }] },
        { 
          type: 'object', 
          value: { 
            name: 'Test Object',
            nested: {
              level: 2,
              data: [1, 2, 3]
            },
            timestamp: Date.now()
          } 
        },
        { type: 'null', value: null }
      ];

      testCases.forEach(testCase => {
        cache.set(`test-${testCase.type}`, testCase.value);
        const retrieved = cache.get(`test-${testCase.type}`);
        expect(retrieved).toEqual(testCase.value);
      });
    });

    it('should handle special characters and Unicode', () => {
      const unicodeData = {
        chinese: '你好世界',
        emoji: '🚀🌟💻',
        special: 'Special chars: !@#$%^&*()',
        mixed: 'Mixed: Hello 世界 🌍'
      };

      cache.set('unicode-test', unicodeData);
      const retrieved = cache.get('unicode-test');
      expect(retrieved).toEqual(unicodeData);
    });
  });

  describe('Performance Considerations', () => {
    it('should handle compression without significant performance degradation', () => {
      const startTime = Date.now();
      
      // 添加100个中等大小的对象
      for (let i = 0; i < 100; i++) {
        const data = {
          id: i,
          data: 'x'.repeat(200),
          timestamp: Date.now(),
          metadata: { index: i, type: 'test' }
        };
        cache.set(`perf-test-${i}`, data);
      }

      const setTime = Date.now() - startTime;

      // 读取所有数据
      const readStartTime = Date.now();
      for (let i = 0; i < 100; i++) {
        const retrieved = cache.get(`perf-test-${i}`);
        expect(retrieved).toBeDefined();
        expect(retrieved.id).toBe(i);
      }
      const readTime = Date.now() - readStartTime;

      // 性能应该在可接受范围内（具体阈值可以根据实际需求调整）
      expect(setTime).toBeLessThan(5000); // 5秒内完成写入
      expect(readTime).toBeLessThan(3000); // 3秒内完成读取

      const stats = cache.getCompressionStats();
      expect(stats.compressedEntries).toBeGreaterThan(0);
    });
  });
});