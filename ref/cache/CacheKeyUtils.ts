import { HashUtils } from './HashUtils';

/**
 * 缓存键生成工具类
 * 用于低敏感度场景，如缓存键生成、临时标识符等
 */
export class CacheKeyUtils {
  /**
   * 生成缓存键 - 用于低敏感度场景
   * 用于缓存系统，统一处理输入标准化
   */
  static generateCacheKey(...inputs: any[]): string {
    // 标准化输入
    const normalized = inputs.map(input => {
      if (typeof input === 'object') {
        return JSON.stringify(input, Object.keys(input).sort());
      }
      return String(input);
    }).join('|');

    // 使用简单hash算法
    return HashUtils.simpleHash(normalized);
  }
}