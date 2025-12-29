import { MigrationManager } from '../MigrationManager';
import { SqliteDatabaseService } from '../../SqliteDatabaseService';
import { LoggerService } from '../../../../utils/LoggerService';
import fs from 'fs';
import path from 'path';

describe('MigrationManager', () => {
  let migrationManager: MigrationManager;
  let sqliteService: SqliteDatabaseService;
  let loggerService: LoggerService;
  let testDbPath: string;

  beforeEach(() => {
    // 创建测试数据库路径
    testDbPath = path.join(__dirname, 'test-migration.db');
    
    // 创建模拟的LoggerService
    loggerService = {
      info: jest.fn(),
      error: jest.fn(),
      warn: jest.fn()
    } as any;

    // 创建SqliteDatabaseService实例
    sqliteService = new SqliteDatabaseService(loggerService, testDbPath);
    
    // 创建MigrationManager实例
    migrationManager = new MigrationManager(sqliteService, loggerService);
  });

  afterEach(() => {
    // 清理测试数据库
    if (sqliteService.isConnected()) {
      sqliteService.close();
    }
    
    if (fs.existsSync(testDbPath)) {
      fs.unlinkSync(testDbPath);
    }
  });

  describe('initialize', () => {
    it('should create schema_migrations table', async () => {
      await sqliteService.connect();
      await migrationManager.initialize();

      // 检查表是否创建
      const stmt = sqliteService.prepare(`
        SELECT name FROM sqlite_master WHERE type='table' AND name='schema_migrations'
      `);
      const result = stmt.get() as { name: string } | undefined;
      
      expect(result).toBeDefined();
      expect(result!.name).toBe('schema_migrations');
    });
  });

  describe('getAvailableMigrations', () => {
    it('should return available migration files', async () => {
      const migrations = await migrationManager.getAvailableMigrations();
      
      expect(Array.isArray(migrations)).toBe(true);
      expect(migrations.length).toBeGreaterThan(0);
      
      // 检查第一个迁移文件
      const firstMigration = migrations[0];
      expect(firstMigration.id).toBe('001_add_hot_reload_columns');
      expect(firstMigration.filename).toBe('001_add_hot_reload_columns.sql');
      expect(typeof firstMigration.sql).toBe('string');
    });
  });

  describe('runMigrations', () => {
    beforeEach(async () => {
      await sqliteService.connect();
      await sqliteService.initializeTables(); // 创建基础表结构
    });

    it('should run pending migrations', async () => {
      // 运行迁移
      await migrationManager.runMigrations();

      // 检查热重载字段是否添加
      const stmt = sqliteService.prepare(`
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

      hotReloadColumns.forEach(column => {
        const found = columns.some(col => col.name === column);
        expect(found).toBe(true);
      });

      // 检查迁移记录
      const migrationStmt = sqliteService.prepare(`
        SELECT * FROM schema_migrations
      `);
      const migrationRecords = migrationStmt.all() as { id: string }[];
      
      expect(migrationRecords.length).toBe(1);
      expect(migrationRecords[0].id).toBe('001_add_hot_reload_columns');
    });

    it('should not run already executed migrations', async () => {
      // 第一次运行迁移
      await migrationManager.runMigrations();

      // 第二次运行迁移
      await migrationManager.runMigrations();

      // 检查迁移记录仍然只有一条
      const migrationStmt = sqliteService.prepare(`
        SELECT COUNT(*) as count FROM schema_migrations
      `);
      const result = migrationStmt.get() as { count: number } | undefined;
      
      expect(result).toBeDefined();
      expect(result!.count).toBe(1);
    });
  });

  describe('getMigrationStatus', () => {
    beforeEach(async () => {
      await sqliteService.connect();
      await sqliteService.initializeTables();
    });

    it('should return correct migration status', async () => {
      const status = await migrationManager.getMigrationStatus();
      
      expect(status.available).toBeGreaterThan(0);
      expect(status.executed).toBe(0);
      expect(status.pending).toBe(status.available);
      expect(Array.isArray(status.migrations)).toBe(true);
    });

    it('should show executed migrations after running them', async () => {
      // 运行迁移
      await migrationManager.runMigrations();

      // 获取状态
      const status = await migrationManager.getMigrationStatus();
      
      expect(status.executed).toBe(1);
      expect(status.pending).toBe(0);
      
      const executedMigration = status.migrations.find(m => m.executed_at);
      expect(executedMigration).toBeDefined();
      expect(executedMigration?.id).toBe('001_add_hot_reload_columns');
    });
  });

  describe('isMigrationExecuted', () => {
    beforeEach(async () => {
      await sqliteService.connect();
      await sqliteService.initializeTables();
    });

    it('should return false for non-executed migration', async () => {
      const isExecuted = await migrationManager.isMigrationExecuted('001_add_hot_reload_columns');
      expect(isExecuted).toBe(false);
    });

    it('should return true for executed migration', async () => {
      // 运行迁移
      await migrationManager.runMigrations();

      // 检查迁移状态
      const isExecuted = await migrationManager.isMigrationExecuted('001_add_hot_reload_columns');
      expect(isExecuted).toBe(true);
    });
  });

  describe('error handling', () => {
    it('should handle invalid SQL gracefully', async () => {
      await sqliteService.connect();
      
      // 创建一个无效的迁移文件
      const invalidMigrationPath = path.join(__dirname, '999_invalid.sql');
      fs.writeFileSync(invalidMigrationPath, 'INVALID SQL STATEMENT;');

      try {
        await migrationManager.runMigrations();
        // 应该抛出错误
        expect(true).toBe(false);
      } catch (error) {
        expect(error).toBeDefined();
      } finally {
        // 清理无效文件
        if (fs.existsSync(invalidMigrationPath)) {
          fs.unlinkSync(invalidMigrationPath);
        }
      }
    });
  });
});