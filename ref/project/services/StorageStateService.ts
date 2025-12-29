
import { injectable, inject } from 'inversify';
import { TYPES } from '../../../types';
import { LoggerService } from '../../../utils/LoggerService';
import { ProjectState, StorageStatus } from '../ProjectStateManager';

/**
 * 存储状态服务
 * 职责：存储状态管理、协调
 */
@injectable()
export class StorageStateService {
  constructor(
    @inject(TYPES.LoggerService) private logger: LoggerService
  ) {}

  /**
   * 更新向量存储状态
   */
 async updateVectorStatus(
   projectStates: Map<string, ProjectState>,
   projectId: string,
   status: Partial<StorageStatus>,
   storagePath?: string
 ): Promise<void> {
   const state = projectStates.get(projectId);
   if (!state) return;

   state.vectorStatus = {
     ...state.vectorStatus,
     ...status,
     lastUpdated: new Date()
   };

   // 更新主状态（基于向量状态）
   this.updateMainStatusBasedOnStorageStates(state);

   projectStates.set(projectId, state);

   this.logger.debug(`Updated vector status for ${projectId}: ${state.vectorStatus.status}`);
 }

  /**
   * 更新图存储状态
   */
  async updateGraphStatus(
    projectStates: Map<string, ProjectState>,
    projectId: string,
    status: Partial<StorageStatus>,
    storagePath?: string
  ): Promise<void> {
    const state = projectStates.get(projectId);
    if (!state) return;

    state.graphStatus = {
      ...state.graphStatus,
      ...status,
      lastUpdated: new Date()
    };

    // 更新主状态（基于图状态）
    this.updateMainStatusBasedOnStorageStates(state);

    projectStates.set(projectId, state);

    this.logger.debug(`Updated graph status for ${projectId}: ${state.graphStatus.status}`);
  }

  /**
   * 根据存储状态更新主状态
   */
  private updateMainStatusBasedOnStorageStates(state: ProjectState): void {
    const { vectorStatus, graphStatus } = state;

    // 如果任一存储正在索引中，主状态为 indexing
    if (vectorStatus.status === 'indexing' || graphStatus.status === 'indexing') {
      state.status = 'indexing';
    }
    // 如果任一存储出错，主状态为 error
    else if (vectorStatus.status === 'error' || graphStatus.status === 'error') {
      state.status = 'error';
    }
    // 如果向量存储完成且图存储完成或禁用，主状态为 active
    else if (vectorStatus.status === 'completed' && 
             (graphStatus.status === 'completed' || graphStatus.status === 'disabled')) {
      state.status = 'active';
    }
    // 如果向量存储待处理且图存储禁用，主状态为 inactive
    else if (vectorStatus.status === 'pending' && graphStatus.status === 'disabled') {
      state.status = 'inactive';
    }
    // 如果两个存储都待处理，主状态为 inactive
    else if (vectorStatus.status === 'pending' && graphStatus.status === 'pending') {
      state.status = 'inactive';
    }
    // 其他情况（部分完成），主状态为 active
    else {
      state.status = 'active';
    }

    state.updatedAt = new Date();
  }

  /**
   * 获取向量存储状态
   */
  getVectorStatus(projectStates: Map<string, ProjectState>, projectId: string): StorageStatus | null {
    const state = projectStates.get(projectId);
    return state ? state.vectorStatus : null;
  }

  /**
   * 获取图存储状态
   */
  getGraphStatus(projectStates: Map<string, ProjectState>, projectId: string): StorageStatus | null {
    const state = projectStates.get(projectId);
    return state ? state.graphStatus : null;
  }

  /**
   * 重置向量存储状态
   */
  async resetVectorStatus(
    projectStates: Map<string, ProjectState>,
    projectId: string,
    storagePath?: string
 ): Promise<void> {
    await this.updateVectorStatus(projectStates, projectId, {
      status: 'pending',
      progress: 0,
      processedFiles: 0,
      failedFiles: 0,
      error: undefined
    }, storagePath);
  }

  /**
   * 重置图存储状态
   */
  async resetGraphStatus(
    projectStates: Map<string, ProjectState>,
    projectId: string,
    storagePath?: string
 ): Promise<void> {
    await this.updateGraphStatus(projectStates, projectId, {
      status: 'pending',
      progress: 0,
      processedFiles: 0,
      failedFiles: 0,
      error: undefined
    }, storagePath);
  }

  /**
   * 开始向量索引
   */
  async startVectorIndexing(
    projectStates: Map<string, ProjectState>,
    projectId: string,
    totalFiles?: number,
    storagePath?: string
  ): Promise<void> {
    await this.updateVectorStatus(projectStates, projectId, {
      status: 'indexing',
      progress: 0,
      totalFiles,
      processedFiles: 0,
      failedFiles: 0,
      error: undefined
    }, storagePath);
  }
  /**
   * 开始图索引
   */
   async startGraphIndexing(
     projectStates: Map<string, ProjectState>,
     projectId: string,
     totalFiles?: number,
     storagePath?: string
   ): Promise<void> {
     await this.updateGraphStatus(projectStates, projectId, {
       status: 'indexing',
       progress: 0,
       totalFiles,
       processedFiles: 0,
       failedFiles: 0,
       error: undefined
     }, storagePath);
   }

  /**
   * 更新向量索引进度
   */
   async updateVectorIndexingProgress(
     projectStates: Map<string, ProjectState>,
     projectId: string,
     progress: number,
     processedFiles?: number,
     failedFiles?: number,
     storagePath?: string
   ): Promise<void> {
     await this.updateVectorStatus(projectStates, projectId, {
       progress,
       processedFiles,
       failedFiles
     }, storagePath);
   }

  /**
   * 更新图索引进度
   */
  async updateGraphIndexingProgress(
    projectStates: Map<string, ProjectState>,
    projectId: string,
    progress: number,
    processedFiles?: number,
    failedFiles?: number,
    storagePath?: string
  ): Promise<void> {
    await this.updateGraphStatus(projectStates, projectId, {
      progress,
      processedFiles,
      failedFiles
    }, storagePath);
  }

  /**
   * 完成向量索引
   */
  async completeVectorIndexing(
    projectStates: Map<string, ProjectState>,
    projectId: string,
    storagePath?: string
  ): Promise<void> {
    const state = projectStates.get(projectId);
    if (!state) return;

    await this.updateVectorStatus(projectStates, projectId, {
      status: 'completed',
      progress: 100,
      lastCompleted: new Date()
    }, storagePath);
  }

  /**
   * 完成图索引
   */
  async completeGraphIndexing(
    projectStates: Map<string, ProjectState>,
    projectId: string,
    storagePath?: string
  ): Promise<void> {
    const state = projectStates.get(projectId);
    if (!state) return;

    await this.updateGraphStatus(projectStates, projectId, {
      status: 'completed',
      progress: 100,
      lastCompleted: new Date()
    }, storagePath);
  }

  /**
   * 向量索引出错
   */
  async failVectorIndexing(
    projectStates: Map<string, ProjectState>,
    projectId: string,
    error: string,
    storagePath?: string
  ): Promise<void> {
    await this.updateVectorStatus(projectStates, projectId, {
      status: 'error',
      error
    }, storagePath);
  }

  /**
   * 图索引出错
   */
  async failGraphIndexing(
    projectStates: Map<string, ProjectState>,
    projectId: string,
    error: string,
    storagePath?: string
  ): Promise<void> {
    await this.updateGraphStatus(projectStates, projectId, {
      status: 'error',
      error
    }, storagePath);
  }
  /**
   * 设置图索引为禁用状态
   */
  async disableGraphIndexing(
    projectStates: Map<string, ProjectState>,
    projectId: string,
    storagePath?: string
  ): Promise<void> {
    await this.updateGraphStatus(projectStates, projectId, {
      status: 'disabled',
      progress: 0,
      processedFiles: 0,
      failedFiles: 0,
      error: undefined
    }, storagePath);
  }
}