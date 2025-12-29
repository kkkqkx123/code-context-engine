
import { injectable, inject } from 'inversify';
import { TYPES } from '../../types';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { ConfigService } from '../../config/ConfigService';
import { ProjectStateStorageUtils } from './utils/ProjectStateStorageUtils';
import { ProjectStateValidator } from './utils/ProjectStateValidator';
import { CoreStateService } from './services/CoreStateService';
import { StorageStateService } from './services/StorageStateService';
import { SqliteStateManager } from '../../database/splite/SqliteStateManager';

export interface ProjectState {
  projectId: string;
  projectPath: string;
  name: string;
  description?: string;
  status: 'active' | 'inactive' | 'indexing' | 'error';

  // 分离的状态管理
  vectorStatus: StorageStatus;
  graphStatus: StorageStatus;

  createdAt: Date;
  updatedAt: Date;
  lastIndexedAt?: Date;
  indexingProgress?: number;
  totalFiles?: number;
  indexedFiles?: number;
  failedFiles?: number;
  collectionInfo?: {
    name: string;
    vectorsCount: number;
    status: string;
  };
  settings: {
    autoIndex: boolean;
    watchChanges: boolean;
    includePatterns?: string[];
    excludePatterns?: string[];
    chunkSize?: number;
    chunkOverlap?: number;
  };
  hotReload: {
    enabled: boolean;
    config: {
      debounceInterval?: number;
      watchPatterns?: string[];
      ignorePatterns?: string[];
      maxFileSize?: number;
      errorHandling?: {
        maxRetries?: number;
        alertThreshold?: number;
        autoRecovery?: boolean;
      };
    };
    lastEnabled?: Date;
    lastDisabled?: Date;
    changesDetected?: number;
    errorsCount?: number;
  };
  metadata?: Record<string, any>;
}

// 新增存储状态接口
export interface StorageStatus {
  status: 'pending' | 'indexing' | 'completed' | 'error' | 'partial' | 'disabled';
  progress: number; // 0-100
  totalFiles?: number;
  processedFiles?: number;
  failedFiles?: number;
  lastUpdated: Date;
  lastCompleted?: Date;
  error?: string;
}

export interface ProjectStats {
  totalProjects: number;
  activeProjects: number;
  indexingProjects: number;
  totalVectors: number;
  totalFiles: number;
  averageIndexingProgress: number;
}

@injectable()
export class ProjectStateManager {
  private projectStates: Map<string, ProjectState> = new Map();
  private storagePath: string;
  private isInitialized: boolean = false;

  constructor(
    @inject(TYPES.LoggerService) private logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) private errorHandler: ErrorHandlerService,
    @inject(TYPES.ConfigService) private configService: ConfigService,
    @inject(TYPES.CoreStateService) private coreStateService: CoreStateService,
    @inject(TYPES.StorageStateService) private storageStateService: StorageStateService,
    @inject(TYPES.SqliteStateManager) private sqliteStateManager: SqliteStateManager
  ) {
    // 存储路径将在initialize方法中设置
    this.storagePath = './data/project-states.json'; // 默认路径
  }

  /**
   * 初始化项目状态管理器
   */
  async initialize(): Promise<void> {
    try {
      // 从配置中获取存储路径
      try {
        const projectConfig = this.configService.get('project');
        if (projectConfig?.statePath) {
          this.storagePath = projectConfig.statePath;
        }
      } catch (configError) {
        this.logger.warn('Failed to get project config, using default storage path', {
          error: configError instanceof Error ? configError.message : String(configError),
          defaultPath: this.storagePath
        });
      }

      // 加载项目状态
      await this.loadProjectStates();

      this.isInitialized = true;
      this.logger.info('Project state manager initialized', {
        projectCount: this.projectStates.size,
        projects: Array.from(this.projectStates.keys()),
        storagePath: this.storagePath
      });
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to initialize project state manager: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'ProjectStateManager', operation: 'initialize' }
      );
      throw error;
    }
  }

  /**
   * 加载项目状态
   */
  private async loadProjectStates(): Promise<void> {
    try {
      // 首先从SQLite加载
      await this.loadProjectStatesFromSQLite();
      
      // 如果SQLite中没有数据，再从JSON文件加载（向后兼容）
      if (this.projectStates.size === 0) {
        await this.loadProjectStatesFromJson();
      }
    } catch (error) {
      // 只有在文件不存在的情况下才忽略错误，其他错误需要继续抛出
      if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
        this.logger.info('Project states file does not exist, initializing empty states');
        this.projectStates = new Map();
      } else {
        this.logger.error('Failed to load project states', error);
        throw error; // 其他错误继续抛出
      }
    }
  }

  /**
   * 从SQLite加载项目状态
   */
  private async loadProjectStatesFromSQLite(): Promise<void> {
    try {
      // 检查数据库是否已连接
      if (!this.sqliteStateManager['sqliteService'].isConnected()) {
        this.logger.warn('SQLite database not connected, skipping SQLite load');
        this.projectStates = new Map();
        return;
      }
      
      const states = await this.sqliteStateManager.getAllProjectStates();
      
      this.projectStates = new Map();
      
      for (const state of states) {
        const projectState: ProjectState = {
          projectId: state.projectId,
          projectPath: '', // 将在后续步骤中填充
          name: state.projectId,
          status: 'active',
          vectorStatus: state.vectorStatus,
          graphStatus: state.graphStatus,
          createdAt: state.lastUpdated,
          updatedAt: state.lastUpdated,
          lastIndexedAt: state.lastUpdated,
          indexingProgress: state.indexingProgress,
          totalFiles: state.totalFiles,
          indexedFiles: state.indexedFiles,
          failedFiles: state.failedFiles,
          hotReload: state.hotReload ? {
            enabled: state.hotReload.enabled,
            config: state.hotReload.config || {
              debounceInterval: this.coreStateService['hotReloadConfigService'].getGlobalConfig().defaultDebounceInterval,
              watchPatterns: this.coreStateService['hotReloadConfigService'].getGlobalConfig().defaultWatchPatterns,
              ignorePatterns: this.coreStateService['hotReloadConfigService'].getGlobalConfig().defaultIgnorePatterns
            },
            lastEnabled: state.hotReload.lastEnabled,
            lastDisabled: state.hotReload.lastDisabled,
            changesDetected: state.hotReload.changesDetected,
            errorsCount: state.hotReload.errorsCount
          } : {
            enabled: false,
            config: {
              debounceInterval: this.coreStateService['hotReloadConfigService'].getGlobalConfig().defaultDebounceInterval,
              watchPatterns: this.coreStateService['hotReloadConfigService'].getGlobalConfig().defaultWatchPatterns,
              ignorePatterns: this.coreStateService['hotReloadConfigService'].getGlobalConfig().defaultIgnorePatterns
            }
          },
          settings: {
            autoIndex: true,
            watchChanges: true
          }
        };

        this.projectStates.set(state.projectId, projectState);
      }
      
      this.logger.info(`Loaded ${states.length} project states from SQLite`);
    } catch (error) {
      this.logger.warn('Failed to load project states from SQLite, continuing with empty states', error);
      // 确保在出错时仍然有一个空的映射
      this.projectStates = new Map();
    }
  }

  /**
   * 从JSON文件加载项目状态（向后兼容）
   */
  private async loadProjectStatesFromJson(): Promise<void> {
    this.projectStates = await ProjectStateStorageUtils.loadProjectStates(
      this.storagePath,
      this.logger,
      (rawState: any) => ProjectStateValidator.validateAndNormalizeProjectState(rawState)
    );
  }

  /**
   * 保存项目状态
   */
  private async saveProjectStates(): Promise<void> {
    try {
      // 保存到SQLite
      await this.saveProjectStatesToSQLite();
      
      // 同时保存到JSON文件作为备份（向后兼容）
      await this.saveProjectStatesToJson();
    } catch (error) {
      this.logger.error('Failed to save project states', error);
      throw error;
    }
  }

  /**
   * 保存项目状态到SQLite
   */
  private async saveProjectStatesToSQLite(): Promise<void> {
    try {
      const states = Array.from(this.projectStates.values()).map(state => ({
        projectId: state.projectId,
        vectorStatus: state.vectorStatus,
        graphStatus: state.graphStatus,
        indexingProgress: state.indexingProgress || 0,
        totalFiles: state.totalFiles || 0,
        indexedFiles: state.indexedFiles || 0,
        failedFiles: state.failedFiles || 0,
        lastUpdated: state.updatedAt,
        hotReload: state.hotReload
      }));
      
      const success = await this.sqliteStateManager.batchSaveProjectStates(states);
      
      if (success) {
        this.logger.info(`Saved ${states.length} project states to SQLite`);
      } else {
        this.logger.warn('Failed to save project states to SQLite');
      }
    } catch (error) {
      this.logger.error('Failed to save project states to SQLite', error);
      throw error;
    }
  }

  /**
   * 保存项目状态到JSON文件（向后兼容）
   */
  private async saveProjectStatesToJson(): Promise<void> {
    await ProjectStateStorageUtils.saveProjectStates(
      this.projectStates,
      this.storagePath,
      this.logger,
      this.errorHandler
    );
  }

  /**
   * 创建或更新项目状态
   */
  async createOrUpdateProjectState(
    projectPath: string,
    options: {
      name?: string;
      description?: string;
      settings?: Partial<ProjectState['settings']>;
      hotReload?: Partial<ProjectState['hotReload']>;
      metadata?: Record<string, any>;
      allowReindex?: boolean;
    } = {}
  ): Promise<ProjectState> {
    const state = await this.coreStateService.createOrUpdateProjectState(
      this.projectStates,
      projectPath,
      this.storagePath,
      options
    );

    // 确保热更新配置有默认值（如果还没有的话）
    if (state && !state.hotReload) {
      // 使用 HotReloadConfigService 提供的默认配置
      const hotReloadConfig = this.coreStateService['hotReloadConfigService'].getGlobalConfig();
      state.hotReload = {
        enabled: false,
        config: {
          debounceInterval: hotReloadConfig.defaultDebounceInterval,
          watchPatterns: hotReloadConfig.defaultWatchPatterns,
          ignorePatterns: hotReloadConfig.defaultIgnorePatterns,
          maxFileSize: hotReloadConfig.defaultMaxFileSize,
          errorHandling: hotReloadConfig.defaultErrorHandling
        },
        changesDetected: 0,
        errorsCount: 0
      };
    }

    // 保存状态到SQLite和JSON
    await this.saveProjectStates();

    return state;
  }

  /**
   * 获取项目状态
   */
  getProjectState(projectId: string): ProjectState | null {
    return this.coreStateService.getProjectState(this.projectStates, projectId);
  }

  /**
   * 获取所有项目状态
   */
  getAllProjectStates(): ProjectState[] {
    return this.coreStateService.getAllProjectStates(this.projectStates);
  }

  /**
   * 根据项目路径获取项目状态
   */
  getProjectStateByPath(projectPath: string): ProjectState | null {
    return this.coreStateService.getProjectStateByPath(this.projectStates, projectPath);
  }

  /**
   * 更新项目状态
   */
  private async updateProjectStatus(projectId: string, status: ProjectState['status']): Promise<void> {
    await this.coreStateService.updateProjectStatus(this.projectStates, projectId, status, this.storagePath);
    
    // 同步更新到SQLite
    const state = this.projectStates.get(projectId);
    if (state) {
      await this.sqliteStateManager.updateProjectStateFields(projectId, { 
        vectorStatus: { ...state.vectorStatus, status },
        graphStatus: { ...state.graphStatus, status }
      });
    }
    
    await this.saveProjectStatesToJson(); // 保持JSON备份
  }

  /**
   * 更新项目索引进度
   */
  private async updateProjectIndexingProgress(projectId: string, progress: number): Promise<void> {
    await this.coreStateService.updateProjectIndexingProgress(this.projectStates, projectId, progress, this.storagePath);
    
    // 同步更新到SQLite
    const state = this.projectStates.get(projectId);
    if (state) {
      await this.sqliteStateManager.updateProjectStateFields(projectId, { indexingProgress: progress });
    }
    
    await this.saveProjectStatesToJson(); // 保持JSON备份
  }

  /**
   * 更新项目最后索引时间
   */
  private async updateProjectLastIndexed(projectId: string): Promise<void> {
    await this.coreStateService.updateProjectLastIndexed(this.projectStates, projectId, this.storagePath);
    
    // 同步更新到SQLite
    const now = new Date();
    await this.sqliteStateManager.updateProjectStateFields(projectId, { lastUpdated: now });
    
    await this.saveProjectStatesToJson(); // 保持JSON备份
  }

  /**
   * 更新项目元数据
   */
  private async updateProjectMetadata(projectId: string, metadata: Record<string, any>): Promise<void> {
    await this.coreStateService.updateProjectMetadata(this.projectStates, projectId, metadata, this.storagePath);
    await this.saveProjectStates();
  }

  /**
   * 删除项目状态
   */
  async deleteProjectState(projectId: string): Promise<boolean> {
    const result = await this.coreStateService.deleteProjectState(this.projectStates, projectId, this.storagePath);
    if (result) {
      // 从SQLite中删除
      await this.sqliteStateManager.deleteProjectState(projectId);
      
      await this.saveProjectStatesToJson(); // 保持JSON备份
    }
    return result;
  }

  /**
   * 获取项目统计信息
   */
  getProjectStats(): ProjectStats {
    return this.coreStateService.getProjectStats(this.projectStates);
  }

  /**
   * 激活项目
   */
  async activateProject(projectId: string): Promise<boolean> {
    try {
      const state = this.projectStates.get(projectId);
      if (!state) {
        return false;
      }

      await this.updateProjectStatus(projectId, 'active');
      return true;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to activate project: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'ProjectStateManager', operation: 'activateProject', projectId }
      );
      return false;
    }
  }



  /**
   * 停用项目
   */
  async deactivateProject(projectId: string): Promise<boolean> {
    try {
      const state = this.projectStates.get(projectId);
      if (!state) {
        this.logger.warn(`Project state not found for deactivation: ${projectId}`);
        return false;
      }

      // 更新状态，但即使保存失败也返回true（因为状态已经更新在内存中）
      try {
        await this.updateProjectStatus(projectId, 'inactive');
      } catch (saveError) {
        this.logger.warn(`Failed to save project state during deactivation, but status was updated in memory: ${saveError instanceof Error ? saveError.message : String(saveError)}`);
        // 即使保存失败，状态已经在内存中更新，所以返回true
      }

      return true;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to deactivate project: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'ProjectStateManager', operation: 'deactivateProject', projectId }
      );
      return false;
    }
  }

  /**
   * 刷新项目状态
   */
  async refreshProjectState(projectId: string): Promise<ProjectState | null> {
    const state = await this.coreStateService.refreshProjectState(this.projectStates, projectId, this.storagePath);
    if (state) {
      await this.saveProjectStates();
    }
    return state;
  }

  /**
   * 刷新所有项目状态
   */
  async refreshAllProjectStates(): Promise<void> {
    await this.coreStateService.refreshAllProjectStates(this.projectStates, this.storagePath);
    await this.saveProjectStates();
  }

  /**
   * 清理无效的项目状态
   */
  async cleanupInvalidStates(): Promise<number> {
    const removedCount = await this.coreStateService.cleanupInvalidStates(this.projectStates, this.storagePath);
    if (removedCount > 0) {
      await this.saveProjectStates();
    }
    return removedCount;
  }

  /**
   * 更新向量存储状态
   */
  async updateVectorStatus(projectId: string, status: Partial<StorageStatus>): Promise<void> {
    await this.storageStateService.updateVectorStatus(this.projectStates, projectId, status, this.storagePath);
    
    // 同步更新到SQLite
    const state = this.projectStates.get(projectId);
    if (state) {
      await this.sqliteStateManager.updateProjectStateFields(projectId, { 
        vectorStatus: { ...state.vectorStatus, ...status }
      });
    }
    
    await this.saveProjectStatesToJson(); // 保持JSON备份
  }

  /**
   * 更新图存储状态
   */
  async updateGraphStatus(projectId: string, status: Partial<StorageStatus>): Promise<void> {
    await this.storageStateService.updateGraphStatus(this.projectStates, projectId, status, this.storagePath);
    
    // 同步更新到SQLite
    const state = this.projectStates.get(projectId);
    if (state) {
      await this.sqliteStateManager.updateProjectStateFields(projectId, { 
        graphStatus: { ...state.graphStatus, ...status }
      });
    }
    
    await this.saveProjectStatesToJson(); // 保持JSON备份
  }

  /**
   * 获取向量存储状态
   */
  getVectorStatus(projectId: string): StorageStatus | null {
    return this.storageStateService.getVectorStatus(this.projectStates, projectId);
  }

  /**
   * 获取图存储状态
   */
  getGraphStatus(projectId: string): StorageStatus | null {
    return this.storageStateService.getGraphStatus(this.projectStates, projectId);
  }

  /**
   * 重置向量存储状态
   */
  async resetVectorStatus(projectId: string): Promise<void> {
    await this.storageStateService.resetVectorStatus(this.projectStates, projectId, this.storagePath);
    
    // 同步重置到SQLite
    await this.sqliteStateManager.updateProjectStateFields(projectId, { 
      vectorStatus: { status: 'pending', progress: 0, lastUpdated: new Date() }
    });
    
    await this.saveProjectStatesToJson(); // 保持JSON备份
  }

  /**
   * 重置图存储状态
   */
  async resetGraphStatus(projectId: string): Promise<void> {
    await this.storageStateService.resetGraphStatus(this.projectStates, projectId, this.storagePath);
    
    // 同步重置到SQLite
    await this.sqliteStateManager.updateProjectStateFields(projectId, { 
      graphStatus: { status: 'pending', progress: 0, lastUpdated: new Date() }
    });
    
    await this.saveProjectStatesToJson(); // 保持JSON备份
  }

  /**
   * 开始向量索引
   */
  async startVectorIndexing(projectId: string, totalFiles?: number): Promise<void> {
    await this.storageStateService.startVectorIndexing(this.projectStates, projectId, totalFiles, this.storagePath);
    
    // 同步更新到SQLite
    await this.sqliteStateManager.updateProjectStateFields(projectId, { 
      vectorStatus: { status: 'indexing', progress: 0, lastUpdated: new Date() }
    });
    
    await this.saveProjectStatesToJson(); // 保持JSON备份
  }

  /**
   * 开始图索引
   */
  async startGraphIndexing(projectId: string, totalFiles?: number): Promise<void> {
    await this.storageStateService.startGraphIndexing(this.projectStates, projectId, totalFiles, this.storagePath);
    
    // 同步更新到SQLite
    await this.sqliteStateManager.updateProjectStateFields(projectId, { 
      graphStatus: { status: 'indexing', progress: 0, lastUpdated: new Date() }
    });
    
    await this.saveProjectStatesToJson(); // 保持JSON备份
  }

  /**
   * 更新向量索引进度
   */
  async updateVectorIndexingProgress(projectId: string, progress: number, processedFiles?: number, failedFiles?: number): Promise<void> {
    await this.storageStateService.updateVectorIndexingProgress(this.projectStates, projectId, progress, processedFiles, failedFiles, this.storagePath);
    
    // 同步更新到SQLite
    await this.sqliteStateManager.updateProjectStateFields(projectId, { 
      vectorStatus: { progress, lastUpdated: new Date() }
    });
    
    await this.saveProjectStatesToJson(); // 保持JSON备份
  }

  /**
   * 更新图索引进度
   */
  async updateGraphIndexingProgress(projectId: string, progress: number, processedFiles?: number, failedFiles?: number): Promise<void> {
    await this.storageStateService.updateGraphIndexingProgress(this.projectStates, projectId, progress, processedFiles, failedFiles, this.storagePath);
    
    // 同步更新到SQLite
    await this.sqliteStateManager.updateProjectStateFields(projectId, { 
      graphStatus: { progress, lastUpdated: new Date() }
    });
    
    await this.saveProjectStatesToJson(); // 保持JSON备份
  }

  /**
   * 完成向量索引
   */
  async completeVectorIndexing(projectId: string): Promise<void> {
    await this.storageStateService.completeVectorIndexing(this.projectStates, projectId, this.storagePath);
    
    // 同步更新到SQLite
    await this.sqliteStateManager.updateProjectStateFields(projectId, { 
      vectorStatus: { status: 'completed', progress: 100, lastUpdated: new Date() }
    });
    
    await this.saveProjectStatesToJson(); // 保持JSON备份
  }

  /**
   * 完成图索引
   */
  async completeGraphIndexing(projectId: string): Promise<void> {
    await this.storageStateService.completeGraphIndexing(this.projectStates, projectId, this.storagePath);
    
    // 同步更新到SQLite
    await this.sqliteStateManager.updateProjectStateFields(projectId, { 
      graphStatus: { status: 'completed', progress: 100, lastUpdated: new Date() }
    });
    
    await this.saveProjectStatesToJson(); // 保持JSON备份
  }

  /**
   * 向量索引出错
   */
  async failVectorIndexing(projectId: string, error: string): Promise<void> {
    await this.storageStateService.failVectorIndexing(this.projectStates, projectId, error, this.storagePath);
    
    // 同步更新到SQLite
    await this.sqliteStateManager.updateProjectStateFields(projectId, { 
      vectorStatus: { status: 'error', error, lastUpdated: new Date() }
    });
    
    await this.saveProjectStatesToJson(); // 保持JSON备份
  }

  /**
   * 图索引出错
   */
  async failGraphIndexing(projectId: string, error: string): Promise<void> {
    await this.storageStateService.failGraphIndexing(this.projectStates, projectId, error, this.storagePath);
    
    // 同步更新到SQLite
    await this.sqliteStateManager.updateProjectStateFields(projectId, { 
      graphStatus: { status: 'error', error, lastUpdated: new Date() }
    });
    
    await this.saveProjectStatesToJson(); // 保持JSON备份
  }
  /**
   * 禁用图索引
   */
  async disableGraphIndexing(projectId: string): Promise<void> {
    await this.storageStateService.disableGraphIndexing(this.projectStates, projectId, this.storagePath);
    
    // 同步更新到SQLite
    await this.sqliteStateManager.updateProjectStateFields(projectId, { 
      graphStatus: { status: 'disabled', lastUpdated: new Date() }
    });
    
    await this.saveProjectStatesToJson(); // 保持JSON备份
  }

  /**
   * 获取所有已知项目的路径
   */
  async getAllProjects(): Promise<string[]> {
    try {
      const allStates = this.getAllProjectStates();
      return allStates.map(state => state.projectPath);
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to get all projects: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'ProjectStateManager', operation: 'getAllProjects' }
      );
      return [];
    }
  }

  /**
   * 根据项目路径获取项目ID
   */
  getProjectId(projectPath: string): string | null {
    try {
      const projectId = this.coreStateService['projectIdManager'].getProjectId(projectPath);
      return projectId || null; // Convert undefined to null
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to get project ID for path: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'ProjectStateManager', operation: 'getProjectId', projectPath }
      );
      return null;
    }
  }
}