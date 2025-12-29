import Database from 'better-sqlite3';
import path from 'path';
import fs from 'fs';
import { injectable, inject, unmanaged } from 'inversify';
import { TYPES } from '../../types';
import { LoggerService } from '../../utils/LoggerService';

/**
 * SQLite数据库服务类
 * 提供数据库连接管理和基本操作
 */
@injectable()
export class SqliteDatabaseService {
  private db: Database.Database | null = null;
  private dbPath: string;
  private logger: LoggerService;

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
  @unmanaged() 
    dbPath?: string
  ) {
    this.logger = logger;
    this.dbPath = dbPath || path.join(process.cwd(), 'data', 'code-search-helper.db');
    this.ensureDataDirectory();
  }

  /**
   * 确保数据目录存在
   */
  private ensureDataDirectory(): void {
    const dataDir = path.dirname(this.dbPath);
    if (!fs.existsSync(dataDir)) {
      fs.mkdirSync(dataDir, { recursive: true });
    }
  }

  /**
   * 连接到数据库
   */
  connect(): void {
    if (this.db) {
      return; // 已经连接
    }

    try {
      this.db = new Database(this.dbPath);
      this.db.pragma('journal_mode = WAL');
      this.db.pragma('foreign_keys = ON');
      this.logger.info(`SQLite数据库连接成功: ${this.dbPath}`);
    } catch (error) {
      this.logger.error('SQLite数据库连接失败:', error);
      throw error;
    }
  }

  /**
   * 关闭数据库连接
   */
  close(): void {
    if (this.db) {
      this.db.close();
      this.db = null;
      this.logger.info('SQLite数据库连接已关闭');
    }
  }

  /**
   * 获取数据库实例
   */
  getDatabase(): Database.Database {
    if (!this.db) {
      throw new Error('数据库未连接，请先调用connect()方法');
    }
    return this.db;
  }

  /**
   * 检查并添加热重载字段（用于现有数据库的迁移）
   */
  private addHotReloadColumnsIfNeeded(): void {
    const db = this.getDatabase();
    
    try {
      // 检查是否已存在热重载字段
      const columns = db.prepare("PRAGMA table_info(project_status)").all() as any[];
      const columnNames = columns.map(col => col.name);
      
      // 添加缺失的热重载字段
      if (!columnNames.includes('hot_reload_enabled')) {
        db.exec('ALTER TABLE project_status ADD COLUMN hot_reload_enabled BOOLEAN DEFAULT FALSE');
        this.logger.info('Added hot_reload_enabled column to project_status table');
      }
      
      if (!columnNames.includes('hot_reload_config')) {
        db.exec('ALTER TABLE project_status ADD COLUMN hot_reload_config JSON');
        this.logger.info('Added hot_reload_config column to project_status table');
      }
      
      if (!columnNames.includes('hot_reload_last_enabled')) {
        db.exec('ALTER TABLE project_status ADD COLUMN hot_reload_last_enabled DATETIME');
        this.logger.info('Added hot_reload_last_enabled column to project_status table');
      }
      
      if (!columnNames.includes('hot_reload_last_disabled')) {
        db.exec('ALTER TABLE project_status ADD COLUMN hot_reload_last_disabled DATETIME');
        this.logger.info('Added hot_reload_last_disabled column to project_status table');
      }
      
      if (!columnNames.includes('hot_reload_changes_detected')) {
        db.exec('ALTER TABLE project_status ADD COLUMN hot_reload_changes_detected INTEGER DEFAULT 0');
        this.logger.info('Added hot_reload_changes_detected column to project_status table');
      }
      
      if (!columnNames.includes('hot_reload_errors_count')) {
        db.exec('ALTER TABLE project_status ADD COLUMN hot_reload_errors_count INTEGER DEFAULT 0');
        this.logger.info('Added hot_reload_errors_count column to project_status table');
      }
      
      // 创建热重载相关索引（如果不存在）
      db.exec('CREATE INDEX IF NOT EXISTS idx_project_status_hot_reload_enabled ON project_status(hot_reload_enabled)');
      db.exec('CREATE INDEX IF NOT EXISTS idx_project_status_hot_reload_updated ON project_status(hot_reload_last_enabled, hot_reload_last_disabled)');
      
      this.logger.info('Hot reload columns migration completed');
    } catch (error) {
      this.logger.error('Failed to add hot reload columns', error);
      throw error;
    }
  }

  /**
   * 初始化数据库表结构
   */
  initializeTables(): void {
    const db = this.getDatabase();

    // 创建项目表
    db.exec(`
      CREATE TABLE IF NOT EXISTS projects (
          id TEXT PRIMARY KEY,
          path TEXT UNIQUE NOT NULL,
          name TEXT,
          description TEXT,
          collection_name TEXT,
          space_name TEXT,
          created_at DATETIME NOT NULL,
          updated_at DATETIME NOT NULL,
          last_indexed_at DATETIME,
          status TEXT NOT NULL,
          settings JSON,
          metadata JSON
      )
    `);

    // 创建文件索引状态表
    db.exec(`
      CREATE TABLE IF NOT EXISTS file_index_states (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          project_id TEXT NOT NULL,
          file_path TEXT NOT NULL,
          relative_path TEXT NOT NULL,
          content_hash TEXT NOT NULL,
          file_size INTEGER,
          last_modified DATETIME NOT NULL,
          last_indexed DATETIME,
          indexing_version INTEGER DEFAULT 1,
          chunk_count INTEGER,
          vector_count INTEGER,
          language TEXT,
          file_type TEXT,
          status TEXT DEFAULT 'pending',
          error_message TEXT,
          metadata JSON,
          created_at DATETIME NOT NULL,
          updated_at DATETIME NOT NULL,
          
          UNIQUE(project_id, file_path),
          FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
      )
    `);

    // 创建项目状态表
    db.exec(`
      CREATE TABLE IF NOT EXISTS project_status (
          project_id TEXT PRIMARY KEY,
          vector_status JSON NOT NULL,
          graph_status JSON NOT NULL,
          indexing_progress REAL DEFAULT 0,
          total_files INTEGER DEFAULT 0,
          indexed_files INTEGER DEFAULT 0,
          failed_files INTEGER DEFAULT 0,
          last_updated DATETIME NOT NULL,
          hot_reload_enabled BOOLEAN DEFAULT FALSE,
          hot_reload_config JSON,
          hot_reload_last_enabled DATETIME,
          hot_reload_last_disabled DATETIME,
          hot_reload_changes_detected INTEGER DEFAULT 0,
          hot_reload_errors_count INTEGER DEFAULT 0,
          
          FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
      )
    `);

    // 创建文件变更历史表
    db.exec(`
      CREATE TABLE IF NOT EXISTS file_change_history (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          file_path TEXT NOT NULL,
          project_id TEXT NOT NULL,
          change_type TEXT NOT NULL,
          previous_hash TEXT,
          current_hash TEXT,
          file_size INTEGER,
          timestamp DATETIME NOT NULL,
          metadata JSON,
          
          FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
      )
    `);

    // 创建索引
    db.exec('CREATE INDEX IF NOT EXISTS idx_file_states_project ON file_index_states(project_id)');
    db.exec('CREATE INDEX IF NOT EXISTS idx_file_states_hash ON file_index_states(content_hash)');
    db.exec('CREATE INDEX IF NOT EXISTS idx_file_states_path ON file_index_states(file_path)');
    db.exec('CREATE INDEX IF NOT EXISTS idx_file_states_modified ON file_index_states(last_modified)');
    db.exec('CREATE INDEX IF NOT EXISTS idx_change_history_project ON file_change_history(project_id, timestamp)');
    db.exec('CREATE INDEX IF NOT EXISTS idx_change_history_file ON file_change_history(file_path, timestamp)');
    
    // 创建热重载相关索引
    db.exec('CREATE INDEX IF NOT EXISTS idx_project_status_hot_reload_enabled ON project_status(hot_reload_enabled)');
    db.exec('CREATE INDEX IF NOT EXISTS idx_project_status_hot_reload_updated ON project_status(hot_reload_last_enabled, hot_reload_last_disabled)');

    // 确保表结构创建完成后再检查热重载字段
    try {
      this.addHotReloadColumnsIfNeeded();
    } catch (error) {
      this.logger.warn('Failed to add hot reload columns during initialization, will retry on demand', error);
    }

    this.logger.info('数据库表结构初始化完成');
  }

  /**
   * 执行事务
   */
  transaction<T>(fn: () => T): T {
    const db = this.getDatabase();
    const transaction = db.transaction(fn);
    return transaction();
  }

  /**
   * 准备SQL语句
   */
  prepare(sql: string): Database.Statement {
    const db = this.getDatabase();
    return db.prepare(sql);
  }

  /**
   * 执行SQL语句
   */
  exec(sql: string): void {
    const db = this.getDatabase();
    db.exec(sql);
  }

  /**
   * 检查数据库连接状态
   */
  isConnected(): boolean {
    return this.db !== null && this.db.open;
  }

  /**
   * 获取数据库文件路径
   */
  getDatabasePath(): string {
    return this.dbPath;
  }

  /**
   * 备份数据库
   */
  backup(backupPath: string): void {
    if (!this.isConnected()) {
      throw new Error('数据库未连接，无法执行备份');
    }

    // 先关闭数据库连接以避免文件锁定
    const db = this.db;
    this.db = null;

    try {
      // 使用文件复制进行备份
      fs.copyFileSync(this.dbPath, backupPath);
      this.logger.info(`数据库备份完成: ${backupPath}`);
    } catch (error) {
      this.logger.error('数据库备份失败:', error);
      throw error;
    } finally {
      // 恢复数据库连接
      this.db = db;
    }
  }

  /**
   * 获取数据库统计信息
   */
  getStats(): {
    projects: number;
    fileStates: number;
    projectStatus: number;
    changeHistory: number;
    databaseSize: number;
    tableSizes?: Record<string, number>;
  } {
    const db = this.getDatabase();

    const projects = db.prepare('SELECT COUNT(*) as count FROM projects').get() as { count: number };
    const fileStates = db.prepare('SELECT COUNT(*) as count FROM file_index_states').get() as { count: number };
    const projectStatus = db.prepare('SELECT COUNT(*) as count FROM project_status').get() as { count: number };
    const changeHistory = db.prepare('SELECT COUNT(*) as count FROM file_change_history').get() as { count: number };

    let databaseSize = 0;
    try {
      const stats = fs.statSync(this.dbPath);
      databaseSize = stats.size;
    } catch (error) {
      this.logger.warn('无法获取数据库文件大小:', error);
    }

    // 获取表大小信息
    let tableSizes: Record<string, number> | undefined;
    try {
      const tableSizesTemp: Record<string, number> = {};
      const tables = db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'").all() as { name: string }[];
      for (const table of tables) {
        const count = db.prepare(`SELECT COUNT(*) as count FROM ${table.name}`).get() as { count: number };
        tableSizesTemp[table.name] = count.count;
      }
      tableSizes = tableSizesTemp;
    } catch (error) {
      this.logger.warn('无法获取表大小信息:', error);
      tableSizes = undefined;
    }

    return {
      projects: projects.count,
      fileStates: fileStates.count,
      projectStatus: projectStatus.count,
      changeHistory: changeHistory.count,
      databaseSize,
      tableSizes
    };
  }

  /**
   * 初始化数据库服务
   */
  async initialize(): Promise<void> {
    if (this.isConnected()) {
      return;
    }

    this.connect();
    this.initializeTables();
  }

  /**
   * 执行查询
   */
  async executeQuery(query: string, params?: any[]): Promise<any> {
    const db = this.getDatabase();
    const stmt = db.prepare(query);
    return params ? stmt.run(...params) : stmt.run();
  }
}

// 注意：单例实例已移除，请使用DI容器获取实例
// export const sqliteDatabaseService = new SqliteDatabaseService();