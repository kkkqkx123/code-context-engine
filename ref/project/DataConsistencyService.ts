import { injectable, inject } from 'inversify';
import { TYPES } from '../../types';
import { LoggerService } from '../../utils/LoggerService';
import { ProjectIdManager } from '../../database/ProjectIdManager';
import { ProjectStateManager } from './ProjectStateManager';
import { SqliteProjectManager } from '../../database/splite/SqliteProjectManager';
import { SqliteStateManager } from '../../database/splite/SqliteStateManager';

@injectable()
export class DataConsistencyService {
  constructor(
    @inject(TYPES.LoggerService) private logger: LoggerService,
    @inject(TYPES.ProjectIdManager) private projectIdManager: ProjectIdManager,
    @inject(TYPES.ProjectStateManager) private projectStateManager: ProjectStateManager,
    @inject(TYPES.SqliteProjectManager) private sqliteProjectManager: SqliteProjectManager,
    @inject(TYPES.SqliteStateManager) private sqliteStateManager: SqliteStateManager
  ) {}

  /**
   * 同步项目数据到所有存储
   */
  async syncProjectData(projectId: string, projectPath: string): Promise<void> {
    try {
      // 同步项目ID映射到SQLite
      await this.syncProjectIdMapping(projectId, projectPath);
      
      // 同步项目状态到SQLite
      await this.syncProjectState(projectId);
      
      this.logger.info(`Successfully synced project data for ${projectId}`);
    } catch (error) {
      this.logger.error(`Failed to sync project data for ${projectId}`, error);
      throw error;
    }
  }

  /**
   * 同步项目ID映射
   */
  private async syncProjectIdMapping(projectId: string, projectPath: string): Promise<void> {
    try {
      // 确保ProjectIdManager中的数据同步到SQLite
      await this.projectIdManager['syncProjectToSQLite'](projectId, projectPath);
      this.logger.debug(`Synced project ID mapping for ${projectId}`);
    } catch (error) {
      this.logger.warn(`Failed to sync project ID mapping for ${projectId}`, error);
    }
  }

  /**
   * 同步项目状态
   */
  private async syncProjectState(projectId: string): Promise<void> {
    try {
      // 确保ProjectStateManager中的数据同步到SQLite
      const state = this.projectStateManager.getProjectState(projectId);
      if (state) {
        await this.sqliteStateManager.updateProjectStateFields(projectId, {
          vectorStatus: state.vectorStatus,
          graphStatus: state.graphStatus,
          indexingProgress: state.indexingProgress || 0,
          totalFiles: state.totalFiles || 0,
          indexedFiles: state.indexedFiles || 0,
          failedFiles: state.failedFiles || 0,
          lastUpdated: state.updatedAt,
          hotReload: state.hotReload
        });
        this.logger.debug(`Synced project state for ${projectId}`);
      }
    } catch (error) {
      this.logger.warn(`Failed to sync project state for ${projectId}`, error);
    }
  }

  /**
   * 同步所有项目数据
   */
  async syncAllProjectData(): Promise<void> {
    try {
      // 同步所有项目ID映射
      await this.projectIdManager['syncAllProjectsToSQLite']();
      
      // 同步所有项目状态
      await this.projectStateManager['saveProjectStatesToSQLite']();
      
      this.logger.info('Successfully synced all project data');
    } catch (error) {
      this.logger.error('Failed to sync all project data', error);
      throw error;
    }
  }
}