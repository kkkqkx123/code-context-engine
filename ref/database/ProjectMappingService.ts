import { injectable, inject } from 'inversify';
import { TYPES } from '../types';
import { SqliteDatabaseService } from './splite/SqliteDatabaseService';
import { LoggerService } from '../utils/LoggerService';
import { HashUtils } from '../utils/cache/HashUtils';

export interface ProjectMapping {
  projectId: string;
  projectPath: string;
  collectionName: string;
  spaceName: string;
  projectName: string;
  createdAt: Date;
  updatedAt: Date;
}

@injectable()
export class ProjectMappingService {
  private mappings: Map<string, ProjectMapping> = new Map(); // projectId -> ProjectMapping

  constructor(
    @inject(TYPES.SqliteDatabaseService) private dbService: SqliteDatabaseService,
    @inject(TYPES.LoggerService) private logger: LoggerService
  ) {
    this.initializeTable();
  }

  private async initializeTable(): Promise<void> {
    try {
      // 确保数据库已连接
      if (!this.dbService.isConnected()) {
        this.dbService.connect();
      }

      // 创建统一的项目映射表
      const createTableSQL = `
        CREATE TABLE IF NOT EXISTS unified_project_mapping (
          project_id TEXT PRIMARY KEY,
          project_path TEXT NOT NULL,
          collection_name TEXT NOT NULL,
          space_name TEXT NOT NULL,
          project_name TEXT NOT NULL,
          created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
          updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
      `;

      this.dbService.exec(createTableSQL);

      // 创建索引
      this.dbService.exec('CREATE INDEX IF NOT EXISTS idx_unified_mapping_path ON unified_project_mapping(project_path)');
      this.dbService.exec('CREATE INDEX IF NOT EXISTS idx_unified_mapping_name ON unified_project_mapping(project_name)');

      // 创建触发器
      this.dbService.exec(`
        CREATE TRIGGER IF NOT EXISTS update_unified_mapping_timestamp 
        AFTER UPDATE ON unified_project_mapping
        BEGIN
          UPDATE unified_project_mapping 
          SET updated_at = CURRENT_TIMESTAMP 
          WHERE project_id = NEW.project_id;
        END
      `);

      // 加载现有映射
      await this.loadMappings();

      this.logger.info('Unified mapping table initialized successfully');
    } catch (error) {
      this.logger.error('Failed to initialize unified mapping table', error);
      throw error;
    }
  }

  /**
   * 加载所有映射关系
   */
  private async loadMappings(): Promise<void> {
    try {
      const selectSQL = `
        SELECT project_id, project_path, collection_name, space_name, project_name, created_at, updated_at
        FROM unified_project_mapping
        ORDER BY updated_at DESC
      `;

      const results = this.dbService.prepare(selectSQL).all() as any[];

      this.mappings = new Map();

      for (const row of results) {
        const mapping: ProjectMapping = {
          projectId: row.project_id,
          projectPath: row.project_path,
          collectionName: row.collection_name,
          spaceName: row.space_name,
          projectName: row.project_name,
          createdAt: new Date(row.created_at),
          updatedAt: new Date(row.updated_at)
        };

        this.mappings.set(mapping.projectId, mapping);
      }

      this.logger.info(`Loaded ${this.mappings.size} project mappings`);
    } catch (error) {
      this.logger.error('Failed to load project mappings', error);
      throw error;
    }
  }

  /**
   * 创建或更新项目映射
   */
  async createOrUpdateMapping(
    projectPath: string,
    collectionName: string,
    spaceName: string
  ): Promise<ProjectMapping> {
    try {
      // 生成项目ID
      const projectId = await this.generateProjectId(projectPath);

      // 提取项目名称
      const projectName = this.extractProjectName(projectPath);

      // 创建映射对象
      const mapping: ProjectMapping = {
        projectId,
        projectPath,
        collectionName,
        spaceName,
        projectName,
        createdAt: new Date(),
        updatedAt: new Date()
      };

      // 保存到内存映射
      this.mappings.set(projectId, mapping);

      // 保存到数据库
      const insertSQL = `
        INSERT OR REPLACE INTO unified_project_mapping 
        (project_id, project_path, collection_name, space_name, project_name, updated_at)
        VALUES (?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
      `;

      this.dbService.prepare(insertSQL).run(
        mapping.projectId,
        mapping.projectPath,
        mapping.collectionName,
        mapping.spaceName,
        mapping.projectName
      );

      this.logger.debug(`Saved unified mapping for project: ${projectId}`);

      return mapping;
    } catch (error) {
      this.logger.error(`Failed to create or update mapping for path: ${projectPath}`, error);
      throw error;
    }
  }

  /**
   * 根据项目ID获取映射
   */
  getMappingById(projectId: string): ProjectMapping | null {
    return this.mappings.get(projectId) || null;
  }

  /**
   * 根据项目路径获取映射
   */
  getMappingByPath(projectPath: string): ProjectMapping | null {
    // 标准化路径以确保匹配
    const normalizedPath = HashUtils.deepNormalizePath(projectPath);

    for (const mapping of this.mappings.values()) {
      if (HashUtils.arePathsEqual(mapping.projectPath, normalizedPath)) {
        return mapping;
      }
    }

    return null;
  }

  /**
   * 获取所有映射
   */
  getAllMappings(): ProjectMapping[] {
    return Array.from(this.mappings.values());
  }

  /**
   * 删除映射
   */
  async deleteMapping(projectId: string): Promise<boolean> {
    try {
      // 从内存中删除
      const deleted = this.mappings.delete(projectId);

      if (deleted) {
        // 从数据库中删除
        const deleteSQL = `DELETE FROM unified_project_mapping WHERE project_id = ?`;
        this.dbService.prepare(deleteSQL).run(projectId);

        this.logger.debug(`Deleted mapping for project: ${projectId}`);
      }

      return deleted;
    } catch (error) {
      this.logger.error(`Failed to delete mapping for project: ${projectId}`, error);
      throw error;
    }
  }

  /**
   * 生成项目ID
   */
  private async generateProjectId(projectPath: string): Promise<string> {
    // 使用更安全的哈希算法生成项目ID
    const normalizedPath = HashUtils.normalizePath(projectPath);
    const directoryHash = await HashUtils.calculateDirectoryHash(normalizedPath);
    return directoryHash.hash.substring(0, 16); // 使用前16个字符作为项目ID
  }

  /**
   * 提取项目名称
   */
  private extractProjectName(projectPath: string): string {
    if (!projectPath) return 'unknown';
    // 处理各种路径分隔符，包括Windows和Unix风格
    return projectPath.split(/[/\\]/).filter(Boolean).pop() || 'unknown';
  }
}