import { injectable, inject } from 'inversify';
import { TYPES } from '../../types';
import { SqliteDatabaseService } from './SqliteDatabaseService';
import { LoggerService } from '../../utils/LoggerService';

export interface HotReloadConfig {
  debounceInterval?: number;
  watchPatterns?: string[];
  ignorePatterns?: string[];
  maxFileSize?: number;
  errorHandling?: {
    maxRetries?: number;
    alertThreshold?: number;
    autoRecovery?: boolean;
  };
}

export interface HotReloadData {
  enabled: boolean;
  config?: HotReloadConfig;
  lastEnabled?: Date;
  lastDisabled?: Date;
  changesDetected?: number;
  errorsCount?: number;
}

export interface ProjectState {
  projectId: string;
  vectorStatus: any;
  graphStatus: any;
  indexingProgress: number;
  totalFiles: number;
  indexedFiles: number;
  failedFiles: number;
  lastUpdated: Date;
  // 新增热重载字段
  hotReload?: HotReloadData;
}

@injectable()
export class SqliteStateManager {
  constructor(
    @inject(TYPES.SqliteDatabaseService) private sqliteService: SqliteDatabaseService,
    @inject(TYPES.LoggerService) private logger: LoggerService
  ) {}

  /**
   * 检查热重载字段是否存在
   */
  private async checkHotReloadColumns(): Promise<boolean> {
    try {
      // 检查数据库是否已连接
      if (!this.sqliteService.isConnected()) {
        this.logger.warn('Database not connected, cannot check hot reload columns');
        return false;
      }
      
      const stmt = this.sqliteService.prepare(`
        PRAGMA table_info(project_status)
      `);
      
      const columns = stmt.all() as any[];
      const hotReloadColumns = [
        'hot_reload_enabled',
        'hot_reload_config',
        'hot_reload_last_enabled',
        'hot_reload_last_disabled',
        'hot_reload_changes_detected',
        'hot_reload_errors_count'
      ];
      
      return hotReloadColumns.every(col =>
        columns.some(column => column.name === col)
      );
    } catch (error) {
      this.logger.warn('Failed to check hot reload columns, will assume columns exist', error);
      // 在出现错误时，默认返回true，避免因为临时性问题导致功能不可用
      return true;
    }
  }

  /**
   * 保存项目状态
   */
  async saveProjectState(state: ProjectState): Promise<boolean> {
    try {
      const hasHotReloadColumns = await this.checkHotReloadColumns();
      
      if (hasHotReloadColumns && state.hotReload) {
        // 包含热重载数据的完整保存
        const stmt = this.sqliteService.prepare(`
          INSERT OR REPLACE INTO project_status 
          (project_id, vector_status, graph_status, indexing_progress, total_files, indexed_files, failed_files, last_updated,
           hot_reload_enabled, hot_reload_config, hot_reload_last_enabled, hot_reload_last_disabled, 
           hot_reload_changes_detected, hot_reload_errors_count)
          VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        `);
        
        const result = stmt.run(
          state.projectId,
          JSON.stringify(state.vectorStatus),
          JSON.stringify(state.graphStatus),
          state.indexingProgress || 0,
          state.totalFiles || 0,
          state.indexedFiles || 0,
          state.failedFiles || 0,
          state.lastUpdated.toISOString(),
          state.hotReload.enabled || false,
          state.hotReload.config ? JSON.stringify(state.hotReload.config) : null,
          state.hotReload.lastEnabled ? state.hotReload.lastEnabled.toISOString() : null,
          state.hotReload.lastDisabled ? state.hotReload.lastDisabled.toISOString() : null,
          state.hotReload.changesDetected || 0,
          state.hotReload.errorsCount || 0
        );
        
        return result.changes > 0;
      } else {
        // 原有的保存逻辑（向后兼容）
        const stmt = this.sqliteService.prepare(`
          INSERT OR REPLACE INTO project_status 
          (project_id, vector_status, graph_status, indexing_progress, total_files, indexed_files, failed_files, last_updated)
          VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        `);
        
        const result = stmt.run(
          state.projectId,
          JSON.stringify(state.vectorStatus),
          JSON.stringify(state.graphStatus),
          state.indexingProgress || 0,
          state.totalFiles || 0,
          state.indexedFiles || 0,
          state.failedFiles || 0,
          state.lastUpdated.toISOString()
        );
        
        return result.changes > 0;
      }
    } catch (error) {
      this.logger.error(`Failed to save project state: ${state.projectId}`, error);
      return false;
    }
  }

  /**
   * 获取项目状态
   */
  async getProjectState(projectId: string): Promise<ProjectState | null> {
    try {
      const hasHotReloadColumns = await this.checkHotReloadColumns();
      
      let query = `
        SELECT * FROM project_status WHERE project_id = ?
      `;
      
      const stmt = this.sqliteService.prepare(query);
      const result = stmt.get(projectId) as any;
      
      if (!result) {
        return null;
      }
      
      const projectState: ProjectState = {
        projectId: result.project_id,
        vectorStatus: JSON.parse(result.vector_status),
        graphStatus: JSON.parse(result.graph_status),
        indexingProgress: result.indexing_progress,
        totalFiles: result.total_files,
        indexedFiles: result.indexed_files,
        failedFiles: result.failed_files,
        lastUpdated: new Date(result.last_updated)
      };
      
      // 如果有热重载字段，添加热重载数据
      if (hasHotReloadColumns && result.hot_reload_enabled !== undefined) {
        projectState.hotReload = {
          enabled: Boolean(result.hot_reload_enabled),
          config: result.hot_reload_config ? JSON.parse(result.hot_reload_config) : undefined,
          lastEnabled: result.hot_reload_last_enabled ? new Date(result.hot_reload_last_enabled) : undefined,
          lastDisabled: result.hot_reload_last_disabled ? new Date(result.hot_reload_last_disabled) : undefined,
          changesDetected: result.hot_reload_changes_detected || 0,
          errorsCount: result.hot_reload_errors_count || 0
        };
      }
      
      return projectState;
    } catch (error) {
      this.logger.error(`Failed to get project state: ${projectId}`, error);
      return null;
    }
  }

  /**
   * 获取所有项目状态
   */
  async getAllProjectStates(): Promise<ProjectState[]> {
    try {
      const hasHotReloadColumns = await this.checkHotReloadColumns();
      
      const stmt = this.sqliteService.prepare(`
        SELECT * FROM project_status ORDER BY last_updated DESC
      `);
      
      const results = stmt.all() as any[];
      
      return results.map(result => {
        const projectState: ProjectState = {
          projectId: result.project_id,
          vectorStatus: JSON.parse(result.vector_status),
          graphStatus: JSON.parse(result.graph_status),
          indexingProgress: result.indexing_progress,
          totalFiles: result.total_files,
          indexedFiles: result.indexed_files,
          failedFiles: result.failed_files,
          lastUpdated: new Date(result.last_updated)
        };
        
        // 如果有热重载字段，添加热重载数据
        if (hasHotReloadColumns && result.hot_reload_enabled !== undefined) {
          projectState.hotReload = {
            enabled: Boolean(result.hot_reload_enabled),
            config: result.hot_reload_config ? JSON.parse(result.hot_reload_config) : undefined,
            lastEnabled: result.hot_reload_last_enabled ? new Date(result.hot_reload_last_enabled) : undefined,
            lastDisabled: result.hot_reload_last_disabled ? new Date(result.hot_reload_last_disabled) : undefined,
            changesDetected: result.hot_reload_changes_detected || 0,
            errorsCount: result.hot_reload_errors_count || 0
          };
        }
        
        return projectState;
      });
    } catch (error) {
      this.logger.error('Failed to get all project states', error);
      return [];
    }
  }

  /**
   * 删除项目状态
   */
  async deleteProjectState(projectId: string): Promise<boolean> {
    try {
      const stmt = this.sqliteService.prepare(`
        DELETE FROM project_status WHERE project_id = ?
      `);
      
      const result = stmt.run(projectId);
      return result.changes > 0;
    } catch (error) {
      this.logger.error(`Failed to delete project state: ${projectId}`, error);
      return false;
    }
  }

  /**
   * 批量保存项目状态
   */
  async batchSaveProjectStates(states: ProjectState[]): Promise<boolean> {
    if (states.length === 0) return true;
    
    try {
      const hasHotReloadColumns = await this.checkHotReloadColumns();
      
      this.sqliteService.transaction(() => {
        if (hasHotReloadColumns) {
          const stmt = this.sqliteService.prepare(`
            INSERT OR REPLACE INTO project_status 
            (project_id, vector_status, graph_status, indexing_progress, total_files, indexed_files, failed_files, last_updated,
             hot_reload_enabled, hot_reload_config, hot_reload_last_enabled, hot_reload_last_disabled, 
             hot_reload_changes_detected, hot_reload_errors_count)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
          `);
          
          for (const state of states) {
            stmt.run(
              state.projectId,
              JSON.stringify(state.vectorStatus),
              JSON.stringify(state.graphStatus),
              state.indexingProgress || 0,
              state.totalFiles || 0,
              state.indexedFiles || 0,
              state.failedFiles || 0,
              state.lastUpdated.toISOString(),
              state.hotReload?.enabled || false,
              state.hotReload?.config ? JSON.stringify(state.hotReload.config) : null,
              state.hotReload?.lastEnabled ? state.hotReload.lastEnabled.toISOString() : null,
              state.hotReload?.lastDisabled ? state.hotReload.lastDisabled.toISOString() : null,
              state.hotReload?.changesDetected || 0,
              state.hotReload?.errorsCount || 0
            );
          }
        } else {
          // 原有的批量保存逻辑（向后兼容）
          const stmt = this.sqliteService.prepare(`
            INSERT OR REPLACE INTO project_status 
            (project_id, vector_status, graph_status, indexing_progress, total_files, indexed_files, failed_files, last_updated)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
          `);
          
          for (const state of states) {
            stmt.run(
              state.projectId,
              JSON.stringify(state.vectorStatus),
              JSON.stringify(state.graphStatus),
              state.indexingProgress || 0,
              state.totalFiles || 0,
              state.indexedFiles || 0,
              state.failedFiles || 0,
              state.lastUpdated.toISOString()
            );
          }
        }
      });
      
      this.logger.info(`Batch saved ${states.length} project states`);
      return true;
    } catch (error) {
      this.logger.error('Failed to batch save project states', error);
      return false;
    }
  }

  /**
   * 更新项目状态的部分字段
   */
  async updateProjectStateFields(projectId: string, updates: Partial<ProjectState>): Promise<boolean> {
    const existingState = await this.getProjectState(projectId);
    if (!existingState) {
      return false;
    }
    
    const updatedState: ProjectState = {
      ...existingState,
      ...updates,
      projectId,
      lastUpdated: new Date()
    };
    
    return await this.saveProjectState(updatedState);
  }

  /**
   * 更新热重载数据
   */
  async updateHotReloadData(projectId: string, hotReloadData: Partial<HotReloadData>): Promise<boolean> {
    try {
      const hasHotReloadColumns = await this.checkHotReloadColumns();
      
      if (!hasHotReloadColumns) {
        this.logger.warn('Hot reload columns not available, skipping update');
        return false;
      }
      
      const existingState = await this.getProjectState(projectId);
      if (!existingState) {
        return false;
      }
      
      const updatedHotReload: HotReloadData = {
        enabled: hotReloadData.enabled ?? existingState.hotReload?.enabled ?? false,
        config: hotReloadData.config ?? existingState.hotReload?.config,
        lastEnabled: hotReloadData.lastEnabled ?? existingState.hotReload?.lastEnabled,
        lastDisabled: hotReloadData.lastDisabled ?? existingState.hotReload?.lastDisabled,
        changesDetected: hotReloadData.changesDetected ?? existingState.hotReload?.changesDetected ?? 0,
        errorsCount: hotReloadData.errorsCount ?? existingState.hotReload?.errorsCount ?? 0
      };
      
      const stmt = this.sqliteService.prepare(`
        UPDATE project_status SET
          hot_reload_enabled = ?,
          hot_reload_config = ?,
          hot_reload_last_enabled = ?,
          hot_reload_last_disabled = ?,
          hot_reload_changes_detected = ?,
          hot_reload_errors_count = ?,
          last_updated = ?
        WHERE project_id = ?
      `);
      
      const result = stmt.run(
        updatedHotReload.enabled || false,
        updatedHotReload.config ? JSON.stringify(updatedHotReload.config) : null,
        updatedHotReload.lastEnabled ? updatedHotReload.lastEnabled.toISOString() : null,
        updatedHotReload.lastDisabled ? updatedHotReload.lastDisabled.toISOString() : null,
        updatedHotReload.changesDetected || 0,
        updatedHotReload.errorsCount || 0,
        new Date().toISOString(),
        projectId
      );
      
      return result.changes > 0;
    } catch (error) {
      this.logger.error(`Failed to update hot reload data: ${projectId}`, error);
      return false;
    }
  }

  /**
   * 获取项目状态统计信息
   */
  async getProjectStateStats(): Promise<{
    totalProjects: number;
    activeProjects: number;
    indexingProjects: number;
    errorProjects: number;
    hotReloadEnabledProjects?: number;
  }> {
    try {
      const hasHotReloadColumns = await this.checkHotReloadColumns();
      
      let query = `
        SELECT 
          COUNT(*) as total_projects,
          SUM(CASE WHEN json_extract(vector_status, '$.status') = 'indexing' OR json_extract(graph_status, '$.status') = 'indexing' THEN 1 ELSE 0 END) as indexing_projects,
          SUM(CASE WHEN json_extract(vector_status, '$.status') = 'error' OR json_extract(graph_status, '$.status') = 'error' THEN 1 ELSE 0 END) as error_projects
      `;
      
      if (hasHotReloadColumns) {
        query += `, SUM(CASE WHEN hot_reload_enabled = 1 THEN 1 ELSE 0 END) as hot_reload_enabled_projects`;
      }
      
      query += ` FROM project_status`;
      
      const stmt = this.sqliteService.prepare(query);
      const result = stmt.get() as any;
      
      const stats: any = {
        totalProjects: result.total_projects,
        activeProjects: result.total_projects - result.error_projects,
        indexingProjects: result.indexing_projects,
        errorProjects: result.error_projects
      };
      
      if (hasHotReloadColumns) {
        stats.hotReloadEnabledProjects = result.hot_reload_enabled_projects;
      }
      
      return stats;
    } catch (error) {
      this.logger.error('Failed to get project state stats', error);
      return {
        totalProjects: 0,
        activeProjects: 0,
        indexingProjects: 0,
        errorProjects: 0
      };
    }
  }

  /**
   * 获取启用热重载的项目列表
   */
  async getHotReloadEnabledProjects(): Promise<string[]> {
    try {
      const hasHotReloadColumns = await this.checkHotReloadColumns();
      
      if (!hasHotReloadColumns) {
        return [];
      }
      
      const stmt = this.sqliteService.prepare(`
        SELECT project_id FROM project_status WHERE hot_reload_enabled = 1
      `);
      
      const results = stmt.all() as any[];
      return results.map(result => result.project_id);
    } catch (error) {
      this.logger.error('Failed to get hot reload enabled projects', error);
      return [];
    }
  }
}