import { HashUtils } from './HashUtils';
import { NodeIdGenerator } from '../deterministic-node-id';

/**
 * 内容哈希工具类
 * 用于中等敏感度场景，如内容比较、节点ID生成等
 */
export class ContentHashUtils {
  /**
   * 生成内容哈希 - 用于中等敏感度场景
   * 用于内容比较和去重，使用更好的分布性算法
   */
  static generateContentHash(content: string): string {
    // 使用FNV-1a hash算法
    return HashUtils.fnv1aHash(content);
  }

  /**
   * 生成节点ID - 使用中央ID生成服务
   * 用于节点ID生成，统一ID格式
   */
  static generateNodeId(input: string, type: string = 'node'): string {
    // 使用中央ID生成服务
    return NodeIdGenerator.forSymbol(input, type, 'content_hash', 0);
  }
}