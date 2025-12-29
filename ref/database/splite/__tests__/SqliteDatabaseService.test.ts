import Database from 'better-sqlite3';
import { SqliteDatabaseService } from '../SqliteDatabaseService';
import { LoggerService } from '../../../utils/LoggerService';

// Mock dependencies
jest.mock('better-sqlite3');
jest.mock('fs');
jest.mock('../../../utils/LoggerService');

const mockFs = require('fs');

describe('SqliteDatabaseService', () => {
  let sqliteService: SqliteDatabaseService;
  let mockLoggerService: jest.Mocked<LoggerService>;
  let mockDatabase: jest.Mocked<Database.Database>;
  let mockStatement: jest.Mocked<Database.Statement>;

  beforeEach(() => {
    // Create mock instances
    mockLoggerService = {
      info: jest.fn(),
      error: jest.fn(),
      warn: jest.fn()
    } as any;

    mockStatement = {
      run: jest.fn().mockReturnValue({ changes: 1, lastInsertRowid: 1 }),
      get: jest.fn(),
      all: jest.fn()
    } as any;

    mockDatabase = {
      close: jest.fn(),
      pragma: jest.fn(),
      exec: jest.fn(),
      prepare: jest.fn().mockReturnValue(mockStatement),
      transaction: jest.fn((fn) => fn),
      open: true
    } as any;

    // Mock better-sqlite3 constructor
    (Database as jest.MockedClass<typeof Database>).mockImplementation(() => mockDatabase);

    // Mock fs methods
    mockFs.existsSync.mockReturnValue(false);
    mockFs.mkdirSync.mockReturnValue(undefined);
    mockFs.copyFileSync.mockReturnValue(undefined);
    mockFs.statSync.mockReturnValue({ size: 1024 });

    sqliteService = new SqliteDatabaseService(mockLoggerService, ':memory:');
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  describe('constructor', () => {
    it('should create service with default database path', () => {
      const service = new SqliteDatabaseService(mockLoggerService);

      expect(service.getDatabasePath()).toContain('code-search-helper.db');
      expect(mockFs.mkdirSync).toHaveBeenCalled();
    });

    it('should create service with custom database path', () => {
      const customPath = '/custom/path/database.db';
      const service = new SqliteDatabaseService(mockLoggerService, customPath);

      expect(service.getDatabasePath()).toBe(customPath);
    });

    it('should handle existing data directory', () => {
      mockFs.existsSync.mockReturnValue(true);
      // 重置调用历史，因为在beforeEach中已经调用过一次
      mockFs.mkdirSync.mockClear();

      const service = new SqliteDatabaseService(mockLoggerService);

      expect(mockFs.mkdirSync).not.toHaveBeenCalled();
    });
  });

  describe('connect', () => {
    it('should connect to database successfully', () => {
      sqliteService.connect();

      expect(Database).toHaveBeenCalledWith(':memory:');
      expect(mockDatabase.pragma).toHaveBeenCalledWith('journal_mode = WAL');
      expect(mockDatabase.pragma).toHaveBeenCalledWith('foreign_keys = ON');
      expect(mockLoggerService.info).toHaveBeenCalledWith(
        expect.stringContaining('SQLite数据库连接成功')
      );
    });

    it('should not connect if already connected', () => {
      sqliteService.connect();
      const initialCallCount = (Database as unknown as jest.Mock).mock.calls.length;

      sqliteService.connect();

      expect(Database).toHaveBeenCalledTimes(initialCallCount);
    });

    it('should handle connection errors', () => {
      (Database as jest.MockedClass<typeof Database>).mockImplementation(() => {
        throw new Error('Connection failed');
      });

      expect(() => sqliteService.connect()).toThrow('Connection failed');
      expect(mockLoggerService.error).toHaveBeenCalledWith(
        'SQLite数据库连接失败:',
        expect.any(Error)
      );
    });
  });

  describe('close', () => {
    it('should close database connection', () => {
      sqliteService.connect();
      sqliteService.close();

      expect(mockDatabase.close).toHaveBeenCalled();
      expect(mockLoggerService.info).toHaveBeenCalledWith(
        'SQLite数据库连接已关闭'
      );
    });

    it('should handle closing when not connected', () => {
      sqliteService.close();

      expect(mockDatabase.close).not.toHaveBeenCalled();
    });
  });

  describe('getDatabase', () => {
    it('should return database instance when connected', () => {
      sqliteService.connect();

      const db = sqliteService.getDatabase();

      expect(db).toBe(mockDatabase);
    });

    it('should throw error when not connected', () => {
      expect(() => sqliteService.getDatabase()).toThrow(
        '数据库未连接，请先调用connect()方法'
      );
    });
  });

  describe('initializeTables', () => {
    it('should create all required tables', () => {
      sqliteService.connect();
      sqliteService.initializeTables();

      expect(mockDatabase.exec).toHaveBeenCalled();
      const execCalls = mockDatabase.exec.mock.calls;

      // Verify tables are created
      const sqlCalls = execCalls.map(call => call[0]).join(' ');
      expect(sqlCalls).toContain('CREATE TABLE IF NOT EXISTS projects');
      expect(sqlCalls).toContain('CREATE TABLE IF NOT EXISTS file_index_states');
      expect(sqlCalls).toContain('CREATE TABLE IF NOT EXISTS project_status');
      expect(sqlCalls).toContain('CREATE TABLE IF NOT EXISTS file_change_history');

      // Verify indexes are created
      expect(sqlCalls).toContain('CREATE INDEX IF NOT EXISTS idx_file_states_project');
      expect(sqlCalls).toContain('CREATE INDEX IF NOT EXISTS idx_file_states_hash');

      expect(mockLoggerService.info).toHaveBeenCalledWith(
        '数据库表结构初始化完成'
      );
    });

    it('should throw error when not connected', () => {
      expect(() => sqliteService.initializeTables()).toThrow(
        '数据库未连接，请先调用connect()方法'
      );
    });
  });

  describe('transaction', () => {
    it('should execute transaction successfully', () => {
      sqliteService.connect();

      const mockFn = jest.fn().mockReturnValue('result');
      const result = sqliteService.transaction(mockFn);

      expect(mockDatabase.transaction).toHaveBeenCalledWith(mockFn);
      expect(result).toBe('result');
    });

    it('should throw error when not connected', () => {
      const mockFn = jest.fn();

      expect(() => sqliteService.transaction(mockFn)).toThrow(
        '数据库未连接，请先调用connect()方法'
      );
    });
  });

  describe('prepare', () => {
    it('should prepare SQL statement', () => {
      sqliteService.connect();

      const sql = 'SELECT * FROM projects';
      const statement = sqliteService.prepare(sql);

      expect(mockDatabase.prepare).toHaveBeenCalledWith(sql);
      expect(statement).toBe(mockStatement);
    });

    it('should throw error when not connected', () => {
      expect(() => sqliteService.prepare('SELECT 1')).toThrow(
        '数据库未连接，请先调用connect()方法'
      );
    });
  });

  describe('exec', () => {
    it('should execute SQL statement', () => {
      sqliteService.connect();

      const sql = 'CREATE TABLE test (id INTEGER)';
      sqliteService.exec(sql);

      expect(mockDatabase.exec).toHaveBeenCalledWith(sql);
    });

    it('should throw error when not connected', () => {
      expect(() => sqliteService.exec('SELECT 1')).toThrow(
        '数据库未连接，请先调用connect()方法'
      );
    });
  });

  describe('isConnected', () => {
    it('should return true when connected', () => {
      sqliteService.connect();

      expect(sqliteService.isConnected()).toBe(true);
    });

    it('should return false when not connected', () => {
      expect(sqliteService.isConnected()).toBe(false);
    });

    it('should return false when database is closed', () => {
      sqliteService.connect();
      mockDatabase.open = false;

      expect(sqliteService.isConnected()).toBe(false);
    });
  });

  describe('getDatabasePath', () => {
    it('should return database path', () => {
      expect(sqliteService.getDatabasePath()).toBe(':memory:');
    });
  });

  describe('backup', () => {
    it('should backup database successfully', () => {
      sqliteService.connect();

      const backupPath = '/backup/path/database.db';
      sqliteService.backup(backupPath);

      expect(mockFs.copyFileSync).toHaveBeenCalledWith(':memory:', backupPath);
      expect(mockLoggerService.info).toHaveBeenCalledWith(
        expect.stringContaining('数据库备份完成')
      );
    });

    it('should throw error when not connected', () => {
      expect(() => sqliteService.backup('/backup/path')).toThrow(
        '数据库未连接，无法执行备份'
      );
    });

    it('should handle backup errors', () => {
      sqliteService.connect();
      mockFs.copyFileSync.mockImplementation(() => {
        throw new Error('Backup failed');
      });

      expect(() => sqliteService.backup('/backup/path')).toThrow('Backup failed');
      expect(mockLoggerService.error).toHaveBeenCalledWith(
        '数据库备份失败:',
        expect.any(Error)
      );
    });
  });

  describe('getStats', () => {
    beforeEach(() => {
      sqliteService.connect();
      // 为不同的查询返回不同的mock结果
      mockDatabase.prepare.mockImplementation((sql: string) => {
        mockStatement.get.mockImplementation(() => {
          if (sql.includes('projects')) return { count: 5 };
          if (sql.includes('file_index_states')) return { count: 100 };
          if (sql.includes('project_status')) return { count: 3 };
          if (sql.includes('file_change_history')) return { count: 50 };
          return { count: 0 };
        });
        // Mock all method for table size queries
        mockStatement.all.mockImplementation(() => {
          if (sql.includes('sqlite_master')) {
            return [
              { name: 'projects' },
              { name: 'file_index_states' },
              { name: 'project_status' },
              { name: 'file_change_history' }
            ];
          }
          return [];
        });
        return mockStatement;
      });
    });

    it('should return database statistics', () => {
      const stats = sqliteService.getStats();

      expect(stats).toEqual({
        projects: 5,
        fileStates: 100,
        projectStatus: 3,
        changeHistory: 50,
        databaseSize: 1024,
        tableSizes: expect.any(Object)
      });
    });

    it('should handle file stat errors gracefully', () => {
      mockFs.statSync.mockImplementation(() => {
        throw new Error('File not found');
      });

      const stats = sqliteService.getStats();

      expect(stats.databaseSize).toBe(0);
      expect(mockLoggerService.warn).toHaveBeenCalledWith(
        '无法获取数据库文件大小:',
        expect.any(Error)
      );
    });

    it('should handle table size query errors gracefully', () => {
      // 模拟prepare方法在表大小查询时抛出错误
      mockDatabase.prepare.mockImplementation((sql: string) => {
        if (sql.includes('sqlite_master')) {
          throw new Error('Query failed');
        }
        mockStatement.get.mockImplementation(() => {
          if (sql.includes('projects')) return { count: 5 };
          if (sql.includes('file_index_states')) return { count: 100 };
          if (sql.includes('project_status')) return { count: 3 };
          if (sql.includes('file_change_history')) return { count: 50 };
          return { count: 0 };
        });
        mockStatement.all.mockImplementation(() => {
          return [
            { name: 'projects' },
            { name: 'file_index_states' },
            { name: 'project_status' },
            { name: 'file_change_history' }
          ];
        });
        return mockStatement;
      });

      const stats = sqliteService.getStats();

      expect(stats.tableSizes).toBeUndefined();
      expect(mockLoggerService.warn).toHaveBeenCalledWith(
        '无法获取表大小信息:',
        expect.any(Error)
      );
    });
  });

  describe('initialize', () => {
    it('should initialize service when not connected', async () => {
      await sqliteService.initialize();

      expect(mockDatabase.pragma).toHaveBeenCalled();
      expect(mockDatabase.exec).toHaveBeenCalled();
    });

    it('should not initialize when already connected', async () => {
      sqliteService.connect();
      const initialExecCount = mockDatabase.exec.mock.calls.length;

      await sqliteService.initialize();

      expect(mockDatabase.exec).toHaveBeenCalledTimes(initialExecCount);
    });
  });

  describe('executeQuery', () => {
    it('should execute query with parameters', async () => {
      sqliteService.connect();

      const query = 'INSERT INTO projects (id, name) VALUES (?, ?)';
      const params = ['project1', 'Test Project'];

      await sqliteService.executeQuery(query, params);

      expect(mockDatabase.prepare).toHaveBeenCalledWith(query);
      expect(mockStatement.run).toHaveBeenCalledWith(...params);
    });

    it('should execute query without parameters', async () => {
      sqliteService.connect();

      const query = 'SELECT * FROM projects';

      await sqliteService.executeQuery(query);

      expect(mockDatabase.prepare).toHaveBeenCalledWith(query);
      expect(mockStatement.run).toHaveBeenCalledWith();
    });

    it('should throw error when not connected', async () => {
      await expect(sqliteService.executeQuery('SELECT 1')).rejects.toThrow(
        '数据库未连接，请先调用connect()方法'
      );
    });
  });
});