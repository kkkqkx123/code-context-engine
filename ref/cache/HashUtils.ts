import * as crypto from 'crypto';
import * as fs from 'fs/promises';
import * as path from 'path';

export interface FileHash {
  path: string;
  hash: string;
  size: number;
  lastModified: Date;
}

export interface DirectoryHash {
  path: string;
  hash: string;
  fileCount: number;
  files: FileHash[];
}

export class HashUtils {
  static async calculateFileHash(filePath: string): Promise<string> {
    try {
      const fileBuffer = await fs.readFile(filePath);
      return crypto.createHash('sha256').update(fileBuffer).digest('hex');
    } catch (error) {
      throw new Error(`Failed to calculate hash for ${filePath}: ${error}`);
    }
  }

  static async calculateDirectoryHash(dirPath: string): Promise<DirectoryHash> {
    const files: FileHash[] = [];
    const hash = crypto.createHash('sha256');

    const processFile = async (filePath: string) => {
      try {
        const stats = await fs.stat(filePath);
        const fileHash = await this.calculateFileHash(filePath);

        const fileHashInfo: FileHash = {
          path: path.relative(dirPath, filePath),
          hash: fileHash,
          size: stats.size,
          lastModified: stats.mtime,
        };

        files.push(fileHashInfo);
        hash.update(fileHashInfo.path + fileHashInfo.hash);
      } catch (error) {
        // Skip files that can't be read
      }
    };

    const processDirectory = async (currentPath: string) => {
      const entries = await fs.readdir(currentPath, { withFileTypes: true });

      for (const entry of entries) {
        const fullPath = path.join(currentPath, entry.name);

        if (entry.isDirectory()) {
          await processDirectory(fullPath);
        } else if (entry.isFile()) {
          await processFile(fullPath);
        }
      }
    };

    await processDirectory(dirPath);

    return {
      path: dirPath,
      hash: hash.digest('hex'),
      fileCount: files.length,
      files,
    };
  }

  static calculateStringHash(content: string): string {
    return crypto.createHash('sha256').update(content).digest('hex');
  }

  static generateId(prefix: string = 'id'): string {
    const timestamp = Date.now().toString(36);
    const random = Math.random().toString(36).substr(2, 9);
    return `${prefix}_${timestamp}_${random}`;
  }

  static normalizePath(filePath: string): string {
    // 标准化路径，转换反斜杠为正斜杠，并移除尾斜杠（除非是根路径）
    let normalized = path.normalize(filePath).replace(/\\/g, '/');

    // 移除尾斜杠，但保留根路径的斜杠
    if (normalized.length > 1 && normalized.endsWith('/')) {
      normalized = normalized.slice(0, -1);
    }

    // 处理双斜杠问题（Windows UNC路径）
    if (normalized.startsWith('//') && !normalized.startsWith('///')) {
      normalized = '/' + normalized.substring(2);
    }

    return normalized;
  }

  /**
   * 深度标准化路径，处理更多边界情况
   */
  static deepNormalizePath(filePath: string): string {
    if (!filePath) return '';

    // 1. 标准化路径分隔符
    let normalized = filePath.replace(/\\/g, '/');

    // 2. 处理相对路径部分
    normalized = path.normalize(normalized);

    // 3. 移除尾部斜杠（除非是根路径）
    if (normalized.length > 1 && normalized.endsWith('/')) {
      normalized = normalized.slice(0, -1);
    }

    // 4. 处理Windows UNC路径
    if (normalized.startsWith('//') && !normalized.startsWith('///')) {
      normalized = '/' + normalized.substring(2);
    }

    // 5. 移除多余的斜杠
    normalized = normalized.replace(/\/+/g, '/');

    // 6. 处理当前目录引用
    normalized = normalized.replace(/\/\.\//g, '/');

    // 7. 移除开头的"./"
    if (normalized.startsWith('./')) {
      normalized = normalized.substring(2);
    }

    return normalized;
  }

  /**
   * 比较两个路径是否指向同一个位置
   */
  static arePathsEqual(path1: string, path2: string): boolean {
    if (!path1 || !path2) return path1 === path2;

    // 使用深度标准化进行比较
    return this.deepNormalizePath(path1) === this.deepNormalizePath(path2);
  }

  /**
   * 生成符合数据库命名规范的项目名称
   * 将项目路径转换为安全的名称，只包含字母、数字、连字符和下划线
   * 
   * @param projectId 项目ID或路径
   * @param prefix 名称前缀，默认为'project'
   * @param maxLength 最大长度，默认为63（符合大多数数据库命名规范）
   * @param saveMapping 是否保存映射关系到数据库，默认为false
   * @param mappingService 映射服务实例，当saveMapping为true时需要提供
   * @returns 符合命名规范的安全名称
   */
  static generateSafeProjectName(
    projectId: string,
    prefix: string = 'project',
    maxLength: number = 63,
    saveMapping: boolean = false,
    mappingService?: any
  ): string {
    try {
      // 1. 首先尝试直接使用项目ID（如果它已经符合规范）
      const directPattern = /^[a-zA-Z0-9_-]{1,63}$/;
      if (directPattern.test(projectId) && !projectId.startsWith('_')) {
        // 如果需要保存映射关系
        if (saveMapping && mappingService) {
          mappingService.saveMapping(projectId, projectId).catch((error: any) => {
            // 记录错误但不中断主流程
            console.error('Failed to save project path mapping:', error);
          });
        }
        return `${prefix}-${projectId}`;
      }

      // 2. 如果不符合规范，使用哈希值
      const hash = crypto.createHash('sha256').update(projectId).digest('hex');

      // 3. 提取哈希的前部分，确保总长度不超过限制
      const prefixLength = prefix.length + 1; // +1 for the hyphen
      const maxHashLength = maxLength - prefixLength;
      const shortHash = hash.substring(0, Math.min(maxHashLength, 16)); // 使用最多16位哈希

      const safeName = `${prefix}-${shortHash}`;

      // 4. 保存映射关系（如果需要）
      if (saveMapping && mappingService) {
        mappingService.saveMapping(shortHash, projectId).catch((error: any) => {
          // 记录错误但不中断主流程
          console.error('Failed to save project path mapping:', error);
        });
      }

      return safeName;
    } catch (error) {
      // 如果出现任何错误，返回一个基于时间戳的安全名称
      const timestamp = Date.now().toString(36);
      return `${prefix}-${timestamp}`;
    }
  }

  static getFileExtension(filePath: string): string {
    const ext = path.extname(filePath).toLowerCase();
    return ext.startsWith('.') ? ext.substr(1) : ext;
  }

  /**
   * 简单hash实现 - 用于低敏感度场景
   * 算法: hash = ((hash << 5) - hash) + char
   */
  static simpleHash(str: string): string {
    let hash = 0;
    for (let i = 0; i < str.length; i++) {
      const char = str.charCodeAt(i);
      hash = ((hash << 5) - hash) + char;
      hash = hash >>> 0; // 确保为无符号32位整数
    }
    return Math.abs(hash).toString(36);
  }

  /**
   * FNV-1a hash实现 - 用于中等敏感度场景
   * 算法: hash ^= char; hash *= FNV_PRIME
   */
  static fnv1aHash(str: string): string {
    const FNV_PRIME = 16777619;
    const FNV_OFFSET_BASIS = 2166136261;

    let hash = FNV_OFFSET_BASIS;
    for (let i = 0; i < str.length; i++) {
      hash ^= str.charCodeAt(i);
      hash *= FNV_PRIME;
      hash = hash >>> 0; // 转为无符号32位整数
    }
    return hash === FNV_OFFSET_BASIS ? '0' : Math.abs(hash).toString(36);
  }
}