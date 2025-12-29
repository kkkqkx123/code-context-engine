
import { injectable, inject } from 'inversify';
import { TYPES } from '../../../types';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { ProjectIdManager } from '../../../database/ProjectIdManager';
import { QdrantService } from '../../../database/qdrant/QdrantService';
import { NebulaClient } from '../../../database/nebula/client/NebulaClient';
import { ProjectState, ProjectStats, StorageStatus } from '../ProjectStateManager';
import { ProjectStateStorageUtils } from '../utils/ProjectStateStorageUtils';
import { ProjectStateValidator } from '../utils/ProjectStateValidator';
import { HotReloadConfigService } from '../../filesystem/HotReloadConfigService';
import * as path from 'path';

/**
 * 核心状态服务
 * 职责：基本状态管理、业务逻辑
 */
@injectable()
export class CoreStateService {
  constructor(
    @inject(TYPES.LoggerService) private logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) private errorHandler: ErrorHandlerService,
    @inject(TYPES.ProjectIdManager) private projectIdManager: ProjectIdManager,
    @inject(TYPES.QdrantService) private qdrantService: QdrantService,
    @inject(TYPES.NebulaClient) private nebulaClient: NebulaClient,
    @inject(TYPES.HotReloadConfigService) private hotReloadConfigService: HotReloadConfigService
  ) {}

  /**
   * 创建或更新项目状态
   */
  async createOrUpdateProjectState(
    projectStates: Map<string, ProjectState>,
    projectPath: string,
    storagePath: string,
    options: {
      name?: string;
      description?: string;
      settings?: Partial<ProjectState['settings']>;
      hotReload?: Partial<ProjectState['hotReload']>;
      metadata?: Record<string, any>;
      allowReindex?: boolean;
    } = {}
  ): Promise<ProjectState> {
    try {
      // 生成或获取项目ID
      const projectId = await this.projectIdManager.generateProjectId(projectPath);

      // 检查是否已存在相同 projectPath 的项目状态
      const existingStateByPath = this.getProjectStateByDirectPath(projectStates, projectPath);
      const allowReindex = options.allowReindex ?? false;
      
      if (existingStateByPath && !allowReindex && existingStateByPath.projectId !== projectId) {
        // 如果已存在相同路径的项目，且不是重新索引操作，且projectId不同，则抛出错误
        throw new Error(`项目路径 "${projectPath}" 已被项目 "${existingStateByPath.projectId}" 使用，不能重复添加`);
      } else if (existingStateByPath && allowReindex && existingStateByPath.projectId !== projectId) {
        // 如果是重新索引操作且projectId不同，清理旧的项目状态和相关资源
        this.logger.info(`重新索引项目，清理旧项目状态和资源: ${existingStateByPath.projectId}`, {
          oldProjectId: existingStateByPath.projectId,
          newProjectId: projectId,
          projectPath
        });
        
        // 从内存中删除旧项目状态
        projectStates.delete(existingStateByPath.projectId);
        
        // 删除旧的Qdrant集合
        try {
          await this.qdrantService.deleteCollectionForProject(existingStateByPath.projectPath);
          this.logger.info(`已删除旧项目的Qdrant集合: ${existingStateByPath.projectPath}`, {
            projectId: existingStateByPath.projectId
          });
        } catch (error) {
          this.logger.warn(`删除旧项目Qdrant集合失败: ${existingStateByPath.projectPath}`, {
            error: error instanceof Error ? error.message : String(error),
            projectId: existingStateByPath.projectId
          });
        }
        
        // 删除旧的Nebula Graph空间
        try {
          await this.nebulaClient.deleteSpaceForProject(existingStateByPath.projectPath);
          this.logger.info(`已删除旧项目的Nebula空间: ${existingStateByPath.projectPath}`, {
            projectId: existingStateByPath.projectId
          });
        } catch (error) {
          this.logger.warn(`删除旧项目Nebula空间失败: ${existingStateByPath.projectPath}`, {
            error: error instanceof Error ? error.message : String(error),
            projectId: existingStateByPath.projectId
          });
        }
      }

      // 检查是否已存在状态
      let state = projectStates.get(projectId);

      if (state) {
        // 更新现有状态
        state.updatedAt = new Date();
        if (options.name) state.name = options.name;
        if (

options.description !== undefined) state.description = options.description;
        if (options.settings) state.settings = { ...state.settings, ...options.settings };
        if (options.hotReload) {
          // 处理热更新配置的变更
          const oldEnabled = state.hotReload?.enabled;
          const newEnabled = options.hotReload.enabled;
          
          state.hotReload = { ...state.hotReload, ...options.hotReload };
          
          // 记录启用/禁用时间
          if (oldEnabled !== newEnabled) {
            if (newEnabled === true) {
              state.hotReload.lastEnabled = new Date();
            } else if (newEnabled === false) {
              state.hotReload.lastDisabled = new Date();
            }
          }
        }
        if (options.metadata) state.metadata = { ...state.metadata, ...options.metadata };
      } else {
        // 创建新状态
        const vectorStatus = ProjectStateValidator.initializeStorageStatus();
        let graphStatus = ProjectStateValidator.initializeStorageStatus();
        
        // 检查NEBULA_ENABLED环境变量，如果禁用则设置图索引状态为"已禁用"
        const nebulaEnabled = process.env.NEBULA_ENABLED?.toLowerCase() !== 'false';
        if (!nebulaEnabled) {
          graphStatus.status = 'disabled';
        }
        
        state = {
          projectId,
          projectPath,
          name: options.name || path.basename(projectPath),
          description: options.description,
          status: 'inactive',
          vectorStatus,
          graphStatus,
          createdAt: new Date(),
          updatedAt: new Date(),
          settings: {
            autoIndex: true,
            watchChanges: true,
            ...options.settings
          },
          hotReload: {
            enabled: false,
            config: {
              debounceInterval: this.hotReloadConfigService.getGlobalConfig().defaultDebounceInterval,
              watchPatterns: this.hotReloadConfigService.getGlobalConfig().defaultWatchPatterns,
              ignorePatterns: this.hotReloadConfigService.getGlobalConfig().defaultIgnorePatterns,
              maxFileSize: this.hotReloadConfigService.getGlobalConfig().defaultMaxFileSize,
              errorHandling: this.hotReloadConfigService.getGlobalConfig().defaultErrorHandling
            },
            changesDetected: 0,
            errorsCount: 0
          },
          metadata: options.metadata || {}
        };
      }

      // 更新集合信息
      await this.updateProjectCollectionInfo(projectStates, projectId);

      // 重新获取状态以确保collectionInfo已更新
      state = projectStates.get(projectId) || state;

      // 保存状态
      if (!state) {
        throw new Error(`Failed to create or update project state for ${projectId}`);
      }
      
      projectStates.set(projectId, state);

      this.logger.info(`Project state ${state ? 'updated' : 'created'} for ${projectId}`, {
        projectId,
        projectPath,
        status: state.status,
        hasIndexingProgress: !!state.indexingProgress,
        hasLastIndexedAt: !!state.lastIndexedAt
      });
      return state;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to create or update project state: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'CoreStateService', operation: 'createOrUpdateProjectState', projectPath, options }
      );
      throw error;
    }
  }

  /**
   * 获取项目状态
   */
  getProjectState(projectStates: Map<string, ProjectState>, projectId: string): ProjectState | null {
    return projectStates.get(projectId) || null;
  }

  /**
   * 获取所有项目状态
   */
  getAllProjectStates(projectStates: Map<string, ProjectState>): ProjectState[] {
    return Array.from(projectStates.values());
  }

  /**
   * 根据项目路径获取项目状态
   */
  getProjectStateByPath(projectStates: Map<string, ProjectState>, projectPath: string): ProjectState | null {
    const projectId = this.projectIdManager.getProjectId(projectPath);
    return projectId ? this.getProjectState(projectStates, projectId) : null;
  }

  /**
   * 直接通过 projectPath 查找项目状态（不通过 projectIdManager）
   */
  private getProjectStateByDirectPath(projectStates: Map<string, ProjectState>, projectPath: string): ProjectState | null {
    // 标准化路径以避免路径格式差异（如 / 和 \ 的区别）
    const { HashUtils } = require('../../../utils/HashUtils');
    const normalizedPath = HashUtils.normalizePath(projectPath);

    // 遍历所有项目状态，查找匹配的 projectPath
    for (const state of projectStates.values()) {
      if (HashUtils.normalizePath(state.projectPath) === normalizedPath) {
        return state;
      }
    }

    return null;
  }

  /**
   * 删除项目状态
   */
  async deleteProjectState(
    projectStates: Map<string, ProjectState>,
    projectId: string,
    storagePath: string
  ): Promise<boolean> {
    try {
      const state = projectStates.get(projectId);
      if (!state) {
        return false;
      }

      // 删除对应的向量集合（这会自动删除ProjectIdManager中的映射）
      try {
        await this.qdrantService.deleteCollectionForProject(state.projectPath);
        this.logger.info(`Deleted Qdrant collection for project ${projectId}`);
      } catch (collectionError) {
        this.logger.warn(`Failed to delete Qdrant collection for project ${projectId}`, { error: collectionError });
        // 即使删除集合失败，也继续删除项目状态
      }
      
      // 删除对应的Nebula Graph空间（注意：这里可能会重复删除映射，但removeProject方法会处理）
      try {
        await this.nebulaClient.deleteSpaceForProject(state.projectPath);
        this.logger.info(`Deleted Nebula space for project ${projectId}`);
      } catch (spaceError) {
        this.logger.warn(`Failed to delete Nebula space for project ${projectId}`, { error: spaceError });
        // 即使删除空间失败，也继续删除项目状态
      }

      // 从映射中删除
      projectStates.delete(projectId);

      this.logger.info(`Deleted project state for ${projectId}`);
      return true;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to delete project state: ${error

 instanceof Error ? error.message : String(error)}`),
        { component: 'CoreStateService', operation: 'deleteProjectState', projectId }
      );
      return false;
    }
  }

  /**
   * 获取项目统计信息
   */
  getProjectStats(projectStates: Map<string, ProjectState>): ProjectStats {
    const states = Array.from(projectStates.values());

    const totalProjects = states.length;
    const activeProjects = states.filter(s => s.status === 'active').length;
    const indexingProjects = states.filter(s => s.status === 'indexing').length;
    const totalVectors = states.reduce((sum, s) => sum + (s.collectionInfo?.vectorsCount || 0), 0);
    const totalFiles = states.reduce((sum, s) => sum + (s.totalFiles || 0), 0);
    const averageIndexingProgress = states.length > 0
      ? states.reduce((sum, s) => sum + (s.indexingProgress || 0), 0) / states.length
      : 0;

    return {
      totalProjects,
      activeProjects,
      indexingProjects,
      totalVectors,
      totalFiles,
      averageIndexingProgress
    };
  }

  /**
   * 更新项目状态
   */
  async updateProjectStatus(
    projectStates: Map<string, ProjectState>,
    projectId: string,
    status: ProjectState['status'],
    storagePath: string
  ): Promise<void> {
    const state = projectStates.get(projectId);
    if (!state) return;

    state.status = status;
    state.updatedAt = new Date();

    projectStates.set(projectId, state);

    this.logger.debug(`Updated project status for ${projectId}: ${status}`);
  }

  /**
   * 更新项目索引进度
   */
  async updateProjectIndexingProgress(
    projectStates: Map<string, ProjectState>,
    projectId: string,
    progress: number,
    storagePath: string
  ): Promise<void> {
    const state = projectStates.get(projectId);
    if (!state) return;

    state.indexingProgress = progress;
    state.updatedAt = new Date();

    projectStates.set(projectId, state);

    this.logger.debug(`Updated indexing progress for ${projectId}: ${progress}%`);
  }

  /**
   * 更新项目最后索引时间
   */
  async updateProjectLastIndexed(
    projectStates: Map<string, ProjectState>,
    projectId: string,
    storagePath: string
  ): Promise<void> {
    const state = projectStates.get(projectId);
    if (!state) return;

    state.lastIndexedAt = new Date();
    state.updatedAt = new Date();

    // 注意：索引统计信息现在由ProjectStateManager直接管理，无需从索引服务获取

    projectStates.set(projectId, state);

    this.logger.debug(`Updated last indexed time for ${projectId}`);
  }

  /**
   * 更新项目元数据
   */
  async updateProjectMetadata(
    projectStates: Map<string, ProjectState>,
    projectId: string,
    metadata: Record<string, any>,
    storagePath: string
  ): Promise<void> {
    const state = projectStates.get(projectId);
    if (!state) return;

    state.metadata = { ...state.metadata, ...metadata };
    state.updatedAt = new Date();

    projectStates.set(projectId, state);

    this.logger.debug(`Updated metadata for ${projectId}`);
  }

  /**
   * 更新项目集合信息
   */
  private async updateProjectCollectionInfo(projectStates: Map<string, ProjectState>, projectId: string): Promise<void> {
    const state = projectStates.get(projectId);
    if (!state) return;

    await this.updateProjectCollectionInfoWithRetry(projectStates, projectId);
  }

  /**
   * 带重试机制的集合信息更新
   */
  private async updateProjectCollectionInfoWithRetry(projectStates: Map<string, ProjectState>, projectId: string, maxRetries: number = 3): Promise<void> {
    const state = projectStates.get(projectId);
    if (!state) return;

    for (let attempt = 0; attempt < maxRetries; attempt++) {
      try {
        const projectPath = this.projectIdManager.getProjectPath(projectId);
        if (!projectPath) return;

        const collectionInfo = await this.qdrantService.getCollectionInfoForProject(projectPath
);
        if (collectionInfo) {
          state.collectionInfo = {
            name: collectionInfo.name,
            vectorsCount: collectionInfo.pointsCount,
            status: collectionInfo.status
          };
        }

        // 成功则退出重试循环
        return;
      } catch (error) {
        if (attempt === maxRetries - 1) {
          // 最后一次尝试仍然失败，记录警告
          this.logger.warn(`Failed to update collection info for ${projectId} after ${maxRetries} attempts`, { error });
          return;
        }

        // 指数退避等待
        const waitTime = Math.pow(2, attempt) * 1000;
        this.logger.debug(`Retrying collection info update for ${projectId}, attempt ${attempt + 1}/${maxRetries}, waiting ${waitTime}ms`);
        await new Promise(resolve => setTimeout(resolve, waitTime));
      }
    }
  }

  /**
   * 刷新项目状态
   */
  async refreshProjectState(
    projectStates: Map<string, ProjectState>,
    projectId: string,
    storagePath: string
  ): Promise<ProjectState | null> {
    try {
      const state = projectStates.get(projectId);
      if (!state) {
        return null;
      }

      // 更新集合信息
      await this.updateProjectCollectionInfo(projectStates, projectId);

      // 更新时间戳
      state.updatedAt = new Date();

      return state;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to refresh project state: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'CoreStateService', operation: 'refreshProjectState', projectId }
      );
      return null;
    }
  }

  /**
   * 刷新所有项目状态
   */
  async refreshAllProjectStates(
    projectStates: Map<string, ProjectState>,
    storagePath: string
  ): Promise<void> {
    try {
      const projectIds = Array.from(projectStates.keys());

      for (const projectId of projectIds) {
        await this.refreshProjectState(projectStates, projectId, storagePath);
      }

      this.logger.info('Refreshed all project states');
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to refresh all project states: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'CoreStateService', operation: 'refreshAllProjectStates' }
      );
      throw error;
    }
  }

  /**
   * 清理无效的项目状态
   */
  async cleanupInvalidStates(
    projectStates: Map<string, ProjectState>,
    storagePath: string
  ): Promise<number> {
    try {
      const initialCount = projectStates.size;
      const validStates: ProjectState[] = [];

      for (const state of projectStates.values()) {
        const isValid = await ProjectStateValidator.isProjectStateValid(state);
        if (isValid) {
          validStates.push(state);
        } else {
          this.logger.info(`Removing invalid project state: ${state.projectId} at ${state.projectPath}`);
        }
      }

      // 重建状态映射
      projectStates.clear();
      validStates.forEach(state => projectStates.set(state.projectId, state));

      const removedCount = initialCount - projectStates.size;
      this.logger.info(`Cleaned up ${removedCount} invalid project states, ${projectStates.size} states remaining`);

      return removedCount;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to cleanup invalid project states: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'CoreStateService', operation: 'cleanupInvalidStates' }
      );
      throw error;
    }
  }
}