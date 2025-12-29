import { ProjectState, StorageStatus } from '../ProjectStateManager';
import { ProjectStateStorageUtils } from './ProjectStateStorageUtils';
import { LoggerService } from '../../../utils/LoggerService';
import { TYPES } from '../../../types';
import { injectable, inject } from 'inversify';

/**
 * 项目状态验证器
 * 职责：验证和规范化项目状态数据
 */
@injectable()
export class ProjectStateValidator {
  constructor(
    @inject(TYPES.LoggerService) private logger: LoggerService
  ) {}

  /**
   * 验证并规范化项目状态
   */
  static validateAndNormalizeProjectState(rawState: any): ProjectState {
    if (!rawState) {
      throw new Error('Project state is required');
    }

    // 验证必需字段
    if (!rawState.projectId || typeof rawState.projectId !== 'string') {
      throw new Error('Invalid or missing projectId');
    }
    if (!rawState.projectPath || typeof rawState.projectPath !== 'string') {
      throw new Error('Invalid or missing projectPath');
    }
    if (rawState.projectPath.trim() === '') {
      throw new Error('projectPath cannot be empty or contain only whitespace');
    }
    if (!rawState.name || typeof rawState.name !== 'string') {
      throw new Error('Invalid or missing name');
    }
    if (!['active', 'inactive', 'indexing', 'error'].includes(rawState.status)) {
      throw new Error('Invalid or missing status');
    }
    if (!rawState.createdAt || !rawState.updatedAt) {
      throw new Error('Missing timestamp fields');
    }

    // 验证settings对象
    if (!rawState.settings || typeof rawState.settings !== 'object') {
      throw new Error('Invalid or missing settings');
    }

    // 构建规范化的项目状态
    const normalizedState: ProjectState = {
      projectId: rawState.projectId,
      projectPath: rawState.projectPath,
      name: rawState.name,
      description: rawState.description || undefined,
      status: rawState.status,
      vectorStatus: rawState.vectorStatus ? ProjectStateValidator.normalizeStorageStatus(rawState.vectorStatus) : ProjectStateValidator.initializeStorageStatus(),
      graphStatus: rawState.graphStatus ? ProjectStateValidator.normalizeStorageStatus(rawState.graphStatus) : ProjectStateValidator.initializeStorageStatus(),
      createdAt: ProjectStateStorageUtils.parseDate(rawState.createdAt),
      updatedAt: ProjectStateStorageUtils.parseDate(rawState.updatedAt),
      lastIndexedAt: rawState.lastIndexedAt ? ProjectStateStorageUtils.parseDate(rawState.lastIndexedAt) : undefined,
      indexingProgress: typeof rawState.indexingProgress === 'number' ? rawState.indexingProgress : undefined,
      totalFiles: typeof rawState.totalFiles === 'number' ? rawState.totalFiles : undefined,
      indexedFiles: typeof rawState.indexedFiles === 'number' ? rawState.indexedFiles : undefined,
      failedFiles: typeof rawState.failedFiles === 'number' ? rawState.failedFiles : undefined,
      collectionInfo: rawState.collectionInfo ? {
        name: rawState.collectionInfo.name,
        vectorsCount: typeof rawState.collectionInfo.vectorsCount === 'number' ? rawState.collectionInfo.vectorsCount : 0,
        status: rawState.collectionInfo.status || 'unknown'
      } : undefined,

      settings: {
        autoIndex: typeof rawState.settings.autoIndex === 'boolean' ? rawState.settings.autoIndex : true,
        watchChanges: typeof rawState.settings.watchChanges === 'boolean' ? rawState.settings.watchChanges : true,
        includePatterns: Array.isArray(rawState.settings.includePatterns) ? rawState.settings.includePatterns : undefined,
        excludePatterns: Array.isArray(rawState.settings.excludePatterns) ? rawState.settings.excludePatterns : undefined,
        chunkSize: typeof rawState.settings.chunkSize === 'number' ? rawState.settings.chunkSize : undefined,
        chunkOverlap: typeof rawState.settings.chunkOverlap === 'number' ? rawState.settings.chunkOverlap : undefined,
      },
      hotReload: rawState.hotReload ? {
        enabled: typeof rawState.hotReload.enabled === 'boolean' ? rawState.hotReload.enabled : false,
        config: {
          debounceInterval: typeof rawState.hotReload.config?.debounceInterval === 'number' ? rawState.hotReload.config.debounceInterval : 500,
          watchPatterns: Array.isArray(rawState.hotReload.config?.watchPatterns) ? rawState.hotReload.config.watchPatterns : ['**/*.{js,ts,jsx,tsx,json,md,py,go,java,css,html,scss}'],
          ignorePatterns: Array.isArray(rawState.hotReload.config?.ignorePatterns) ? rawState.hotReload.config.ignorePatterns : [
            '**/node_modules/**',
            '**/.git/**',
            '**/dist/**',
            '**/build/**',
            '**/target/**',
            '**/venv/**',
            '**/.vscode/**',
            '**/.idea/**',
            '**/logs/**',
            '**/*.log',
            '**/coverage/**',
            '**/tmp/**',
            '**/temp/**'
          ],
          maxFileSize: typeof rawState.hotReload.config?.maxFileSize === 'number' ? rawState.hotReload.config.maxFileSize : 512000,
          errorHandling: rawState.hotReload.config?.errorHandling ? {
            maxRetries: typeof rawState.hotReload.config.errorHandling.maxRetries === 'number' ? rawState.hotReload.config.errorHandling.maxRetries : 3,
            alertThreshold: typeof rawState.hotReload.config.errorHandling.alertThreshold === 'number' ? rawState.hotReload.config.errorHandling.alertThreshold : 5,
            autoRecovery: typeof rawState.hotReload.config.errorHandling.autoRecovery === 'boolean' ? rawState.hotReload.config.errorHandling.autoRecovery : true
          } : {
            maxRetries: 3,
            alertThreshold: 5,
            autoRecovery: true
          }
        },
        lastEnabled: rawState.hotReload.lastEnabled ? ProjectStateStorageUtils.parseDate(rawState.hotReload.lastEnabled) : undefined,
        lastDisabled: rawState.hotReload.lastDisabled ? ProjectStateStorageUtils.parseDate(rawState.hotReload.lastDisabled) : undefined,
        changesDetected: typeof rawState.hotReload.changesDetected === 'number' ? rawState.hotReload.changesDetected : 0,
        errorsCount: typeof rawState.hotReload.errorsCount === 'number' ? rawState.hotReload.errorsCount : 0
      } : {
        enabled: false,
        config: {
          debounceInterval: 500,
          watchPatterns: ['**/*.{js,ts,jsx,tsx,json,md,py,go,java,css,html,scss}'],
          ignorePatterns: [
            '**/node_modules/**',
            '**/.git/**',
            '**/dist/**',
            '**/build/**',
            '**/target/**',
            '**/venv/**',
            '**/.vscode/**',
            '**/.idea/**',
            '**/logs/**',
            '**/*.log',
            '**/coverage/**',
            '**/tmp/**',
            '**/temp/**'
          ],
          maxFileSize: 512000, // 500KB
          errorHandling: {
            maxRetries: 3,
            alertThreshold: 5,
            autoRecovery: true
          }
        },
        changesDetected: 0,
        errorsCount: 0
      },
      metadata: rawState.metadata && typeof rawState.metadata === 'object' ? rawState.metadata : {}
    };

    return normalizedState;
  }

  /**
   * 检查项目状态是否有效
   */
  static async isProjectStateValid(state: ProjectState): Promise<boolean> {
    try {
      // 检查项目路径是否存在
      const fs = await import('fs/promises');
      await fs.access(state.projectPath);
      return true;
    } catch (error) {
      // 路径不存在，状态无效
      return false;
    }
  }

  /**
   * 初始化存储状态
   */
  static initializeStorageStatus(): StorageStatus {
    return {
      status: 'pending',
      progress: 0,
      lastUpdated: new Date()
    };
  }

  /**
   * 规范化存储状态
   */
  static normalizeStorageStatus(rawStatus: any): StorageStatus {
    // 处理null或undefined输入
    if (rawStatus === null || rawStatus === undefined) {
      return ProjectStateValidator.initializeStorageStatus();
    }
    
    return {
      status: ['pending', 'indexing', 'completed', 'error', 'partial', 'disabled'].includes(rawStatus.status) ? rawStatus.status : 'pending',
      progress: typeof rawStatus.progress === 'number' ? (rawStatus.progress > 100 ? 0 : Math.max(0, Math.min(100, rawStatus.progress))) : 0,
      totalFiles: typeof rawStatus.totalFiles === 'number' ? rawStatus.totalFiles : undefined,
      processedFiles: typeof rawStatus.processedFiles === 'number' ? rawStatus.processedFiles : undefined,
      failedFiles: typeof rawStatus.failedFiles === 'number' ? rawStatus.failedFiles : undefined,
      lastUpdated: rawStatus.lastUpdated ? ProjectStateStorageUtils.parseDate(rawStatus.lastUpdated) : new Date(),
      lastCompleted: rawStatus.lastCompleted ? ProjectStateStorageUtils.parseDate(rawStatus.lastCompleted) : undefined,
      error: typeof rawStatus.error === 'string' ? rawStatus.error : undefined
    };
  }
}