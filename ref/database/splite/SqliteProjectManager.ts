import { injectable, inject } from 'inversify';
import { IProjectManager } from '../common/IDatabaseService';
import { EventListener } from '../../types';
import { TYPES } from '../../types';
import { SqliteDatabaseService } from './SqliteDatabaseService';
import { LoggerService } from '../../utils/LoggerService';
import { Subscription } from '../common/DatabaseEventTypes';

export interface Project {
  id: string;
  path: string;
  name?: string;
  description?: string;
  collection_name?: string;
  space_name?: string;
  created_at: Date;
  updated_at: Date;
  last_indexed_at?: Date;
  status: 'active' | 'inactive' | 'indexing' | 'error';
  settings?: any;
  metadata?: any;
}

export interface FileIndexState {
  id?: number;
  project_id: string;
  file_path: string;
  relative_path: string;
  content_hash: string;
  file_size?: number;
  last_modified: Date;
  last_indexed?: Date;
  indexing_version: number;
  chunk_count?: number;
  vector_count?: number;
  language?: string;
  file_type?: string;
  status: 'pending' | 'indexed' | 'failed';
  error_message?: string;
  metadata?: any;
  created_at: Date;
  updated_at: Date;
}

export interface ProjectStatus {
  project_id: string;
  vector_status: any;
  graph_status: any;
  indexing_progress: number;
  total_files: number;
  indexed_files: number;
  failed_files: number;
  last_updated: Date;
}

@injectable()
export class SqliteProjectManager implements IProjectManager {
  private sqliteService: SqliteDatabaseService;
  private logger: LoggerService;
  private eventListeners: Map<string, EventListener[]> = new Map();

  constructor(
    @inject(TYPES.SqliteDatabaseService) sqliteService: SqliteDatabaseService,
    @inject(TYPES.LoggerService) logger: LoggerService
  ) {
    this.sqliteService = sqliteService;
    this.logger = logger;
  }

  async createProjectSpace(projectPath: string, config?: any): Promise<boolean> {
    try {
      const project: Project = {
        id: this.generateProjectId(projectPath),
        path: projectPath,
        name: config?.name || this.extractProjectName(projectPath),
        description: config?.description,
        collection_name: config?.collectionName,
        space_name: config?.spaceName,
        created_at: new Date(),
        updated_at: new Date(),
        status: 'active',
        settings: config?.settings,
        metadata: config?.metadata
      };
      const stmt = this.sqliteService.prepare(`
       INSERT INTO projects (id, path, name, description, collection_name, space_name, created_at, updated_at, status, settings, metadata)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
     `);

      stmt.run(
        project.id,
        project.path,
        project.name,
        project.description,
        project.collection_name,
        project.space_name,
        project.created_at.toISOString(),
        project.updated_at.toISOString(),
        project.status,
        JSON.stringify(project.settings || {}),
        JSON.stringify(project.metadata || {})
      );

      this.emitEvent('space_created', { projectPath, projectId: project.id });
      return true;
    } catch (error) {
      // 检查是否是唯一约束错误
      const errorMessage = error instanceof Error ? error.message : String(error);
      const isUniqueConstraintError = errorMessage.includes('UNIQUE constraint failed') || errorMessage.includes('SQLITE_CONSTRAINT');

      if (isUniqueConstraintError) {
        // 如果是唯一约束错误，说明项目已存在，这可能不是真正的错误
        this.logger.warn(`Project already exists in SQLite (unique constraint), treating as success: ${projectPath}`, {
          projectId: this.generateProjectId(projectPath),
          error: errorMessage
        });
        return true; // 返回true，表示操作成功（项目已存在）
      } else {
        // 记录详细的错误信息，便于调试
        this.logger.error(`Failed to create project space in SQLite:`, {
          projectPath,
          projectId: this.generateProjectId(projectPath),
          error: error instanceof Error ? {
            message: error.message,
            stack: error.stack,
            name: error.name
          } : error,
          config,
          isUniqueConstraintError
        });

        this.emitEvent('error', error);
        return false;
      }
    }
  }

  async deleteProjectSpace(projectPath: string): Promise<boolean> {
    try {
      const project = await this.getProjectByPath(projectPath);
      if (!project) {
        return false;
      }

      const stmt = this.sqliteService.prepare('DELETE FROM projects WHERE id = ?');
      stmt.run(project.id);

      this.emitEvent('space_deleted', { projectPath, projectId: project.id });
      return true;
    } catch (error) {
      this.emitEvent('error', error);
      return false;
    }
  }

  async getProjectSpaceInfo(projectPath: string): Promise<any> {
    const project = await this.getProjectByPath(projectPath);
    if (!project) {
      return null;
    }

    const statusStmt = this.sqliteService.prepare('SELECT * FROM project_status WHERE project_id = ?');
    const status = statusStmt.get(project.id) as ProjectStatus | undefined;

    const fileStatesStmt = this.sqliteService.prepare('SELECT COUNT(*) as count FROM file_index_states WHERE project_id = ? AND status = ?');
    const indexedFiles = fileStatesStmt.get(project.id, 'indexed') as { count: number };
    const pendingFiles = fileStatesStmt.get(project.id, 'pending') as { count: number };
    const failedFiles = fileStatesStmt.get(project.id, 'failed') as { count: number };

    return {
      project,
      status: status || {
        project_id: project.id,
        vector_status: {},
        graph_status: {},
        indexing_progress: 0,
        total_files: 0,
        indexed_files: 0,
        failed_files: 0,
        last_updated: new Date()
      },
      fileStats: {
        indexed: indexedFiles.count,
        pending: pendingFiles.count,
        failed: failedFiles.count,
        total: indexedFiles.count + pendingFiles.count + failedFiles.count
      }
    };
  }

  async clearProjectSpace(projectPath: string): Promise<boolean> {
    try {
      const project = await this.getProjectByPath(projectPath);
      if (!project) {
        return false;
      }

      // 删除项目相关的所有数据
      const deleteFileStates = this.sqliteService.prepare('DELETE FROM file_index_states WHERE project_id = ?');
      const deleteProjectStatus = this.sqliteService.prepare('DELETE FROM project_status WHERE project_id = ?');
      const deleteChangeHistory = this.sqliteService.prepare('DELETE FROM file_change_history WHERE project_id = ?');

      this.sqliteService.transaction(() => {
        deleteFileStates.run(project.id);
        deleteProjectStatus.run(project.id);
        deleteChangeHistory.run(project.id);
      });

      this.emitEvent('space_cleared', { projectPath, projectId: project.id });
      return true;
    } catch (error) {
      this.emitEvent('error', error);
      return false;
    }
  }

  async listProjectSpaces(): Promise<any[]> {
    const stmt = this.sqliteService.prepare('SELECT * FROM projects ORDER BY created_at DESC');
    const projects = stmt.all() as Project[];

    return projects.map(project => ({
      id: project.id,
      path: project.path,
      name: project.name,
      status: project.status,
      created_at: project.created_at,
      last_indexed_at: project.last_indexed_at
    }));
  }

  async insertProjectData(projectPath: string, data: any): Promise<boolean> {
    // 实现项目数据插入逻辑
    this.emitEvent('data_inserted', { projectPath, data });
    return true;
  }

  async updateProjectData(projectPath: string, id: string, data: any): Promise<boolean> {
    // 实现项目数据更新逻辑
    this.emitEvent('data_updated', { projectPath, id, data });
    return true;
  }

  async deleteProjectData(projectPath: string, id: string): Promise<boolean> {
    // 实现项目数据删除逻辑
    this.emitEvent('data_deleted', { projectPath, id });
    return true;
  }

  async searchProjectData(projectPath: string, query: any): Promise<any[]> {
    // 实现项目数据搜索逻辑
    return [];
  }

  async getProjectDataById(projectPath: string, id: string): Promise<any> {
    // 实现根据ID获取项目数据逻辑
    return null;
  }

  subscribe(eventType: string, listener: EventListener): Subscription {
    if (!this.eventListeners.has(eventType)) {
      this.eventListeners.set(eventType, []);
    }
    this.eventListeners.get(eventType)!.push(listener);

    const subscription: Subscription = {
      id: `${eventType}_${Date.now()}_${Math.random()}`,
      eventType,
      handler: listener,
      unsubscribe: () => {
        const listeners = this.eventListeners.get(eventType);
        if (listeners) {
          const index = listeners.indexOf(listener);
          if (index > -1) {
            listeners.splice(index, 1);
          }
        }
      }
    };

    return subscription;
  }

  // SQLite特定的项目管理方法
  async getProjectById(projectId: string): Promise<Project | null> {
    const stmt = this.sqliteService.prepare('SELECT * FROM projects WHERE id = ?');
    const result = stmt.get(projectId) as Project | undefined;
    return result || null;
  }

  async updateProjectStatus(projectId: string, status: ProjectStatus): Promise<boolean> {
    try {
      const stmt = this.sqliteService.prepare(`
        INSERT OR REPLACE INTO project_status 
        (project_id, vector_status, graph_status, indexing_progress, total_files, indexed_files, failed_files, last_updated)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      `);

      stmt.run(
        status.project_id,
        JSON.stringify(status.vector_status),
        JSON.stringify(status.graph_status),
        status.indexing_progress,
        status.total_files,
        status.indexed_files,
        status.failed_files,
        status.last_updated.toISOString()
      );

      return true;
    } catch (error) {
      this.emitEvent('error', error);
      return false;
    }
  }

  async getFileIndexStates(projectId: string): Promise<FileIndexState[]> {
    const stmt = this.sqliteService.prepare('SELECT * FROM file_index_states WHERE project_id = ? ORDER BY file_path');
    return stmt.all(projectId) as FileIndexState[];
  }

  private async getProjectByPath(projectPath: string): Promise<Project | null> {
    const stmt = this.sqliteService.prepare('SELECT * FROM projects WHERE path = ?');
    const result = stmt.get(projectPath) as Project | undefined;
    return result || null;
  }

  private generateProjectId(projectPath: string): string {
    // 使用更安全的哈希算法生成项目ID
    const crypto = require('crypto');
    const normalizedPath = projectPath.replace(/\\/g, '/'); // 统一路径分隔符
    const hash = crypto.createHash('sha256').update(normalizedPath).digest('hex');
    return hash.substring(0, 16); // 使用前16个字符作为项目ID
  }

  private extractProjectName(projectPath: string): string {
    return projectPath.split(/[/\\]/).filter(Boolean).pop() || 'unknown';
  }

  private emitEvent(eventType: string, data: any): void {
    const listeners = this.eventListeners.get(eventType);
    if (listeners) {
      listeners.forEach(listener => {
        try {
          listener(data);
        } catch (error) {
          console.error(`Error in event listener for ${eventType}:`, error);
        }
      });
    }
  }
}