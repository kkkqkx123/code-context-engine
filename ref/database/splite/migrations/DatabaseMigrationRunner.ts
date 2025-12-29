import { injectable, inject } from 'inversify';
import { TYPES } from '../../../types';
import { LoggerService } from '../../../utils/LoggerService';
import { SqliteDatabaseService } from '../SqliteDatabaseService';
import { MigrationManager } from './MigrationManager';

@injectable()
export class DatabaseMigrationRunner {
  constructor(
    @inject(TYPES.SqliteDatabaseService) private sqliteService: SqliteDatabaseService,
    @inject(TYPES.LoggerService) private logger: LoggerService,
    @inject(TYPES.MigrationManager) private migrationManager: MigrationManager
  ) {}

  /**
   * 运行数据库迁移
   */
  async runMigrations(): Promise<boolean> {
    try {
      this.logger.info('Starting database migration process...');
      
      // 确保数据库已连接
      if (!this.sqliteService.isConnected()) {
        this.sqliteService.connect();
      }
      
      // 获取迁移状态
      const status = await this.migrationManager.getMigrationStatus();
      this.logger.info(`Migration status: ${status.available} available, ${status.executed} executed, ${status.pending} pending`);
      
      if (status.pending > 0) {
        this.logger.info(`Running ${status.pending} pending migrations...`);
        
        // 运行迁移
        await this.migrationManager.runMigrations();
        
        this.logger.info('All migrations completed successfully');
        
        // 验证迁移结果
        const finalStatus = await this.migrationManager.getMigrationStatus();
        this.logger.info(`Final migration status: ${finalStatus.executed} executed, ${finalStatus.pending} pending`);
        
        return true;
      } else {
        this.logger.info('No pending migrations to run');
        return true;
      }
    } catch (error) {
      this.logger.error('Database migration failed', error);
      return false;
    }
  }

  /**
   * 检查迁移状态
   */
  async checkMigrationStatus(): Promise<{
    needsMigration: boolean;
    status: any;
  }> {
    try {
      if (!this.sqliteService.isConnected()) {
        this.sqliteService.connect();
      }
      
      const status = await this.migrationManager.getMigrationStatus();
      
      return {
        needsMigration: status.pending > 0,
        status
      };
    } catch (error) {
      this.logger.error('Failed to check migration status', error);
      return {
        needsMigration: false,
        status: null
      };
    }
  }

  /**
   * 初始化数据库并运行迁移
   */
  async initializeDatabase(): Promise<boolean> {
    try {
      this.logger.info('Initializing database...');
      
      // 连接数据库
      if (!this.sqliteService.isConnected()) {
        this.sqliteService.connect();
      }
      
      // 初始化表结构
      this.sqliteService.initializeTables();
      
      // 运行迁移
      const migrationSuccess = await this.runMigrations();
      
      if (migrationSuccess) {
        this.logger.info('Database initialization completed successfully');
        return true;
      } else {
        this.logger.error('Database initialization failed during migration');
        return false;
      }
    } catch (error) {
      this.logger.error('Database initialization failed', error);
      return false;
    }
  }

  /**
   * 验证数据库结构
   */
  async validateDatabaseStructure(): Promise<{
    isValid: boolean;
    issues: string[];
  }> {
    try {
      if (!this.sqliteService.isConnected()) {
        this.sqliteService.connect();
      }
      
      const issues: string[] = [];
      
      // 检查基本表是否存在
      const tables = ['projects', 'file_index_states', 'project_status', 'file_change_history'];
      
      for (const table of tables) {
        const stmt = this.sqliteService.prepare(`
          SELECT name FROM sqlite_master WHERE type='table' AND name=?
        `);
        const result = stmt.get(table);
        
        if (!result) {
          issues.push(`Missing table: ${table}`);
        }
      }
      
      // 检查热重载字段是否存在
      const hotReloadColumns = [
        'hot_reload_enabled',
        'hot_reload_config',
        'hot_reload_last_enabled',
        'hot_reload_last_disabled',
        'hot_reload_changes_detected',
        'hot_reload_errors_count'
      ];
      
      const stmt = this.sqliteService.prepare(`
        PRAGMA table_info(project_status)
      `);
      const columns = stmt.all() as any[];
      const existingColumns = columns.map(col => col.name);
      
      for (const column of hotReloadColumns) {
        if (!existingColumns.includes(column)) {
          issues.push(`Missing hot reload column: ${column}`);
        }
      }
      
      return {
        isValid: issues.length === 0,
        issues
      };
    } catch (error) {
      this.logger.error('Failed to validate database structure', error);
      return {
        isValid: false,
        issues: [`Validation error: ${error instanceof Error ? error.message : String(error)}`]
      };
    }
  }

  /**
   * 获取数据库统计信息
   */
  async getDatabaseStats(): Promise<{
    tables: number;
    migrations: number;
    lastMigration?: string;
  }> {
    try {
      if (!this.sqliteService.isConnected()) {
        this.sqliteService.connect();
      }
      
      // 获取表数量
      const stmt = this.sqliteService.prepare(`
        SELECT COUNT(*) as count FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'
      `);
      const tableResult = stmt.get() as { count: number };
      
      // 获取迁移状态
      const migrationStatus = await this.migrationManager.getMigrationStatus();
      
      // 获取最后执行的迁移
      const lastMigrationStmt = this.sqliteService.prepare(`
        SELECT filename FROM schema_migrations ORDER BY executed_at DESC LIMIT 1
      `);
      const lastMigrationResult = lastMigrationStmt.get() as { filename: string } | undefined;
      
      return {
        tables: tableResult.count,
        migrations: migrationStatus.executed,
        lastMigration: lastMigrationResult?.filename
      };
    } catch (error) {
      this.logger.error('Failed to get database stats', error);
      return {
        tables: 0,
        migrations: 0
      };
    }
  }
}