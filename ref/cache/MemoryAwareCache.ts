import { LRUCache, CacheOptions } from './LRUCache';
import * as zlib from 'zlib';

export interface MemoryOptions {
  maxMemory?: number; // 最大内存使用（字节）
  memoryCheckInterval?: number; // 内存检查间隔（毫秒）
  enableCompression?: boolean; // 启用数据压缩
  compressionThreshold?: number; // 压缩阈值（字节）
  compressionLevel?: number; // 压缩级别 (1-9)
  enableDetailedStats?: boolean; // 启用详细统计
}

interface CompressedEntry {
  compressed: boolean;
  data: Buffer; // 压缩后的二进制数据
  originalSize: number; // 原始大小
  compressedSize: number; // 压缩后大小
}

interface MemoryStats {
  totalEntries: number;
  compressedEntries: number;
  uncompressedEntries: number;
  totalMemoryUsage: number;
  compressedMemoryUsage: number;
  uncompressedMemoryUsage: number;
  compressionRatio: number;
  spaceSaved: number;
  averageEntrySize: number;
  largestEntrySize: number;
  smallestEntrySize: number;
}

export class MemoryAwareCache<K, V> extends LRUCache<K, V> {
  private maxMemory: number;
  private memoryUsage: number;
  private sizeMap: Map<K, number>;
  private enableCompression: boolean;
  private compressionThreshold: number;
  private compressionLevel: number;
  private enableDetailedStats: boolean;
  private compressionStats = {
    compressedEntries: 0,
    totalOriginalSize: 0,
    totalCompressedSize: 0,
    compressionErrors: 0,
    compressionAttempts: 0
  };
  private detailedStats = {
    totalEntries: 0,
    largestEntrySize: 0,
    smallestEntrySize: Infinity,
    entrySizeHistory: [] as number[]
  };

  constructor(maxSize: number = 1000, options: MemoryOptions & CacheOptions = {}) {
    super(maxSize, options);
    this.maxMemory = options.maxMemory || 100 * 1024 * 1024; // 100MB 默认
    this.memoryUsage = 0;
    this.sizeMap = new Map();
    this.enableCompression = options.enableCompression || false;
    this.compressionThreshold = options.compressionThreshold || 1024; // 1KB以上才压缩
    this.compressionLevel = options.compressionLevel || 6; // 默认压缩级别
    this.enableDetailedStats = options.enableDetailedStats || false;
  }

  set(key: K, value: V, ttl?: number): void {
    const entryInfo = this.processValue(value);
    const entrySize = entryInfo.size;

    // 如果key已存在，减去旧大小
    if (this.sizeMap.has(key)) {
      this.memoryUsage -= this.sizeMap.get(key)!;
      this.detailedStats.totalEntries--;
    }

    // 检查内存限制，必要时驱逐条目
    while (this.memoryUsage + entrySize > this.maxMemory && this.size() > 0) {
      this.evictForMemory();
    }

    this.sizeMap.set(key, entrySize);
    this.memoryUsage += entrySize;
    this.detailedStats.totalEntries++;

    // 更新详细统计
    if (this.enableDetailedStats) {
      this.updateDetailedStats(entrySize);
    }

    super.set(key, entryInfo.data, ttl);
  }

  get(key: K): V | undefined {
    const result = super.get(key);
    if (result === undefined) {
      return undefined;
    }

    // 如果是压缩数据，需要解压
    if (this.enableCompression && this.isCompressedEntry(result)) {
      return this.decompressData(result);
    }

    return result;
  }

  delete(key: K): boolean {
    const result = super.delete(key);
    if (result && this.sizeMap.has(key)) {
      this.memoryUsage -= this.sizeMap.get(key)!;
      this.sizeMap.delete(key);
      this.detailedStats.totalEntries--;
    }
    return result;
  }

  clear(): void {
    super.clear();
    this.memoryUsage = 0;
    this.sizeMap.clear();
    this.compressionStats = {
      compressedEntries: 0,
      totalOriginalSize: 0,
      totalCompressedSize: 0,
      compressionErrors: 0,
      compressionAttempts: 0
    };
    this.detailedStats = {
      totalEntries: 0,
      largestEntrySize: 0,
      smallestEntrySize: Infinity,
      entrySizeHistory: []
    };
  }

  getMemoryUsage(): number {
    return this.memoryUsage;
  }

  getMaxMemory(): number {
    return this.maxMemory;
  }

  getCompressionStats() {
    if (!this.enableCompression) {
      return {
        enabled: false,
        message: 'Compression is disabled'
      };
    }

    const compressionRatio = this.compressionStats.totalOriginalSize > 0
      ? this.compressionStats.totalCompressedSize / this.compressionStats.totalOriginalSize
      : 1;

    const successRate = this.compressionStats.compressionAttempts > 0
      ? ((this.compressionStats.compressedEntries / this.compressionStats.compressionAttempts) * 100).toFixed(2)
      : '0.00';

    return {
      enabled: true,
      compressedEntries: this.compressionStats.compressedEntries,
      totalOriginalSize: this.compressionStats.totalOriginalSize,
      totalCompressedSize: this.compressionStats.totalCompressedSize,
      compressionRatio: compressionRatio,
      savingsPercent: ((1 - compressionRatio) * 100).toFixed(2) + '%',
      compressionErrors: this.compressionStats.compressionErrors,
      compressionAttempts: this.compressionStats.compressionAttempts,
      successRate: successRate + '%',
      averageCompressionRatio: this.compressionStats.compressedEntries > 0
        ? (this.compressionStats.totalCompressedSize / this.compressionStats.totalOriginalSize).toFixed(3)
        : '0.000'
    };
  }

  getDetailedMemoryStats(): MemoryStats {
    const compressedMemory = this.compressionStats.totalCompressedSize;
    const uncompressedMemory = this.memoryUsage - compressedMemory;

    const totalEntries = this.detailedStats.totalEntries;
    const compressedEntries = this.compressionStats.compressedEntries;
    const uncompressedEntries = totalEntries - compressedEntries;

    const compressionRatio = this.compressionStats.totalOriginalSize > 0
      ? this.compressionStats.totalCompressedSize / this.compressionStats.totalOriginalSize
      : 1;

    const spaceSaved = this.compressionStats.totalOriginalSize - this.compressionStats.totalCompressedSize;

    const averageEntrySize = totalEntries > 0
      ? this.memoryUsage / totalEntries
      : 0;

    return {
      totalEntries,
      compressedEntries,
      uncompressedEntries,
      totalMemoryUsage: this.memoryUsage,
      compressedMemoryUsage: compressedMemory,
      uncompressedMemoryUsage: uncompressedMemory,
      compressionRatio,
      spaceSaved,
      averageEntrySize,
      largestEntrySize: this.detailedStats.largestEntrySize,
      smallestEntrySize: this.detailedStats.smallestEntrySize === Infinity ? 0 : this.detailedStats.smallestEntrySize
    };
  }

  getMemoryUtilization(): number {
    return (this.memoryUsage / this.maxMemory) * 100;
  }

  private processValue(value: V): { data: any; size: number } {
    if (!this.enableCompression) {
      const size = this.estimateSize(value);
      return { data: value, size };
    }

    const originalSize = this.estimateSize(value);
    this.compressionStats.compressionAttempts++;

    // 只对大于阈值的数据进行压缩
    if (originalSize < this.compressionThreshold) {
      return { data: value, size: originalSize };
    }

    try {
      const compressedData = this.compressData(value);
      const compressedSize = compressedData.length;

      // 只有压缩有效时才使用压缩数据（至少10%的压缩率）
      if (compressedSize < originalSize * 0.9) {
        this.compressionStats.compressedEntries++;
        this.compressionStats.totalOriginalSize += originalSize;
        this.compressionStats.totalCompressedSize += compressedSize;

        return {
          data: {
            compressed: true,
            data: compressedData,
            originalSize,
            compressedSize
          } as CompressedEntry,
          size: compressedSize + 64 // 加上压缩标记的开销
        };
      }
    } catch (error) {
      // 压缩失败，记录错误统计
      this.compressionStats.compressionErrors++;
      console.warn(`Compression failed for value:`, error);
    }

    return { data: value, size: originalSize };
  }

  private updateDetailedStats(entrySize: number): void {
    this.detailedStats.largestEntrySize = Math.max(this.detailedStats.largestEntrySize, entrySize);
    this.detailedStats.smallestEntrySize = Math.min(this.detailedStats.smallestEntrySize, entrySize);

    // 保持最近100个条目的大小历史
    this.detailedStats.entrySizeHistory.push(entrySize);
    if (this.detailedStats.entrySizeHistory.length > 100) {
      this.detailedStats.entrySizeHistory.shift();
    }
  }

  private isCompressedEntry(data: any): data is CompressedEntry {
    return typeof data === 'object' && data !== null && 'compressed' in data && data.compressed === true;
  }

  private compressData(value: V): Buffer {
    try {
      // 将值序列化为JSON字符串
      const jsonString = JSON.stringify(value);

      // 使用zlib进行真正的压缩
      const compressed = zlib.deflateSync(jsonString, {
        level: this.compressionLevel
      });

      return compressed;
    } catch (error) {
      throw new Error(`Compression failed: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  private decompressData(entry: CompressedEntry): V {
    try {
      // 使用zlib解压缩
      const decompressed = zlib.inflateSync(entry.data);

      // 将解压缩后的数据解析为JSON
      const jsonString = decompressed.toString('utf8');
      return JSON.parse(jsonString) as V;
    } catch (error) {
      throw new Error(`Decompression failed: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  private estimateSize(value: V): number {
    if (value === null || value === undefined) {
      return 0;
    }

    if (typeof value === 'string') {
      return value.length * 2; // UTF-16
    }

    if (typeof value === 'number') {
      return 8; // 64位数字
    }

    if (typeof value === 'boolean') {
      return 4;
    }

    if (typeof value === 'object') {
      try {
        // 对于对象和数组，使用JSON序列化来估算大小
        const jsonString = JSON.stringify(value);
        return jsonString.length * 2 + 64; // JSON字符串大小 + 对象开销
      } catch (error) {
        // 如果JSON序列化失败（如循环引用），返回默认大小
        return 1024; // 默认对象大小
      }
    }

    return 64; // 默认值
  }

  private evictForMemory(): void {
    // 使用LRU顺序驱逐（复用父类逻辑）
    const keys = this.keys();
    if (keys.length > 0) {
      this.delete(keys[0]);
    }
  }
}