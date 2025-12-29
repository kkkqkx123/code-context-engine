import { injectable, inject, optional } from 'inversify';
import { EventEmitter } from 'events';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { HotReloadRecoveryService } from './HotReloadRecoveryService';
import { HotReloadError, HotReloadErrorCode } from './HotReloadError';
import { FileHashManager } from './FileHashManager';

// 定义错误上下文接口
interface ErrorContext {
  component: string;
  operation: string;
  metadata?: Record<string, any>;
}
import { FileWatcherService, FileWatcherCallbacks } from './FileWatcherService';
import { FileSystemTraversal, FileInfo } from './FileSystemTraversal';
import { FileUtils } from '../../utils/filesystem/FileUtils';
import { ProjectIdManager } from '../../database/ProjectIdManager';
import { TYPES } from '../../types';
import * as path from 'path';

export interface FileWatcherOptions {
  watchPaths: string[];
  ignored?: string[];
  ignoreInitial?: boolean;
  awaitWriteFinish?: boolean;
  awaitWriteFinishOptions?: {
    stabilityThreshold: number;
    pollInterval: number;
  };
  usePolling?: boolean;
  interval?: number;
  [key: string]: any;
}

export interface ChangeDetectionOptions {
  debounceInterval?: number;
  maxConcurrentOperations?: number;
  enableHashComparison?: boolean;
  trackFileHistory?: boolean;
  historySize?: number;
  enableDetailedLogging?: boolean;
  permissionRetryAttempts?: number;
  permissionRetryDelay?: number;
  maxFileSizeBytes?: number;
  excludedExtensions?: string[];
  excludedDirectories?: string[];
}

export interface FileChangeEvent {
  type: 'created' | 'modified' | 'deleted' | 'renamed';
  path: string;
  relativePath: string;
  previousHash?: string;
  currentHash?: string;
  timestamp: Date;
  size?: number;
  language?: string;
  oldPath?: string; // 改名前的路径
  oldRelativePath?: string; // 改名前的相对路径
}

export interface ChangeDetectionCallbacks {
  onFileCreated?: (event: FileChangeEvent) => void;
  onFileModified?: (event: FileChangeEvent) => void;
  onFileDeleted?: (event: FileChangeEvent) => void;
  onFileRenamed?: (event: FileChangeEvent) => void;
  onError?: (error: Error) => void;
}

export interface FileHistoryEntry {
  path: string;
  hash: string;
  timestamp: Date;
  size: number;
  language: string;
}

@injectable()
export class ChangeDetectionService extends EventEmitter {
  private logger: LoggerService;
  private errorHandler: ErrorHandlerService;
  private fileWatcherService: FileWatcherService;
  private fileSystemTraversal: FileSystemTraversal;
  private fileHashManager: FileHashManager;
  private fileHistory: Map<string, FileHistoryEntry[]> = new Map();
  private pendingChanges: Map<string, NodeJS.Timeout> = new Map();
  private changeAccumulator: Map<string, FileChangeEvent[]> = new Map();
  private batchTimers: Map<string, NodeJS.Timeout> = new Map();
  private isRunning: boolean = false;
  private testMode: boolean = false;
  private options: Required<ChangeDetectionOptions>;
  private callbacks: ChangeDetectionCallbacks = {};
  private stats = {
    filesProcessed: 0,
    changesDetected: 0,
    errorsEncountered: 0,
    permissionErrors: 0,
    averageProcessingTime: 0,
  };
  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.HotReloadRecoveryService) private hotReloadRecoveryService: HotReloadRecoveryService,
    @inject(TYPES.FileWatcherService) fileWatcherService: FileWatcherService,
    @inject(TYPES.FileSystemTraversal) fileSystemTraversal: FileSystemTraversal,
    @inject(TYPES.FileHashManager) fileHashManager: FileHashManager,
    @inject(TYPES.ProjectIdManager) private projectIdManager: ProjectIdManager,
    @inject('ChangeDetectionOptions') @optional() options?: ChangeDetectionOptions
  ) {
    super();
    this.logger = logger;
    this.errorHandler = errorHandler;
    this.fileWatcherService = fileWatcherService;
    this.fileSystemTraversal = fileSystemTraversal;
    this.fileHashManager = fileHashManager;

    // Detect test environment
    this.testMode = process.env.NODE_ENV === 'test' || process.env.JEST_WORKER_ID !== undefined;

    this.options = {
      debounceInterval: this.isTestMode() ? 100 : (options?.debounceInterval ?? 500),
      maxConcurrentOperations: options?.maxConcurrentOperations ?? 10,
      enableHashComparison: options?.enableHashComparison ?? true,
      trackFileHistory: options?.trackFileHistory ?? true,
      historySize: options?.historySize ?? 10,
      enableDetailedLogging: options?.enableDetailedLogging ?? false,
      permissionRetryAttempts: options?.permissionRetryAttempts ?? 3,
      permissionRetryDelay: this.isTestMode() ? 100 : (options?.permissionRetryDelay ?? 1000),
      maxFileSizeBytes: options?.maxFileSizeBytes ?? 512000, // 500KB
      excludedExtensions: options?.excludedExtensions ?? ['.log', '.tmp', '.bak'],
      excludedDirectories: options?.excludedDirectories ?? [
        'node_modules',
        '.git',
        'dist',
        'build',
      ],
    };

    if (this.isTestMode()) {
      this.logger.info('ChangeDetectionService running in test mode - using optimized settings');
    }
  }

  setCallbacks(callbacks: ChangeDetectionCallbacks): void {
    this.callbacks = { ...this.callbacks, ...callbacks };
  }

  async initialize(rootPaths: string[], watcherOptions?: FileWatcherOptions): Promise<void> {
    if (this.isRunning) {
      this.logger.warn('ChangeDetectionService is already initialized');
      return;
    }

    try {
      this.logger.info('Initializing ChangeDetectionService', { rootPaths, options: this.options });

      // 等待ProjectIdManager完成加载
      await this.waitForProjectIdManager();

      // Initialize file hashes for existing files
      await this.initializeFileHashes(rootPaths);

      // Set up file watcher callbacks
      const fileWatcherCallbacks: FileWatcherCallbacks = {
        onFileAdded: fileInfo => this.handleFileAdded(fileInfo),
        onFileChanged: fileInfo => this.handleFileChanged(fileInfo),
        onFileDeleted: filePath => this.handleFileDeleted(filePath),
        onFileRenamed: (oldPath, newPath, fileInfo) => this.handleFileRenamed(oldPath, newPath, fileInfo),
        onError: error => this.handleWatcherError(error),
        onReady: () => this.handleWatcherReady(),
      };

      this.fileWatcherService.setCallbacks(fileWatcherCallbacks);

      // Start watching files
      const watchOptions: FileWatcherOptions = {
        watchPaths: rootPaths,
        ignored: ['**/node_modules/**', '**/.git/**', '**/dist/**', '**/build/**'],
        ignoreInitial: true,
        awaitWriteFinish: true,
        awaitWriteFinishOptions: {
          stabilityThreshold: this.options.debounceInterval,
          pollInterval: this.isTestMode() ? 25 : 100,
        },
        // Test-specific optimizations
        usePolling: this.isTestMode(),
        interval: this.isTestMode() ? 50 : undefined,
        ...watcherOptions,
      };

      await this.fileWatcherService.startWatching(watchOptions);
      this.isRunning = true;

      this.logger.info('ChangeDetectionService initialized successfully');
    } catch (error) {
      const errorContext: ErrorContext = {
        component: 'ChangeDetectionService',
        operation: 'initialize',
        metadata: { rootPaths, options: this.options },
      };

      const report = this.errorHandler.handleError(
        error instanceof Error ? error : new Error(String(error)),
        errorContext
      );

      this.logger.error('Failed to initialize ChangeDetectionService', { errorId: report.id });
      throw error;
    }
  }

  private async handleInitializationError(error: any, rootPaths: string[]): Promise<void> {
    const errorContext: ErrorContext = {
      component: 'ChangeDetectionService',
      operation: 'initialize',
      metadata: { rootPaths, options: this.options },
    };

    // 创建热更新错误
    const hotReloadError = new HotReloadError(
      HotReloadErrorCode.CHANGE_DETECTION_FAILED,
      `Failed to initialize ChangeDetectionService: ${error instanceof Error ? error.message : String(error)}`,
      { rootPaths, options: this.options }
    );

    // 使用错误处理服务处理错误
    const report = this.errorHandler.handleHotReloadError(hotReloadError, errorContext);

    // 使用恢复服务处理错误
    await this.hotReloadRecoveryService.handleError(hotReloadError, errorContext);

    this.logger.error('Failed to initialize ChangeDetectionService', { errorId: report.id });
    throw hotReloadError;
  }

  private async initializeFileHashes(rootPaths: string[]): Promise<void> {
    try {
      const hashUpdates: Array<{ projectId: string, filePath: string, hash: string, fileSize?: number, lastModified?: Date, language?: string, fileType?: string }> = [];

      for (const rootPath of rootPaths) {
        const result = await this.fileSystemTraversal.traverseDirectory(rootPath);

        // 获取项目ID
        const projectId = await this.getProjectIdForPath(rootPath);

        for (const file of result.files) {
          hashUpdates.push({
            projectId,
            filePath: file.relativePath,
            hash: file.hash,
            fileSize: file.size,
            lastModified: file.lastModified,
            language: file.language,
            fileType: path.extname(file.path)
          });

          if (this.options.trackFileHistory) {
            this.addFileHistoryEntry(file);
          }
        }
      }

      // 批量更新文件哈希到数据库
      if (hashUpdates.length > 0) {
        await this.fileHashManager.batchUpdateHashes(hashUpdates);
      }

      this.logger.info(`Initialized file hashes for ${hashUpdates.length} files`);
    } catch (error) {
      const errorContext: ErrorContext = {
        component: 'ChangeDetectionService',
        operation: 'initializeFileHashes',
        metadata: { rootPaths },
      };

      this.errorHandler.handleError(
        error instanceof Error ? error : new Error(String(error)),
        errorContext
      );

      throw error;
    }
  }

  private addFileHistoryEntry(file: FileInfo): void {
    if (!this.options.trackFileHistory) return;

    const history = this.fileHistory.get(file.relativePath) || [];
    const entry: FileHistoryEntry = {
      path: file.path,
      hash: file.hash,
      timestamp: new Date(),
      size: file.size,
      language: file.language,
    };

    history.push(entry);

    // Keep only the most recent entries
    if (history.length > this.options.historySize) {
      history.shift();
    }

    this.fileHistory.set(file.relativePath, history);
  }

  private async handleFileAdded(fileInfo: FileInfo): Promise<void> {
    try {
      this.logger.debug(`File added: ${fileInfo.relativePath}`);

      // 获取实际的项目ID而不是硬编码的'default'
      const projectId = await this.getProjectIdForPath(fileInfo.path);
      const previousHash = await this.fileHashManager.getFileHash(projectId, fileInfo.relativePath);

      if (previousHash === null) {
        // 累积变更而不是立即处理
        this.accumulateChange(projectId, {
          type: 'created',
          path: fileInfo.path,
          relativePath: fileInfo.relativePath,
          currentHash: fileInfo.hash,
          timestamp: new Date(),
          size: fileInfo.size,
          language: fileInfo.language,
        });

        // 调度批量处理
        this.scheduleBatchProcessing(projectId);
      }
    } catch (error) {
      this.handleFileEventError('add', fileInfo.relativePath, error);
    }
  }

  private async handleFileChanged(fileInfo: FileInfo): Promise<void> {
    try {
      this.logger.debug(`File changed: ${fileInfo.relativePath}`);

      // 获取实际的项目ID而不是硬编码的'default'
      const projectId = await this.getProjectIdForPath(fileInfo.path);
      const previousHash = await this.fileHashManager.getFileHash(projectId, fileInfo.relativePath);

      if (previousHash === null) {
        // File not tracked yet, treat as new
        await this.handleFileAdded(fileInfo);
        return;
      }

      // 累积变更而不是立即处理
      this.accumulateChange(projectId, {
        type: 'modified',
        path: fileInfo.path,
        relativePath: fileInfo.relativePath,
        previousHash,
        currentHash: fileInfo.hash,
        timestamp: new Date(),
        size: fileInfo.size,
        language: fileInfo.language,
      });

      // 调度批量处理
      this.scheduleBatchProcessing(projectId);
    } catch (error) {
      this.handleFileEventError('change', fileInfo.relativePath, error);
    }
  }

  private async handleFileDeleted(filePath: string): Promise<void> {
    try {
      this.logger.debug(`File deleted: ${filePath}`);

      // Convert to relative path for consistency
      const relativePath = path.relative(process.cwd(), filePath);
      
      // 获取实际的项目ID而不是硬编码的'default'
      const projectId = await this.getProjectIdForPath(filePath);
      const previousHash = await this.fileHashManager.getFileHash(projectId, relativePath);

      if (previousHash !== null) {
        // 累积变更而不是立即处理
        this.accumulateChange(projectId, {
          type: 'deleted',
          path: filePath,
          relativePath,
          previousHash,
          timestamp: new Date(),
        });

        // 调度批量处理
        this.scheduleBatchProcessing(projectId);
      }
    } catch (error) {
      this.handleFileEventError('delete', filePath, error);
    }
  }

  private async handleFileRenamed(oldPath: string, newPath: string, fileInfo: FileInfo): Promise<void> {
    try {
      this.logger.debug(`File renamed: ${oldPath} -> ${newPath}`);

      // Convert to relative paths for consistency
      const relativeOldPath = path.relative(process.cwd(), oldPath);
      const relativeNewPath = path.relative(process.cwd(), newPath);
      
      // 获取实际的项目ID
      const projectId = await this.getProjectIdForPath(newPath);
      const previousHash = await this.fileHashManager.getFileHash(projectId, relativeOldPath);

      if (previousHash !== null) {
        // 更新文件哈希记录（保持哈希不变）
        await this.fileHashManager.renameFile(projectId, relativeOldPath, relativeNewPath);

        const event: FileChangeEvent = {
          type: 'renamed',
          path: newPath,
          relativePath: fileInfo.relativePath, // 使用fileInfo中的相对路径
          oldPath: relativeOldPath,
          oldRelativePath: relativeOldPath,
          previousHash,
          currentHash: fileInfo.hash,
          timestamp: new Date(),
          size: fileInfo.size,
          language: fileInfo.language,
        };

        this.emit('fileRenamed', event);

        if (this.callbacks.onFileRenamed) {
          try {
            this.callbacks.onFileRenamed(event);
          } catch (error) {
            this.logger.error('Error in onFileRenamed callback', error);
          }
        }
      } else {
        // 如果没有找到旧文件记录，将新文件作为添加处理
        await this.handleFileAdded(fileInfo);
      }
    } catch (error) {
      this.handleFileEventError('rename', `${oldPath} -> ${newPath}`, error);
    }
  }

  private handleWatcherError(error: Error): void {
    const errorContext: ErrorContext = {
      component: 'ChangeDetectionService',
      operation: 'watcher',
      metadata: {},
    };

    this.errorHandler.handleError(error, errorContext);
    this.logger.error('File watcher error', error);

    if (this.callbacks.onError) {
      try {
        this.callbacks.onError(error);
      } catch (callbackError) {
        this.logger.error('Error in onError callback', callbackError);
      }
    }
  }

  private handleWatcherReady(): void {
    this.logger.info('File watcher is ready');
  }

  private handleFileEventError(eventType: string, filePath: string, error: any): void {
    const errorContext: ErrorContext = {
      component: 'ChangeDetectionService',
      operation: `fileEvent:${eventType}`,
      metadata: { filePath },
    };

    this.errorHandler.handleError(
      error instanceof Error ? error : new Error(String(error)),
      errorContext
    );

    this.logger.error(`Error handling ${eventType} event for file ${filePath}`, error);
  }

  async stop(): Promise<void> {
    if (!this.isRunning) {
      this.logger.warn('ChangeDetectionService is not running');
      return;
    }

    try {
      this.logger.info('Stopping ChangeDetectionService');

      // Clear any pending changes
      for (const timeoutId of this.pendingChanges.values()) {
        clearTimeout(timeoutId);
      }
      this.pendingChanges.clear();

      // Clear batch processing state
      this.clearBatchProcessingState();

      // Stop the file watcher
      await this.fileWatcherService.stopWatching();

      this.isRunning = false;
      this.logger.info('ChangeDetectionService stopped successfully');
    } catch (error) {
      const errorContext: ErrorContext = {
        component: 'ChangeDetectionService',
        operation: 'stop',
      };

      this.errorHandler.handleError(
        error instanceof Error ? error : new Error(String(error)),
        errorContext
      );

      throw error;
    }
  }

  async getFileHash(relativePath: string): Promise<string | undefined> {
    // 需要完整路径来获取项目ID，这里使用当前工作目录作为基础路径
    const fullPath = path.resolve(process.cwd(), relativePath);
    const projectId = await this.getProjectIdForPath(fullPath);
    const hash = await this.fileHashManager.getFileHash(projectId, relativePath);
    return hash || undefined;
  }

  getFileHistory(relativePath: string): FileHistoryEntry[] {
    return this.fileHistory.get(relativePath) || [];
  }

  getAllFileHashes(): Map<string, string> {
    // 由于我们现在使用FileHashManager，这个方法需要重新实现
    // 暂时返回空Map，后续可以优化
    return new Map();
  }

  async isFileTracked(relativePath: string): Promise<boolean> {
    // 需要完整路径来获取项目ID，这里使用当前工作目录作为基础路径
    const fullPath = path.resolve(process.cwd(), relativePath);
    const projectId = await this.getProjectIdForPath(fullPath);
    const hash = await this.fileHashManager.getFileHash(projectId, relativePath);
    return hash !== null;
  }

  getTrackedFilesCount(): number {
    // 由于我们现在使用FileHashManager，这个方法需要重新实现
    // 暂时返回0，后续可以优化
    return 0;
  }

  isServiceRunning(): boolean {
    return this.isRunning;
  }

  /**
   * 等待ProjectIdManager完成加载
   */
  private async waitForProjectIdManager(): Promise<void> {
    // 等待ProjectIdManager完成加载和初始化
    // 由于ProjectIdManager依赖数据库服务，确保数据库连接已建立
    let attempts = 0;
    const maxAttempts = 50; // 最多等待5秒 (50 * 100ms)

    while (attempts < maxAttempts) {
      try {
        // 检查ProjectIdManager是否已准备好，可以通过尝试获取一个简单值来验证
        // 如果ProjectIdManager已完全初始化，它应该能够正常工作
        return; // 现在依赖内部的正确初始化，不需要特殊等待
      } catch (error) {
        // 继续等待
      }

      await new Promise(resolve => setTimeout(resolve, 100));
      attempts++;
    }

    // 如果等待后仍然未准备好，记录警告
    this.logger.warn('ProjectIdManager may not be fully ready after waiting, continuing initialization');
  }

  getStats() {
    return { ...this.stats };
  }

  resetStats(): void {
    this.stats = {
      filesProcessed: 0,
      changesDetected: 0,
      errorsEncountered: 0,
      permissionErrors: 0,
      averageProcessingTime: 0,
    };
  }

  // Test environment helper methods
  isTestMode(): boolean {
    return this.testMode;
  }

  async waitForFileProcessing(filePath: string, timeout: number = 3000): Promise<boolean> {
    if (!this.isTestMode()) {
      return true;
    }

    const startTime = Date.now();

    while (Date.now() - startTime < timeout) {
      // Check if file is being processed (has pending changes)
      if (!this.pendingChanges.has(filePath)) {
        return true;
      }

      // Wait before checking again
      await new Promise(resolve => setTimeout(resolve, 50));
    }

    return false;
  }

  async waitForAllProcessing(timeout: number = 5000): Promise<boolean> {
    if (!this.isTestMode()) {
      return true;
    }

    const startTime = Date.now();

    while (Date.now() - startTime < timeout) {
      // Check if all pending changes are processed
      if (this.pendingChanges.size === 0) {
        return true;
      }

      // Wait before checking again
      await new Promise(resolve => setTimeout(resolve, 100));
    }

    return false;
  }

  async flushPendingChanges(): Promise<void> {
    if (!this.isTestMode()) {
      return;
    }

    // Wait for all pending changes to be processed
    await this.waitForAllProcessing();

    // Additional wait to ensure stability
    await new Promise(resolve => setTimeout(resolve, 200));
  }

  /**
   * 获取项目ID（私有方法），用于测试
   */
  private async getProjectIdForPathInternal(rootPath: string): Promise<string> {
    // 使用ProjectIdManager来获取项目ID
    let projectId = this.projectIdManager.getProjectId(rootPath);

    // 如果项目ID不存在，创建一个新的
    if (!projectId) {
      projectId = await this.projectIdManager.generateProjectId(rootPath);
    }

    return projectId;
  }

  /**
   * 获取项目ID（公共方法）
   */
  async getProjectIdForPath(path: string): Promise<string> {
    return this.getProjectIdForPathInternal(path);
  }

  /**
   * 检测文件变化（用于手动更新）
   */
  async detectChangesForUpdate(projectPath: string, options?: { enableHashComparison?: boolean }): Promise<{
    added: string[];
    modified: string[];
    deleted: string[];
    unchanged: string[];
  }> {
    const enableHashComparison = options?.enableHashComparison ?? true;

    try {
      this.logger.info(`Detecting file changes for manual update: ${projectPath}`);

      // 1. 获取当前文件系统状态
      const currentFiles = await this.fileSystemTraversal.traverseDirectory(projectPath);
      const currentFilePaths = currentFiles.files.map(file => file.path);

      // 2. 获取已索引的文件列表
      const projectId = this.projectIdManager.getProjectId(projectPath);
      if (!projectId) {
        throw new Error(`Project ID not found for path: ${projectPath}`);
      }

      // 从IndexingLogicService获取已索引的文件列表
      // 这里我们需要通过依赖注入获取IndexingLogicService
      // 由于ChangeDetectionService没有直接注入IndexingLogicService，我们需要通过其他方式获取
      // 暂时使用一个简化的方法，通过FileHashManager获取已跟踪的文件
      const indexedFiles = await this.getIndexedFilesFromHashManager(projectId);

      // 3. 检测文件变化
      const changes = {
        added: [] as string[],
        modified: [] as string[],
        deleted: [] as string[],
        unchanged: [] as string[]
      };

      // 检测新增文件
      for (const file of currentFilePaths) {
        if (!indexedFiles.includes(file)) {
          changes.added.push(file);
        }
      }

      // 检测删除文件
      for (const file of indexedFiles) {
        if (!currentFilePaths.includes(file)) {
          changes.deleted.push(file);
        }
      }

      // 检测修改文件
      const existingFiles = currentFilePaths.filter(file => indexedFiles.includes(file));
      for (const file of existingFiles) {
        if (enableHashComparison) {
          const hasChanged = await this.hasFileChangedForUpdate(projectId, file);
          if (hasChanged) {
            changes.modified.push(file);
          } else {
            changes.unchanged.push(file);
          }
        } else {
          // 如果不启用哈希比较，则所有现有文件都视为已修改
          changes.modified.push(file);
        }
      }

      this.logger.info(`File changes detected for project ${projectPath}`, {
        added: changes.added.length,
        modified: changes.modified.length,
        deleted: changes.deleted.length,
        unchanged: changes.unchanged.length
      });

      return changes;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to detect file changes for update: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'ChangeDetectionService', operation: 'detectChangesForUpdate', projectPath }
      );
      throw error;
    }
  }

  /**
   * 检查文件是否发生变化（用于手动更新）
   */
  private async hasFileChangedForUpdate(projectId: string, filePath: string): Promise<boolean> {
    try {
      // 获取当前文件哈希
      const currentHash = await FileUtils.calculateFileHash(filePath);

      // 从FileHashManager获取缓存的哈希
      const cachedHash = await this.fileHashManager.getFileHash(projectId, filePath);

      if (cachedHash === null) {
        // 新文件，需要索引
        await this.fileHashManager.updateFileHash(projectId, filePath, currentHash);
        return true;
      }

      const hasChanged = currentHash !== cachedHash;
      if (hasChanged) {
        await this.fileHashManager.updateFileHash(projectId, filePath, currentHash);
      }

      return hasChanged;
    } catch (error) {
      this.logger.warn(`Failed to check file change for ${filePath}:`, error);
      // 如果无法计算哈希，则视为文件已变化
      return true;
    }
  }

  /**
   * 从FileHashManager获取已索引的文件列表
   */
  private async getIndexedFilesFromHashManager(projectId: string): Promise<string[]> {
    try {
      // 这里我们需要一个方法来获取所有已索引的文件
      // 由于FileHashManager没有直接提供这个方法，我们需要通过查询数据库获取
      // 暂时返回空数组，实际实现需要扩展FileHashManager
      // 通过类型断言访问sqliteService，这在实际实现中需要更好的方式
      const fileHashManagerImpl = this.fileHashManager as any;
      const stmt = fileHashManagerImpl.sqliteService?.prepare(`
        SELECT DISTINCT file_path
        FROM file_index_states 
        WHERE project_id = ? AND status = 'indexed'
      `);

      const results = stmt.all(projectId) as any[];
      return results.map(row => row.file_path);
    } catch (error) {
      this.logger.error(`Failed to get indexed files from hash manager: ${projectId}`, error);
      return [];
    }
  }

  /**
   * 累积文件变更
   */
  private accumulateChange(projectId: string, change: FileChangeEvent): void {
    if (!this.changeAccumulator.has(projectId)) {
      this.changeAccumulator.set(projectId, []);
    }
    this.changeAccumulator.get(projectId)!.push(change);
  }

  /**
   * 调度批量处理
   */
  private scheduleBatchProcessing(projectId: string): void {
    // 清除现有的批处理定时器
    if (this.batchTimers.has(projectId)) {
      clearTimeout(this.batchTimers.get(projectId)!);
    }

    const changes = this.changeAccumulator.get(projectId) || [];
    const dynamicDelay = this.calculateOptimalDelay(changes.length);

    const timer = setTimeout(async () => {
      await this.processBatchedChanges(projectId);
    }, dynamicDelay);

    this.batchTimers.set(projectId, timer);
  }

  /**
   * 计算最优延迟时间
   */
  private calculateOptimalDelay(changeCount: number): number {
    if (changeCount >= 50) {
      return 100; // 立即处理
    } else if (changeCount >= 20) {
      return 500; // 中等延迟
    } else if (changeCount >= 10) {
      return 1000; // 标准延迟
    } else {
      return this.options.debounceInterval; // 默认延迟
    }
  }

  /**
   * 处理批量变更
   */
  private async processBatchedChanges(projectId: string): Promise<void> {
    const changes = this.changeAccumulator.get(projectId) || [];
    if (changes.length === 0) return;

    // 清空累积器和定时器
    this.changeAccumulator.set(projectId, []);
    this.batchTimers.delete(projectId);

    this.logger.debug(`Processing batched changes for project ${projectId}`, {
      changeCount: changes.length
    });

    try {
      // 按变更类型分组
      const groupedChanges = this.groupChangesByType(changes);

      // 并发处理不同类型的变更
      await Promise.all([
        this.processFileChanges(groupedChanges.fileChanges),
        this.processIndexChanges(groupedChanges.indexChanges)
      ]);

      this.logger.debug(`Batch processing completed for project ${projectId}`, {
        fileChanges: groupedChanges.fileChanges.length,
        indexChanges: groupedChanges.indexChanges.length
      });
    } catch (error) {
      this.logger.error(`Error processing batched changes for project ${projectId}:`, error);
      // 重新加入队列进行重试
      for (const change of changes) {
        this.accumulateChange(projectId, change);
      }
      // 延迟重试
      setTimeout(() => this.scheduleBatchProcessing(projectId), 5000);
    }
  }

  /**
   * 按类型分组变更
   */
  private groupChangesByType(changes: FileChangeEvent[]): {
    fileChanges: FileChangeEvent[];
    indexChanges: FileChangeEvent[];
  } {
    return changes.reduce((groups: { fileChanges: FileChangeEvent[]; indexChanges: FileChangeEvent[] }, change) => {
      // 判断是否需要索引更新
      if (this.requiresIndexing(change)) {
        groups.indexChanges.push(change);
      } else {
        groups.fileChanges.push(change);
      }
      return groups;
    }, { fileChanges: [], indexChanges: [] });
  }

  /**
   * 判断变更是否需要索引更新
   */
  private requiresIndexing(change: FileChangeEvent): boolean {
    const indexedExtensions = ['.ts', '.js', '.tsx', '.jsx', '.py', '.java', '.cpp', '.c', '.h', '.hpp'];
    const extension = path.extname(change.path);
    return indexedExtensions.includes(extension);
  }

  /**
   * 处理文件变更（哈希更新等）
   */
  private async processFileChanges(changes: FileChangeEvent[]): Promise<void> {
    for (const change of changes) {
      try {
        const projectId = await this.getProjectIdForPath(change.path);

        switch (change.type) {
          case 'created':
          case 'modified':
            await this.fileHashManager.updateFileHash(projectId, change.relativePath, change.currentHash!, {
              fileSize: change.size,
              lastModified: change.timestamp,
              language: change.language,
              fileType: path.extname(change.path)
            });

            if (this.options.trackFileHistory && change.type === 'created') {
              this.addFileHistoryEntry({
                path: change.path,
                relativePath: change.relativePath,
                hash: change.currentHash!,
                size: change.size || 0,
                language: change.language || 'unknown',
                lastModified: change.timestamp,
                name: path.basename(change.path),
                extension: path.extname(change.path),
                isBinary: this.isLikelyBinaryFile(change.path)
              });
            }
            break;

          case 'deleted':
            await this.fileHashManager.deleteFileHash(projectId, change.relativePath);
            break;
        }

        // 发出事件
        this.emit(`file${change.type.charAt(0).toUpperCase() + change.type.slice(1)}`, change);

        // 调用回调
        const callbackKey = `onFile${change.type.charAt(0).toUpperCase() + change.type.slice(1)}` as keyof ChangeDetectionCallbacks;
        if (this.callbacks[callbackKey]) {
          try {
            (this.callbacks[callbackKey] as any)(change);
          } catch (error) {
            this.logger.error(`Error in ${callbackKey} callback`, error);
          }
        }
      } catch (error) {
        this.logger.error(`Error processing file change ${change.type} for ${change.path}:`, error);
      }
    }
  }

  /**
   * 处理索引变更（触发索引更新）
   */
  private async processIndexChanges(changes: FileChangeEvent[]): Promise<void> {
    // 这里可以集成索引更新逻辑
    // 目前先发出事件，让其他服务处理
    for (const change of changes) {
      this.emit('indexUpdateRequired', change);
    }
  }

  /**
   * 清理批量处理状态
   */
  private clearBatchProcessingState(): void {
    for (const timer of this.batchTimers.values()) {
      clearTimeout(timer);
    }
    this.batchTimers.clear();
    this.changeAccumulator.clear();
  }

  /**
   * 启发式检测文件是否为二进制文件（基于扩展名）
   */
  private isLikelyBinaryFile(filePath: string): boolean {
    const binaryExtensions = [
      '.bin', '.exe', '.dll', '.so', '.dylib',
      '.jpg', '.jpeg', '.png', '.gif', '.bmp', '.ico',
      '.zip', '.tar', '.gz', '.rar', '.7z',
      '.pdf', '.doc', '.docx', '.xls', '.xlsx',
      '.mp3', '.mp4', '.avi', '.mov', '.mkv',
      '.jar', '.class', '.o', '.a', '.lib'
    ];
    const ext = path.extname(filePath).toLowerCase();
    return binaryExtensions.includes(ext);
  }

}