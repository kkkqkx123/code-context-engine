import { injectable, inject } from 'inversify';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService, ErrorReport } from '../../utils/ErrorHandlerService';
import { TYPES } from '../../types';
import { HotReloadErrorReport } from './types/HotReloadTypes';
import * as fs from 'fs/promises';
import * as path from 'path';

export interface ErrorPersistenceConfig {
  enabled: boolean;
  storagePath: string;
 maxFileSize: number; // in bytes
 maxFiles: number;
  flushInterval: number; // milliseconds
  enableCompression: boolean;
}

@injectable()
export class HotReloadErrorPersistenceService {
  private config: ErrorPersistenceConfig;
  private errorQueue: HotReloadErrorReport[] = [];
  private flushTimer: NodeJS.Timeout | null = null;
  private isProcessing: boolean = false;
  private readonly errorLogFileName: string = 'hotreload-errors.jsonl';
  private readonly errorArchivePattern: string = 'hotreload-errors-{date}.jsonl';

  constructor(
    @inject(TYPES.LoggerService) private logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) private errorHandler: ErrorHandlerService
  ) {
    this.config = {
      enabled: true,
      storagePath: './logs',
      maxFileSize: 512000, // 500KB
      maxFiles: 10,
      flushInterval: 5000, // 5 seconds
      enableCompression: false
    };

    this.startFlushTimer();
  }

  /**
   * 更新配置
   */
  async updateConfig(config: Partial<ErrorPersistenceConfig>): Promise<void> {
    this.config = { ...this.config, ...config };
    
    // 确保存储路径存在
    await this.ensureStoragePath();
    
    this.logger.info('Error persistence configuration updated', { config: this.config });
  }

 /**
   * 确保存储路径存在
   */
  private async ensureStoragePath(): Promise<void> {
    if (!this.config.enabled) {
      return;
    }

    try {
      await fs.mkdir(this.config.storagePath, { recursive: true });
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to create error log directory: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'HotReloadErrorPersistenceService', operation: 'ensureStoragePath', path: this.config.storagePath }
      );
    }
  }

  /**
   * 开始刷新计时器
   */
  private startFlushTimer(): void {
    if (this.flushTimer) {
      clearInterval(this.flushTimer);
    }

    this.flushTimer = setInterval(() => {
      if (this.config.enabled && this.errorQueue.length > 0) {
        this.flushErrors();
      }
    }, this.config.flushInterval);
  }

  /**
   * 添加错误到队列
   */
  async queueError(errorReport: HotReloadErrorReport): Promise<void> {
    if (!this.config.enabled) {
      return;
    }

    try {
      this.errorQueue.push(errorReport);

      // 如果队列大小超过阈值，立即刷新
      if (this.errorQueue.length >= 100) {
        await this.flushErrors();
      }
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to queue error for persistence: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'HotReloadErrorPersistenceService', operation: 'queueError', errorId: errorReport.id }
      );
    }
  }

  /**
   * 立即添加并持久化错误
   */
  async persistError(errorReport: HotReloadErrorReport): Promise<void> {
    if (!this.config.enabled) {
      this.errorHandler.handleError(
        new Error('Cannot persist error: persistence is disabled'),
        { component: 'HotReloadErrorPersistenceService', operation: 'persistError', errorId: errorReport.id }
      );
      return;
    }

    try {
      // 立即添加到队列并刷新
      this.errorQueue.push(errorReport);
      await this.flushErrors();
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to persist error immediately: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'HotReloadErrorPersistenceService', operation: 'persistError', errorId: errorReport.id }
      );
    }
  }

  /**
   * 将错误刷新到持久化存储
   */
  private async flushErrors(): Promise<void> {
    if (this.isProcessing || this.errorQueue.length === 0 || !this.config.enabled) {
      return;
    }

    this.isProcessing = true;

    try {
      const errorsToFlush = [...this.errorQueue];
      this.errorQueue = []; // 清空队列

      if (errorsToFlush.length === 0) {
        return;
      }

      // 确保存储路径存在
      this.ensureStoragePath();

      // 写入错误日志文件
      const logFilePath = path.join(this.config.storagePath, this.errorLogFileName);
      
      // 将错误报告转换为JSONL格式并追加到文件
      const jsonlContent = errorsToFlush.map(error => JSON.stringify(error)).join('\n') + '\n';
      await fs.appendFile(logFilePath, jsonlContent, 'utf-8');

      // 检查文件大小并执行轮转（如果需要）
      await this.rotateLogFileIfNeeded(logFilePath);

      this.logger.debug(`Flushed ${errorsToFlush.length} errors to ${logFilePath}`);
    } catch (error) {
      // 如果刷新失败，将错误放回队列顶部
      this.errorQueue = [...this.errorQueue]; // 保留当前队列
      this.errorHandler.handleError(
        new Error(`Failed to flush errors to persistence: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'HotReloadErrorPersistenceService', operation: 'flushErrors' }
      );
    } finally {
      this.isProcessing = false;
    }
  }

  /**
   * 检查并轮转日志文件（如果需要）
   */
  private async rotateLogFileIfNeeded(logFilePath: string): Promise<void> {
    try {
      const stats = await fs.stat(logFilePath);
      if (stats.size > this.config.maxFileSize) {
        // 生成带时间戳的归档文件名
        const timestamp = new Date().toISOString().replace(/[:.]/g, '-').replace('T', '_').replace('Z', '');
        const archiveFileName = this.errorArchivePattern.replace('{date}', timestamp);
        const archivePath = path.join(this.config.storagePath, archiveFileName);

        // 重命名当前日志文件为归档文件
        await fs.rename(logFilePath, archivePath);

        this.logger.info(`Log file rotated: ${logFilePath} -> ${archivePath}`);

        // 清理旧的归档文件（保留最新的maxFiles个）
        await this.cleanupOldArchives();
      }
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to rotate log file: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'HotReloadErrorPersistenceService', operation: 'rotateLogFileIfNeeded', logFilePath }
      );
    }
  }

  /**
   * 清理旧的归档文件
   */
  private async cleanupOldArchives(): Promise<void> {
    try {
      const files = await fs.readdir(this.config.storagePath);
      const archiveFiles = files
        .filter(file => file.startsWith('hotreload-errors-') && file.endsWith('.jsonl'))
        .sort(); // 按名称排序（时间戳格式确保按时间顺序）

      // 删除超出maxFiles限制的最旧文件
      const filesToDelete = archiveFiles.length > this.config.maxFiles
        ? archiveFiles.slice(0, archiveFiles.length - this.config.maxFiles)
        : [];
      
      for (const fileToDelete of filesToDelete) {
        const filePath = path.join(this.config.storagePath, fileToDelete);
        await fs.unlink(filePath);
        this.logger.debug(`Deleted old error archive: ${filePath}`);
      }
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to cleanup old error archives: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'HotReloadErrorPersistenceService', operation: 'cleanupOldArchives' }
      );
    }
  }

  /**
   * 读取错误日志
   */
  async readErrorLogs(limit?: number): Promise<HotReloadErrorReport[]> {
    if (!this.config.enabled) {
      return [];
    }

    try {
      const logFilePath = path.join(this.config.storagePath, this.errorLogFileName);
      let content = '';

      try {
        content = await fs.readFile(logFilePath, 'utf-8');
      } catch (error) {
        if ((error as any).code === 'ENOENT') {
          // 文件不存在，尝试从归档中读取
          return await this.readFromArchives(limit);
        }
        throw error;
      }

      // 解析JSONL内容
      const lines = content.trim().split('\n').filter(line => line.trim() !== '');
      const errors = lines.map(line => JSON.parse(line) as HotReloadErrorReport);

      // 如果指定了限制，只返回最新的错误
      return limit ? errors.slice(-limit) : errors;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to read error logs: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'HotReloadErrorPersistenceService', operation: 'readErrorLogs' }
      );
      return [];
    }
  }

  /**
   * 从归档文件读取错误日志
   */
  private async readFromArchives(limit?: number): Promise<HotReloadErrorReport[]> {
    try {
      const files = await fs.readdir(this.config.storagePath);
      const archiveFiles = files
        .filter(file => file.startsWith('hotreload-errors-') && file.endsWith('.jsonl'))
        .sort()
        .reverse(); // 最新的在前

      const allErrors: HotReloadErrorReport[] = [];

      for (const archiveFile of archiveFiles) {
        const archivePath = path.join(this.config.storagePath, archiveFile);
        try {
          const content = await fs.readFile(archivePath, 'utf-8');
          const lines = content.trim().split('\n').filter(line => line.trim() !== '');
          const errors = lines.map(line => JSON.parse(line) as HotReloadErrorReport);
          allErrors.push(...errors);

          // 如果已达到限制，停止读取更多文件
          if (limit && allErrors.length >= limit) {
            break;
          }
        } catch (error) {
          this.logger.warn(`Failed to read archive file ${archiveFile}:`, error);
          continue;
        }
      }

      // 如果指定了限制，只返回最新的错误
      return limit ? allErrors.slice(-limit) : allErrors;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to read error archives: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'HotReloadErrorPersistenceService', operation: 'readFromArchives' }
      );
      return [];
    }
  }

  /**
   * 获取错误统计
   */
  async getErrorStats(): Promise<{
    totalErrors: number;
    errorTypes: { [key: string]: number };
    lastErrorTime: Date | null;
    storageSize: number;
  }> {
    try {
      let totalErrors = 0;
      const errorTypes: { [key: string]: number } = {};
      let lastErrorTime: Date | null = null;
      let storageSize = 0;

      // 检查当前日志文件
      const logFilePath = path.join(this.config.storagePath, this.errorLogFileName);
      try {
        const stats = await fs.stat(logFilePath);
        storageSize += stats.size;

        const content = await fs.readFile(logFilePath, 'utf-8');
        const lines = content.trim().split('\n').filter(line => line.trim() !== '');
        
        for (const line of lines) {
          try {
            const error = JSON.parse(line) as HotReloadErrorReport;
            totalErrors++;
            
            // 统计错误类型
            errorTypes[error.errorCode] = (errorTypes[error.errorCode] || 0) + 1;
            
            // 更新最后错误时间
            const errorTime = new Date(error.timestamp);
            if (!lastErrorTime || errorTime > lastErrorTime) {
              lastErrorTime = errorTime;
            }
          } catch (parseError) {
            this.logger.warn(`Failed to parse error log line:`, parseError);
          }
        }
      } catch (error) {
        if ((error as any).code !== 'ENOENT') {
          throw error;
        }
      }

      // 检查归档文件
      const files = await fs.readdir(this.config.storagePath);
      const archiveFiles = files
        .filter(file => file.startsWith('hotreload-errors-') && file.endsWith('.jsonl'));

      for (const archiveFile of archiveFiles) {
        const archivePath = path.join(this.config.storagePath, archiveFile);
        const stats = await fs.stat(archivePath);
        storageSize += stats.size;

        const content = await fs.readFile(archivePath, 'utf-8');
        const lines = content.trim().split('\n').filter(line => line.trim() !== '');
        
        for (const line of lines) {
          try {
            const error = JSON.parse(line) as HotReloadErrorReport;
            totalErrors++;
            
            // 统计错误类型
            errorTypes[error.errorCode] = (errorTypes[error.errorCode] || 0) + 1;
            
            // 更新最后错误时间
            const errorTime = new Date(error.timestamp);
            if (!lastErrorTime || errorTime > lastErrorTime) {
              lastErrorTime = errorTime;
            }
          } catch (parseError) {
            this.logger.warn(`Failed to parse archive error line:`, parseError);
          }
        }
      }

      return {
        totalErrors,
        errorTypes,
        lastErrorTime,
        storageSize
      };
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to get error stats: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'HotReloadErrorPersistenceService', operation: 'getErrorStats' }
      );
      
      return {
        totalErrors: 0,
        errorTypes: {},
        lastErrorTime: null,
        storageSize: 0
      };
    }
 }

  /**
   * 清除错误日志
   */
  async clearErrorLogs(): Promise<void> {
    try {
      const logFilePath = path.join(this.config.storagePath, this.errorLogFileName);
      
      // 删除当前日志文件
      try {
        await fs.unlink(logFilePath);
      } catch (error) {
        if ((error as any).code !== 'ENOENT') {
          throw error;
        }
      }

      // 删除归档文件
      const files = await fs.readdir(this.config.storagePath);
      const archiveFiles = files
        .filter(file => file.startsWith('hotreload-errors-') && file.endsWith('.jsonl'));

      for (const archiveFile of archiveFiles) {
        const archivePath = path.join(this.config.storagePath, archiveFile);
        await fs.unlink(archivePath);
      }

      this.logger.info('Error logs cleared');
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to clear error logs: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'HotReloadErrorPersistenceService', operation: 'clearErrorLogs' }
      );
    }
  }

  /**
   * 销毁服务
   */
  async destroy(): Promise<void> {
    // 停止刷新计时器
    if (this.flushTimer) {
      clearInterval(this.flushTimer);
      this.flushTimer = null;
    }

    // 刷新剩余的错误
    if (this.errorQueue.length > 0) {
      await this.flushErrors();
    }

    this.logger.info('HotReloadErrorPersistenceService destroyed');
  }
}