import { injectable, inject } from 'inversify';
import { TYPES } from '../../types';
import { LoggerService } from '../../utils/LoggerService';
import { SqliteDatabaseService } from '../../database/splite/SqliteDatabaseService';
import { EventEmitter } from 'events';
import { LRUCache, StatsDecorator } from '../../utils/cache';
import { DetailedStats } from '../../utils/cache/StatsDecorator';

export interface FileHashEntry {
  projectId: string;
  filePath: string;
  hash: string;
  lastModified: Date;
  fileSize: number;
  language?: string;
  fileType?: string;
  createdAt: Date;
  updatedAt: Date;
}

export interface FileHashManager {
  getFileHash(projectId: string, filePath: string): Promise<string | null>;
  updateFileHash(projectId: string, filePath: string, hash: string, metadata?: Partial<FileHashEntry>): Promise<void>;
  getFileHashes(projectId: string, filePaths: string[]): Promise<Map<string, string>>;
  batchUpdateHashes(updates: Array<{projectId: string, filePath: string, hash: string, fileSize?: number, lastModified?: Date, language?: string, fileType?: string}>): Promise<void>;
  deleteFileHash(projectId: string, filePath: string): Promise<void>;
  renameFile(projectId: string, oldPath: string, newPath: string): Promise<void>;
  getChangedFiles(projectId: string, since: Date): Promise<FileHashEntry[]>;
  cleanupExpiredHashes(expiryDays?: number): Promise<number>;
}

@injectable()
export class FileHashManagerImpl extends EventEmitter implements FileHashManager {
  private cache: StatsDecorator<string, FileHashEntry>;
  private sqliteService: SqliteDatabaseService;
  private logger: LoggerService;

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.SqliteDatabaseService) sqliteService: SqliteDatabaseService
  ) {
    super();
    this.logger = logger;
    this.sqliteService = sqliteService;
    
    // 创建基础LRUCache
    const baseCache = new LRUCache<string, FileHashEntry>(10000, {
      enableStats: true,
      defaultTTL: 5 * 60 * 1000 // 5分钟TTL，与原有逻辑一致
    });
    
    // 使用StatsDecorator包装以获得详细统计功能
    this.cache = new StatsDecorator(baseCache);
  }

  /**
   * 获取文件哈希（优先从缓存，然后从数据库）
   */
  async getFileHash(projectId: string, filePath: string): Promise<string | null> {
    const cacheKey = `${projectId}:${filePath}`;
    
    // 1. 检查内存缓存
    const cached = this.cache.get(cacheKey);
    if (cached) {
      return cached.hash;
    }
    
    // 2. 从数据库查询
    try {
      const stmt = this.sqliteService.prepare(`
        SELECT content_hash, last_modified, file_size, language, file_type
        FROM file_index_states 
        WHERE project_id = ? AND file_path = ?
        ORDER BY updated_at DESC 
        LIMIT 1
      `);
      
      const result = stmt.get(projectId, filePath) as any;
      
      if (result) {
        // 更新缓存
        const entry: FileHashEntry = {
          projectId,
          filePath,
          hash: result.content_hash,
          lastModified: new Date(result.last_modified),
          fileSize: result.file_size || 0,
          language: result.language,
          fileType: result.file_type,
          createdAt: new Date(),
          updatedAt: new Date()
        };
        
        this.cache.set(cacheKey, entry);
        return result.content_hash;
      }
      
      return null;
    } catch (error) {
      this.logger.error(`Failed to get file hash from database: ${filePath}`, error);
      return null;
    }
  }

  /**
   * 更新文件哈希
   */
  async updateFileHash(projectId: string, filePath: string, hash: string, metadata?: Partial<FileHashEntry>): Promise<void> {
    const cacheKey = `${projectId}:${filePath}`;
    const now = new Date();
    
    // 1. 更新内存缓存
    const entry: FileHashEntry = {
      projectId,
      filePath,
      hash,
      lastModified: metadata?.lastModified || now,
      fileSize: metadata?.fileSize || 0,
      language: metadata?.language,
      fileType: metadata?.fileType,
      createdAt: metadata?.createdAt || now,
      updatedAt: now
    };
    
    this.cache.set(cacheKey, entry);
    
    // 2. 异步更新数据库
    try {
      const stmt = this.sqliteService.prepare(`
        INSERT OR REPLACE INTO file_index_states 
        (project_id, file_path, relative_path, content_hash, file_size, last_modified, language, file_type, status, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
      `);
      
      const relativePath = this.extractRelativePath(filePath);
      
      stmt.run(
        projectId,
        filePath,
        relativePath,
        hash,
        entry.fileSize,
        entry.lastModified.toISOString(),
        entry.language || null,
        entry.fileType || null,
        'indexed',
        entry.createdAt.toISOString(),
        entry.updatedAt.toISOString()
      );
      
      this.emit('hashUpdated', { projectId, filePath, hash });
    } catch (error) {
      this.logger.error(`Failed to update file hash in database: ${filePath}`, error);
      throw error;
    }
  }

  /**
   * 批量获取文件哈希
   */
  async getFileHashes(projectId: string, filePaths: string[]): Promise<Map<string, string>> {
    const result = new Map<string, string>();
    
    // 分批处理，避免内存溢出
    for (let i = 0; i < filePaths.length; i += 100) {
      const batch = filePaths.slice(i, i + 100);
      const batchHashes = await this.getBatchHashes(projectId, batch);
      batchHashes.forEach((hash, path) => result.set(path, hash));
    }
    
    return result;
  }

  /**
   * 批量更新哈希
   */
  async batchUpdateHashes(updates: Array<{projectId: string, filePath: string, hash: string, fileSize?: number, lastModified?: Date, language?: string, fileType?: string}>): Promise<void> {
    if (updates.length === 0) return;
    
    try {
      // 使用事务进行批量操作
      this.sqliteService.transaction(() => {
        // 首先确保所有涉及的项目都存在于projects表中
        const uniqueProjectIds = [...new Set(updates.map(update => update.projectId))];
        for (const projectId of uniqueProjectIds) {
          // 检查项目是否已存在
          const checkStmt = this.sqliteService.prepare('SELECT id FROM projects WHERE id = ?');
          const existingProject = checkStmt.get(projectId);
          
          if (!existingProject) {
            // 如果项目不存在，创建一个最小的项目记录以满足外键约束
            const now = new Date().toISOString();
            const insertProjectStmt = this.sqliteService.prepare(`
              INSERT OR IGNORE INTO projects
              (id, path, name, description, collection_name, space_name, created_at, updated_at, last_indexed_at, status, settings, metadata)
              VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            `);
            // 使用一个默认路径，实际路径应该在其他地方正确设置
            insertProjectStmt.run(
              projectId,
              `unknown_path_for_${projectId}`,
              `Project ${projectId}`,
              null,  // description
              null,  // collection_name
              null,  // space_name
              now,   // created_at
              now,   // updated_at
              null,  // last_indexed_at
              'active',  // status
              '{}',  // settings (JSON)
              '{}'   // metadata (JSON)
            );
          }
        }
        
        const stmt = this.sqliteService.prepare(`
          INSERT OR REPLACE INTO file_index_states
          (project_id, file_path, relative_path, content_hash, file_size, last_modified, language, file_type, status, created_at, updated_at)
          VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        `);
        
        for (const update of updates) {
          const relativePath = this.extractRelativePath(update.filePath);
          const now = new Date();
          
          stmt.run(
            update.projectId,
            update.filePath,
            relativePath,
            update.hash,
            update.fileSize || 0,
            (update.lastModified || now).toISOString(),
            update.language || null,
            update.fileType || null,
            'indexed',
            now.toISOString(),
            now.toISOString()
          );
          
          // 更新内存缓存
          const cacheKey = `${update.projectId}:${update.filePath}`;
          const entry: FileHashEntry = {
            projectId: update.projectId,
            filePath: update.filePath,
            hash: update.hash,
            lastModified: update.lastModified || now,
            fileSize: update.fileSize || 0,
            language: update.language,
            fileType: update.fileType,
            createdAt: now,
            updatedAt: now
          };
          
          this.cache.set(cacheKey, entry);
        }
      });
      
      this.emit('hashesUpdated', { count: updates.length });
      this.logger.info(`Batch updated ${updates.length} file hashes`);
    } catch (error) {
      this.logger.error('Failed to batch update file hashes', error);
      throw error;
    }
  }

  /**
   * 删除文件哈希
   */
  async deleteFileHash(projectId: string, filePath: string): Promise<void> {
    const cacheKey = `${projectId}:${filePath}`;
    
    // 从内存缓存中删除
    this.cache.delete(cacheKey);
    
    // 从数据库中删除
    try {
      const stmt = this.sqliteService.prepare(`
        DELETE FROM file_index_states
        WHERE project_id = ? AND file_path = ?
      `);
      
      stmt.run(projectId, filePath);
      this.emit('hashDeleted', { projectId, filePath });
    } catch (error) {
      this.logger.error(`Failed to delete file hash from database: ${filePath}`, error);
      throw error;
    }
  }

  /**
   * 重命名文件哈希
   */
  async renameFile(projectId: string, oldPath: string, newPath: string): Promise<void> {
    const oldCacheKey = `${projectId}:${oldPath}`;
    const newCacheKey = `${projectId}:${newPath}`;
    
    try {
      // 更新数据库中的文件路径
      const stmt = this.sqliteService.prepare(`
        UPDATE file_index_states
        SET file_path = ?, relative_path = ?, updated_at = CURRENT_TIMESTAMP
        WHERE project_id = ? AND file_path = ?
      `);
      
      const result = stmt.run(newPath, newPath, projectId, oldPath);
      
      if (result.changes === 0) {
        // 如果没有找到记录，可能是新文件，执行添加操作
        this.logger.warn(`No existing record found for rename operation: ${oldPath} -> ${newPath}`);
        return;
      }
      
      // 更新内存缓存
      const cachedEntry = this.cache.get(oldCacheKey);
      if (cachedEntry) {
        // 更新缓存条目
        const updatedEntry: FileHashEntry = {
          ...cachedEntry,
          filePath: newPath,
          updatedAt: new Date()
        };
        
        // 删除旧缓存条目
        this.cache.delete(oldCacheKey);
        
        // 添加新缓存条目
        this.cache.set(newCacheKey, updatedEntry);
      }
      
      this.emit('hashRenamed', { projectId, oldPath, newPath });
      this.logger.debug(`File hash renamed: ${oldPath} -> ${newPath}`);
    } catch (error) {
      this.logger.error(`Failed to rename file hash in database: ${oldPath} -> ${newPath}`, error);
      throw error;
    }
  }

  /**
   * 获取变更的文件
   */
  async getChangedFiles(projectId: string, since: Date): Promise<FileHashEntry[]> {
    try {
      const stmt = this.sqliteService.prepare(`
        SELECT project_id, file_path, content_hash, file_size, last_modified, language, file_type, created_at, updated_at
        FROM file_index_states 
        WHERE project_id = ? AND updated_at > ?
        ORDER BY updated_at DESC
      `);
      
      const results = stmt.all(projectId, since.toISOString()) as any[];
      
      return results.map(row => ({
        projectId: row.project_id,
        filePath: row.file_path,
        hash: row.content_hash,
        lastModified: new Date(row.last_modified),
        fileSize: row.file_size || 0,
        language: row.language,
        fileType: row.file_type,
        createdAt: new Date(row.created_at),
        updatedAt: new Date(row.updated_at)
      }));
    } catch (error) {
      this.logger.error(`Failed to get changed files for project: ${projectId}`, error);
      return [];
    }
  }

  /**
   * 清理过期的哈希
   */
  async cleanupExpiredHashes(expiryDays: number = 30): Promise<number> {
    const expiryDate = new Date();
    expiryDate.setDate(expiryDate.getDate() - expiryDays);
    
    try {
      const stmt = this.sqliteService.prepare(`
        DELETE FROM file_index_states 
        WHERE updated_at < ?
      `);
      
      const result = stmt.run(expiryDate.toISOString());
      const deletedCount = result.changes;
      
      // 清理内存缓存
      const underlyingCache = this.cache.getUnderlyingCache();
      underlyingCache.cleanup();
      
      this.logger.info(`Cleaned up ${deletedCount} expired file hashes`);
      return deletedCount;
    } catch (error) {
      this.logger.error('Failed to cleanup expired hashes', error);
      return 0;
    }
  }

  /**
   * 获取缓存统计信息
   */
  getCacheStats(): {
    cacheSize: number;
    cacheHitRate: number;
    memoryUsage: number;
  } {
    const stats = this.cache.getStats() as DetailedStats;
    return {
      cacheSize: stats.size,
      cacheHitRate: stats.hitRate * 100, // 转换为百分比
      memoryUsage: stats.memoryUsage
    };
  }

  /**
   * 重置缓存统计
   */
  resetCacheStats(): void {
    this.cache.resetStats();
  }

  /**
   * 批量获取哈希（内部方法）
   */
  private async getBatchHashes(projectId: string, filePaths: string[]): Promise<Map<string, string>> {
    const result = new Map<string, string>();
    
    if (filePaths.length === 0) return result;
    
    try {
      // 先检查缓存
      const uncachedFiles: string[] = [];
      
      for (const filePath of filePaths) {
        const cacheKey = `${projectId}:${filePath}`;
        const cached = this.cache.get(cacheKey);
        if (cached) {
          result.set(filePath, cached.hash);
        } else {
          uncachedFiles.push(filePath);
        }
      }
      
      // 对未缓存的文件进行数据库查询
      if (uncachedFiles.length > 0) {
        const placeholders = uncachedFiles.map(() => '?').join(',');
        const stmt = this.sqliteService.prepare(`
          SELECT file_path, content_hash
          FROM file_index_states 
          WHERE project_id = ? AND file_path IN (${placeholders})
        `);
        
        const queryResults = stmt.all(projectId, ...uncachedFiles) as any[];
        
        queryResults.forEach(row => {
          result.set(row.file_path, row.content_hash);
          
          // 将查询结果加入缓存
          const cacheKey = `${projectId}:${row.file_path}`;
          // 这里我们只有hash信息，需要构建一个基本的entry
          const entry: FileHashEntry = {
            projectId,
            filePath: row.file_path,
            hash: row.content_hash,
            lastModified: new Date(), // 使用当前时间作为默认值
            fileSize: 0,
            createdAt: new Date(),
            updatedAt: new Date()
          };
          this.cache.set(cacheKey, entry);
        });
      }
      
      return result;
    } catch (error) {
      this.logger.error(`Failed to get batch hashes for project: ${projectId}`, error);
      return result;
    }
  }

  /**
   * 提取相对路径
   */
  private extractRelativePath(filePath: string): string {
    // 简单的相对路径提取，可以根据需要调整
    const parts = filePath.split(/[/\\]/);
    return parts.slice(-3).join('/'); // 返回最后3级路径
  }
}