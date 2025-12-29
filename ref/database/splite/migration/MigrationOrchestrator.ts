import { injectable, inject } from 'inversify';
import { TYPES } from '../../../types';
import { LoggerService } from '../../../utils/LoggerService';
import { JsonToSqliteMigrator, MigrationSummary, ValidationResult } from './JsonToSqliteMigrator';
import { MigrationConfig, MigrationOptions, MigrationStatus, MigrationReport, MigrationProgress, MigrationError } from './MigrationTypes';
import fs from 'fs';
import path from 'path';

@injectable()
export class MigrationOrchestrator {
  private migrator: JsonToSqliteMigrator;
  private logger: LoggerService;
  private migrationEnabled: boolean = false;
  private config: MigrationConfig;

  constructor(
    @inject(TYPES.JsonToSqliteMigrator) migrator: JsonToSqliteMigrator,
    @inject(TYPES.LoggerService) logger: LoggerService
  ) {
    this.migrator = migrator;
    this.logger = logger;
    this.config = this.getDefaultConfig();
  }

  /**
   * 执行渐进式迁移
   */
  async executeProgressiveMigration(options?: MigrationOptions): Promise<MigrationReport> {
    this.logger.info('Starting progressive migration');

    try {
      // 应用配置
      if (options?.config) {
        this.updateConfig(options.config);
      }

      // 1. 检查是否需要迁移
      if (!this.shouldMigrate()) {
        this.logger.info('No migration needed, SQLite data is up to date');
        return this.createEmptyReport();
      }

      // 2. 报告进度
      this.reportProgress(options, {
        stage: 'initialization',
        current: 1,
        total: 4,
        percentage: 25,
        message: 'Starting migration process'
      });

      // 3. 执行完整迁移
      const summary = await this.migrator.migrateAll();

      // 4. 报告进度
      this.reportProgress(options, {
        stage: 'migration',
        current: 2,
        total: 4,
        percentage: 50,
        message: 'Migration completed, starting validation'
      });

      // 5. 验证迁移结果
      const validationResult = await this.migrator.validateMigration();

      // 6. 报告进度
      this.reportProgress(options, {
        stage: 'validation',
        current: 3,
        total: 4,
        percentage: 75,
        message: 'Validation completed, generating report'
      });

      // 7. 启用SQLite数据访问
      if (summary.overallSuccess && validationResult.isValid) {
        this.enableSqliteAccess();
      } else if (this.config.rollbackOnError) {
        // 回滚迁移
        this.logger.warn('Migration failed, initiating rollback');
        await this.migrator.rollback();
        this.reportError(options, {
          stage: 'rollback',
          error: 'Migration failed, rolled back to JSON data',
          recoverable: true
        });
      }

      // 8. 生成迁移报告
      const report = await this.generateMigrationReport(summary, validationResult);

      // 9. 报告完成
      this.reportProgress(options, {
        stage: 'completion',
        current: 4,
        total: 4,
        percentage: 100,
        message: 'Migration process completed'
      });

      this.logger.info('Progressive migration completed', {
        success: summary.overallSuccess,
        totalMigrated: summary.totalMigrated,
        totalErrors: summary.totalErrors
      });

      return report;

    } catch (error) {
      this.reportError(options, {
        stage: 'orchestration',
        error: (error as Error).message,
        recoverable: false
      });

      this.logger.error('Migration orchestration failed', {
        error: (error as Error).message,
        stack: (error as Error).stack
      });

      throw error;
    }
  }

  /**
   * 检查是否需要迁移
   */
  private shouldMigrate(): boolean {
    // 检查SQLite数据库是否存在且包含数据
    const dbExists = fs.existsSync('./data/code-search-helper.db');
    
    if (!dbExists) {
      return true; // 数据库文件不存在，需要迁移
    }

    // 检查JSON文件是否存在且比数据库新
    const jsonFilesExist = this.hasJsonDataFiles();
    if (!jsonFilesExist) {
      return false; // 没有JSON数据文件，不需要迁移
    }

    // 检查数据库是否为空
    return this.isDatabaseEmpty();
  }

  /**
   * 检查是否存在JSON数据文件
   */
  private hasJsonDataFiles(): boolean {
    const projectMappingPath = './data/project-mapping.json';
    const projectStatesPath = './data/project-states.json';
    
    return fs.existsSync(projectMappingPath) || fs.existsSync(projectStatesPath);
  }

  /**
   * 检查数据库是否为空
   */
  private isDatabaseEmpty(): boolean {
    try {
      // 这里需要访问SQLite数据库来检查是否有数据
      // 暂时返回true表示需要迁移
      return true;
    } catch (error) {
      this.logger.warn('Failed to check database emptiness, assuming migration needed', {
        error: (error as Error).message
      });
      return true;
    }
  }

  /**
   * 启用SQLite数据访问
   */
  private enableSqliteAccess(): void {
    this.migrationEnabled = true;
    this.logger.info('SQLite data access enabled');
  }

  /**
   * 检查是否已启用SQLite数据访问
   */
  isMigrationEnabled(): boolean {
    return this.migrationEnabled;
  }

  /**
   * 获取迁移状态
   */
  getMigrationStatus(): MigrationStatus {
    return {
      enabled: this.migrationEnabled,
      lastMigration: this.getLastMigrationTime(),
      dataSource: this.migrationEnabled ? 'sqlite' : 'json'
    };
  }

  /**
   * 更新配置
   */
  updateConfig(newConfig: Partial<MigrationConfig>): void {
    this.config = {
      ...this.config,
      ...newConfig
    };
    this.logger.info('Migration configuration updated', this.config);
  }

  /**
   * 获取默认配置
   */
  private getDefaultConfig(): MigrationConfig {
    return {
      backupEnabled: true,
      backupPath: './data/backup/migration',
      validateBeforeMigration: true,
      validateAfterMigration: true,
      rollbackOnError: true,
      maxRetries: 3,
      retryDelay: 1000
    };
  }

  /**
   * 生成迁移报告
   */
  private async generateMigrationReport(summary: MigrationSummary, validation: ValidationResult): Promise<MigrationReport> {
    const startTime = Date.now();

    const consistencyChecks = await this.performConsistencyChecks();
    const performanceMetrics = this.calculatePerformanceMetrics(summary);
    const recommendations = this.generateRecommendations(summary, validation, consistencyChecks);

    const validationDuration = Date.now() - startTime;

    return {
      summary,
      validation,
      consistencyChecks,
      performanceMetrics: {
        ...performanceMetrics,
        validationDuration
      },
      recommendations
    };
  }

  /**
   * 执行一致性检查
   */
  private async performConsistencyChecks() {
    // 这里可以实现详细的一致性检查逻辑
    return [];
  }

  /**
   * 计算性能指标
   */
  private calculatePerformanceMetrics(summary: MigrationSummary) {
    return {
      totalDuration: summary.projectMappings.duration + summary.projectStates.duration,
      backupDuration: 0, // 可以从迁移结果中获取
      migrationDuration: Math.max(summary.projectMappings.duration, summary.projectStates.duration),
      validationDuration: 0
    };
  }

  /**
   * 生成建议
   */
  private generateRecommendations(summary: MigrationSummary, validation: ValidationResult, consistencyChecks: any[]): string[] {
    const recommendations: string[] = [];

    if (summary.totalErrors > 0) {
      recommendations.push('Review and fix migration errors before proceeding');
    }

    if (!validation.isValid) {
      recommendations.push('Address validation issues to ensure data integrity');
    }

    if (consistencyChecks.length > 0) {
      recommendations.push('Perform data consistency checks and resolve any issues');
    }

    if (summary.totalMigrated === 0) {
      recommendations.push('No data was migrated - check source data availability');
    }

    return recommendations;
  }

  /**
   * 报告进度
   */
  private reportProgress(options: MigrationOptions | undefined, progress: MigrationProgress): void {
    if (options?.onProgress) {
      try {
        options.onProgress(progress);
      } catch (error) {
        this.logger.warn('Error in progress callback', { error: (error as Error).message });
      }
    }
    this.logger.info(`Migration progress: ${progress.percentage}% - ${progress.message}`);
  }

  /**
   * 报告错误
   */
  private reportError(options: MigrationOptions | undefined, error: MigrationError): void {
    if (options?.onError) {
      try {
        options.onError(error);
      } catch (callbackError) {
        this.logger.warn('Error in error callback', { error: (callbackError as Error).message });
      }
    }
    this.logger.error(`Migration error in ${error.stage}: ${error.error}`);
  }

  /**
   * 创建空报告（当不需要迁移时）
   */
  private createEmptyReport(): MigrationReport {
    return {
      summary: {
        projectMappings: { success: true, migratedCount: 0, errorCount: 0, errors: [], duration: 0 },
        projectStates: { success: true, migratedCount: 0, errorCount: 0, errors: [], duration: 0 },
        totalMigrated: 0,
        totalErrors: 0,
        overallSuccess: true
      },
      validation: {
        isValid: true,
        issues: [],
        dataConsistency: { projects: 0, fileStates: 0, projectStatus: 0, changeHistory: 0 }
      },
      consistencyChecks: [],
      performanceMetrics: {
        totalDuration: 0,
        backupDuration: 0,
        migrationDuration: 0,
        validationDuration: 0
      },
      recommendations: ['No migration needed - SQLite data is up to date']
    };
  }

  /**
   * 获取最后迁移时间
   */
  private getLastMigrationTime(): Date | null {
    // 实现获取最后迁移时间的逻辑
    // 可以从文件时间戳或数据库记录中获取
    return null;
  }

  /**
   * 保存迁移状态
   */
  private saveMigrationStatus(status: MigrationStatus): void {
    try {
      const statusPath = path.join(this.config.backupPath, 'migration-status.json');
      const statusDir = path.dirname(statusPath);
      
      if (!fs.existsSync(statusDir)) {
        fs.mkdirSync(statusDir, { recursive: true });
      }

      fs.writeFileSync(statusPath, JSON.stringify(status, null, 2));
    } catch (error) {
      this.logger.warn('Failed to save migration status', {
        error: (error as Error).message
      });
    }
  }
}