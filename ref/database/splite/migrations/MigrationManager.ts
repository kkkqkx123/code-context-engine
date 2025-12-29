import { injectable, inject } from 'inversify';
import { TYPES } from '../../../types';
import { LoggerService } from '../../../utils/LoggerService';
import { SqliteDatabaseService } from '../SqliteDatabaseService';
import fs from 'fs';
import path from 'path';

export interface Migration {
  id: string;
  filename: string;
  sql: string;
  executed_at?: Date;
}

@injectable()
export class MigrationManager {
  private migrationsPath: string;
  private migrationsTable = 'schema_migrations';

  constructor(
    @inject(TYPES.SqliteDatabaseService) private sqliteService: SqliteDatabaseService,
    @inject(TYPES.LoggerService) private logger: LoggerService
  ) {
    this.migrationsPath = path.join(__dirname, '.');
  }

  /**
   * 初始化迁移系统
   */
  async initialize(): Promise<void> {
    try {
      // 创建迁移记录表
      this.sqliteService.exec(`
        CREATE TABLE IF NOT EXISTS ${this.migrationsTable} (
          id TEXT PRIMARY KEY,
          filename TEXT NOT NULL,
          executed_at DATETIME NOT NULL
        )
      `);

      this.logger.info('Migration system initialized');
    } catch (error) {
      this.logger.error('Failed to initialize migration system', error);
      throw error;
    }
  }

  /**
   * 获取所有可用的迁移文件
   */
  async getAvailableMigrations(): Promise<Migration[]> {
    try {
      const files = fs.readdirSync(this.migrationsPath)
        .filter(file => file.endsWith('.sql'))
        .sort(); // 按文件名排序确保执行顺序

      const migrations: Migration[] = [];

      for (const file of files) {
        const filePath = path.join(this.migrationsPath, file);
        const sql = fs.readFileSync(filePath, 'utf-8');
        const id = path.basename(file, '.sql');

        migrations.push({
          id,
          filename: file,
          sql
        });
      }

      return migrations;
    } catch (error) {
      this.logger.error('Failed to read migration files', error);
      throw error;
    }
  }

  /**
   * 获取已执行的迁移
   */
  private async getExecutedMigrations(): Promise<Migration[]> {
    try {
      const stmt = this.sqliteService.prepare(`
        SELECT * FROM ${this.migrationsTable} ORDER BY executed_at
      `);

      const results = stmt.all() as any[];

      return results.map(row => ({
        id: row.id,
        filename: row.filename,
        sql: '', // 已执行的迁移不需要SQL内容
        executed_at: new Date(row.executed_at)
      }));
    } catch (error) {
      this.logger.error('Failed to get executed migrations', error);
      throw error;
    }
  }

  /**
   * 执行单个迁移
   */
  private async executeMigration(migration: Migration): Promise<void> {
    try {
      this.logger.info(`Executing migration: ${migration.filename}`);

      // 执行迁移SQL
      this.sqliteService.exec(migration.sql);

      // 记录迁移执行
      const stmt = this.sqliteService.prepare(`
        INSERT INTO ${this.migrationsTable} (id, filename, executed_at) 
        VALUES (?, ?, ?)
      `);

      stmt.run(migration.id, migration.filename, new Date().toISOString());

      this.logger.info(`Migration executed successfully: ${migration.filename}`);
    } catch (error) {
      this.logger.error(`Failed to execute migration: ${migration.filename}`, error);
      throw error;
    }
  }

  /**
   * 运行所有待执行的迁移
   */
  async runMigrations(): Promise<void> {
    try {
      await this.initialize();

      const availableMigrations = await this.getAvailableMigrations();
      const executedMigrations = await this.getExecutedMigrations();

      const executedIds = new Set(executedMigrations.map(m => m.id));
      const pendingMigrations = availableMigrations.filter(m => !executedIds.has(m.id));

      if (pendingMigrations.length === 0) {
        this.logger.info('No pending migrations to execute');
        return;
      }

      this.logger.info(`Found ${pendingMigrations.length} pending migrations`);

      // 在事务中执行所有迁移
      this.sqliteService.transaction(() => {
        for (const migration of pendingMigrations) {
          this.executeMigration(migration);
        }
      });

      this.logger.info('All migrations executed successfully');
    } catch (error) {
      this.logger.error('Migration failed', error);
      throw error;
    }
  }

  /**
   * 获取迁移状态
   */
  async getMigrationStatus(): Promise<{
    available: number;
    executed: number;
    pending: number;
    migrations: Migration[];
  }> {
    try {
      await this.initialize();

      const availableMigrations = await this.getAvailableMigrations();
      const executedMigrations = await this.getExecutedMigrations();

      const executedIds = new Set(executedMigrations.map(m => m.id));
      const pendingMigrations = availableMigrations.filter(m => !executedIds.has(m.id));

      return {
        available: availableMigrations.length,
        executed: executedMigrations.length,
        pending: pendingMigrations.length,
        migrations: availableMigrations.map(m => ({
          ...m,
          executed_at: executedMigrations.find(em => em.id === m.id)?.executed_at
        }))
      };
    } catch (error) {
      this.logger.error('Failed to get migration status', error);
      throw error;
    }
  }

  /**
   * 检查特定迁移是否已执行
   */
  async isMigrationExecuted(migrationId: string): Promise<boolean> {
    try {
      const stmt = this.sqliteService.prepare(`
        SELECT COUNT(*) as count FROM ${this.migrationsTable} WHERE id = ?
      `);

      const result = stmt.get(migrationId) as { count: number };
      return result.count > 0;
    } catch (error) {
      this.logger.error(`Failed to check migration status: ${migrationId}`, error);
      return false;
    }
  }
}