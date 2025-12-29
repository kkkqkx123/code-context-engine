import { injectable, inject } from 'inversify';
import { TYPES } from '../../../types';
import { LoggerService } from '../../../utils/LoggerService';
import { SqliteDatabaseService } from '../SqliteDatabaseService';
import fs from 'fs';
import path from 'path';

export interface MigrationResult {
  success: boolean;
  migratedCount: number;
  errorCount: number;
  errors: string[];
  duration: number;
}

export interface MigrationSummary {
  projectMappings: MigrationResult;
  projectStates: MigrationResult;
  totalMigrated: number;
  totalErrors: number;
  overallSuccess: boolean;
}

export interface ValidationResult {
  isValid: boolean;
  issues: string[];
  dataConsistency: {
    projects: number;
    fileStates: number;
    projectStatus: number;
    changeHistory: number;
  };
}

export interface RollbackResult {
  success: boolean;
  restoredFiles: string[];
  errors: string[];
}

@injectable()
export class JsonToSqliteMigrator {
  private logger: LoggerService;
  private sqliteService: SqliteDatabaseService;
  private dataDir: string;

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.SqliteDatabaseService) sqliteService: SqliteDatabaseService
  ) {
    this.logger = logger;
    this.sqliteService = sqliteService;
    this.dataDir = path.join(process.cwd(), 'data');
  }

  /**
   * 执行完整的数据迁移
   */
  async migrateAll(): Promise<MigrationSummary> {
    const startTime = Date.now();
    
    this.logger.info('Starting complete data migration from JSON to SQLite');

    try {
      // 1. 备份现有JSON文件
      await this.backupJsonFiles();

      // 2. 确保数据库表结构存在
      this.sqliteService.initializeTables();

      // 3. 迁移项目映射数据
      const projectMappingsResult = await this.migrateProjectMappings();

      // 4. 迁移项目状态数据
      const projectStatesResult = await this.migrateProjectStates();

      // 5. 验证迁移结果
      const validationResult = await this.validateMigration();

      const duration = Date.now() - startTime;

      const summary: MigrationSummary = {
        projectMappings: projectMappingsResult,
        projectStates: projectStatesResult,
        totalMigrated: projectMappingsResult.migratedCount + projectStatesResult.migratedCount,
        totalErrors: projectMappingsResult.errorCount + projectStatesResult.errorCount,
        overallSuccess: projectMappingsResult.success && projectStatesResult.success && validationResult.isValid
      };

      this.logMigrationSummary(summary, duration);

      return summary;

    } catch (error) {
      this.logger.error('Migration failed', {
        error: (error as Error).message,
        stack: (error as Error).stack
      });
      throw error;
    }
  }

  /**
   * 迁移项目映射数据
   */
  async migrateProjectMappings(): Promise<MigrationResult> {
    const startTime = Date.now();
    const result: MigrationResult = {
      success: true,
      migratedCount: 0,
      errorCount: 0,
      errors: [],
      duration: 0
    };

    try {
      const projectMappingPath = path.join(this.dataDir, 'project-mapping.json');
      
      if (!fs.existsSync(projectMappingPath)) {
        this.logger.warn('Project mapping file not found, skipping migration');
        return result;
      }

      const projectMappings = JSON.parse(fs.readFileSync(projectMappingPath, 'utf-8'));
      
      if (!Array.isArray(projectMappings)) {
        throw new Error('Invalid project mapping file format');
      }

      const insertStmt = this.sqliteService.prepare(`
        INSERT INTO projects (id, path, name, description, collection_name, space_name, created_at, updated_at, status, settings, metadata)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
      `);

      for (const mapping of projectMappings) {
        try {
          // 从映射中提取项目信息
          const projectPath = Object.keys(mapping.projectIdMap || {})[0];
          const projectId = mapping.projectIdMap?.[projectPath];
          
          if (!projectId || !projectPath) {
            throw new Error('Invalid project mapping structure');
          }

          const collectionName = mapping.collectionMap?.[projectId];
          const spaceName = mapping.spaceMap?.[projectId];
          const updateTime = mapping.projectUpdateTimes?.[projectId];

          insertStmt.run(
            projectId,
            projectPath,
            this.extractProjectName(projectPath),
            null, // description
            collectionName,
            spaceName,
            updateTime || new Date().toISOString(),
            updateTime || new Date().toISOString(),
            'active',
            JSON.stringify({}),
            JSON.stringify({})
          );
          result.migratedCount++;
        } catch (error) {
          result.errorCount++;
          result.errors.push(`Failed to migrate project mapping: ${(error as Error).message}`);
          this.logger.warn('Failed to migrate project mapping', {
            error: (error as Error).message
          });
        }
      }

      result.duration = Date.now() - startTime;
      result.success = result.errorCount === 0;

      this.logger.info('Project mappings migration completed', {
        migrated: result.migratedCount,
        errors: result.errorCount,
        duration: result.duration
      });

    } catch (error) {
      result.success = false;
      result.errors.push(`Migration failed: ${(error as Error).message}`);
      this.logger.error('Project mappings migration failed', {
        error: (error as Error).message
      });
    }

    return result;
  }

  /**
   * 迁移项目状态数据
   */
  async migrateProjectStates(): Promise<MigrationResult> {
    const startTime = Date.now();
    const result: MigrationResult = {
      success: true,
      migratedCount: 0,
      errorCount: 0,
      errors: [],
      duration: 0
    };

    try {
      const projectStatesPath = path.join(this.dataDir, 'project-states.json');
      
      if (!fs.existsSync(projectStatesPath)) {
        this.logger.warn('Project states file not found, skipping migration');
        return result;
      }

      const projectStates = JSON.parse(fs.readFileSync(projectStatesPath, 'utf-8'));
      
      if (!Array.isArray(projectStates)) {
        throw new Error('Invalid project states file format');
      }

      const insertStmt = this.sqliteService.prepare(`
        INSERT OR REPLACE INTO project_status 
        (project_id, vector_status, graph_status, indexing_progress, total_files, indexed_files, failed_files, last_updated)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      `);

      for (const state of projectStates) {
        try {
          insertStmt.run(
            state.projectId,
            JSON.stringify(state.vectorStatus || {}),
            JSON.stringify(state.graphStatus || {}),
            state.indexingProgress || 0,
            state.totalFiles || 0,
            state.indexedFiles || 0,
            state.failedFiles || 0,
            state.updatedAt || new Date().toISOString()
          );
          result.migratedCount++;
        } catch (error) {
          result.errorCount++;
          result.errors.push(`Failed to migrate project state ${state.projectId}: ${(error as Error).message}`);
          this.logger.warn('Failed to migrate project state', {
            projectId: state.projectId,
            error: (error as Error).message
          });
        }
      }

      result.duration = Date.now() - startTime;
      result.success = result.errorCount === 0;

      this.logger.info('Project states migration completed', {
        migrated: result.migratedCount,
        errors: result.errorCount,
        duration: result.duration
      });

    } catch (error) {
      result.success = false;
      result.errors.push(`Migration failed: ${(error as Error).message}`);
      this.logger.error('Project states migration failed', {
        error: (error as Error).message
      });
    }

    return result;
  }

  /**
   * 验证迁移结果
   */
  async validateMigration(): Promise<ValidationResult> {
    const result: ValidationResult = {
      isValid: true,
      issues: [],
      dataConsistency: {
        projects: 0,
        fileStates: 0,
        projectStatus: 0,
        changeHistory: 0
      }
    };

    try {
      // 验证项目表数据
      const projectsStmt = this.sqliteService.prepare('SELECT COUNT(*) as count FROM projects');
      const projectsCount = (projectsStmt.get() as { count: number }).count;
      result.dataConsistency.projects = projectsCount;

      // 验证项目状态表数据
      const statusStmt = this.sqliteService.prepare('SELECT COUNT(*) as count FROM project_status');
      const statusCount = (statusStmt.get() as { count: number }).count;
      result.dataConsistency.projectStatus = statusCount;

      // 验证文件索引状态表数据
      const fileStatesStmt = this.sqliteService.prepare('SELECT COUNT(*) as count FROM file_index_states');
      const fileStatesCount = (fileStatesStmt.get() as { count: number }).count;
      result.dataConsistency.fileStates = fileStatesCount;

      // 验证变更历史表数据
      const changeHistoryStmt = this.sqliteService.prepare('SELECT COUNT(*) as count FROM file_change_history');
      const changeHistoryCount = (changeHistoryStmt.get() as { count: number }).count;
      result.dataConsistency.changeHistory = changeHistoryCount;

      // 检查数据一致性
      if (projectsCount === 0 && this.hasJsonProjectData()) {
        result.issues.push('No projects migrated despite existing JSON data');
        result.isValid = false;
      }

      if (statusCount === 0 && this.hasJsonStateData()) {
        result.issues.push('No project states migrated despite existing JSON data');
        result.isValid = false;
      }

      // 检查外键约束
      const foreignKeyCheck = this.sqliteService.prepare('PRAGMA foreign_key_check');
      const foreignKeyIssues = foreignKeyCheck.all() as any[];
      if (foreignKeyIssues.length > 0) {
        result.issues.push(`Foreign key constraint violations: ${foreignKeyIssues.length}`);
        result.isValid = false;
      }

      this.logger.info('Migration validation completed', {
        isValid: result.isValid,
        issues: result.issues.length,
        dataConsistency: result.dataConsistency
      });

    } catch (error) {
      result.isValid = false;
      result.issues.push(`Validation failed: ${(error as Error).message}`);
      this.logger.error('Migration validation failed', {
        error: (error as Error).message
      });
    }

    return result;
  }

  /**
   * 回滚迁移
   */
  async rollback(): Promise<RollbackResult> {
    const result: RollbackResult = {
      success: true,
      restoredFiles: [],
      errors: []
    };

    try {
      this.logger.info('Starting migration rollback');

      // 恢复备份的JSON文件
      const backupDir = path.join(this.dataDir, 'backup', 'migration');
      
      if (!fs.existsSync(backupDir)) {
        throw new Error('Backup directory not found, cannot rollback');
      }

      const filesToRestore = [
        'project-mapping.json',
        'project-states.json'
      ];

      for (const fileName of filesToRestore) {
        const backupPath = path.join(backupDir, fileName);
        const originalPath = path.join(this.dataDir, fileName);

        if (fs.existsSync(backupPath)) {
          fs.copyFileSync(backupPath, originalPath);
          result.restoredFiles.push(fileName);
          this.logger.info(`Restored file: ${fileName}`);
        }
      }

      // 清空SQLite表（可选）
      if (result.restoredFiles.length > 0) {
        this.sqliteService.exec('DELETE FROM projects');
        this.sqliteService.exec('DELETE FROM project_status');
        this.sqliteService.exec('DELETE FROM file_index_states');
        this.sqliteService.exec('DELETE FROM file_change_history');
        this.logger.info('Cleared SQLite tables');
      }

      this.logger.info('Migration rollback completed', {
        restoredFiles: result.restoredFiles.length,
        success: result.success
      });

    } catch (error) {
      result.success = false;
      result.errors.push(`Rollback failed: ${(error as Error).message}`);
      this.logger.error('Migration rollback failed', {
        error: (error as Error).message
      });
    }

    return result;
  }

  /**
   * 备份JSON文件
   */
  private async backupJsonFiles(): Promise<void> {
    const backupDir = path.join(this.dataDir, 'backup', 'migration');
    
    if (!fs.existsSync(backupDir)) {
      fs.mkdirSync(backupDir, { recursive: true });
    }

    const filesToBackup = [
      'project-mapping.json',
      'project-states.json'
    ];

    for (const fileName of filesToBackup) {
      const originalPath = path.join(this.dataDir, fileName);
      const backupPath = path.join(backupDir, fileName);

      if (fs.existsSync(originalPath)) {
        fs.copyFileSync(originalPath, backupPath);
        this.logger.info(`Backed up file: ${fileName}`);
      }
    }
  }

  /**
   * 检查是否存在JSON项目数据
   */
  private hasJsonProjectData(): boolean {
    const projectMappingPath = path.join(this.dataDir, 'project-mapping.json');
    if (!fs.existsSync(projectMappingPath)) {
      return false;
    }

    try {
      const data = JSON.parse(fs.readFileSync(projectMappingPath, 'utf-8'));
      return Array.isArray(data) && data.length > 0;
    } catch {
      return false;
    }
  }

  /**
   * 检查是否存在JSON状态数据
   */
  private hasJsonStateData(): boolean {
    const projectStatesPath = path.join(this.dataDir, 'project-states.json');
    if (!fs.existsSync(projectStatesPath)) {
      return false;
    }

    try {
      const data = JSON.parse(fs.readFileSync(projectStatesPath, 'utf-8'));
      return Array.isArray(data) && data.length > 0;
    } catch {
      return false;
    }
  }

  /**
   * 提取项目名称
   */
  private extractProjectName(projectPath: string): string {
    return projectPath.split(/[/\\]/).pop() || 'unknown';
  }

  /**
   * 记录迁移摘要
   */
  private logMigrationSummary(summary: MigrationSummary, duration: number): void {
    this.logger.info('Migration summary', {
      duration,
      totalMigrated: summary.totalMigrated,
      totalErrors: summary.totalErrors,
      overallSuccess: summary.overallSuccess,
      projectMappings: {
        migrated: summary.projectMappings.migratedCount,
        errors: summary.projectMappings.errorCount
      },
      projectStates: {
        migrated: summary.projectStates.migratedCount,
        errors: summary.projectStates.errorCount
      }
    });

    if (!summary.overallSuccess) {
      this.logger.warn('Migration completed with errors', {
        errors: [...summary.projectMappings.errors, ...summary.projectStates.errors]
      });
    }
  }
}