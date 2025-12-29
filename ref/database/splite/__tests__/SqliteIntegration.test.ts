import { Container } from 'inversify';
import { TYPES } from '../../../types';
import { SqliteDatabaseService } from '../SqliteDatabaseService';
import { SqliteConnectionManager } from '../SqliteConnectionManager';
import { SqliteProjectManager } from '../SqliteProjectManager';

import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { ConfigService } from '../../../config/ConfigService';

describe('SQLite Integration Tests', () => {
  let container: Container;
  let sqliteService: SqliteDatabaseService;
  let connectionManager: SqliteConnectionManager;
  let projectManager: SqliteProjectManager;

  beforeAll(async () => {
    // 设置DI容器
    container = new Container();

    // 注册基础服务 - 只绑定SQLite服务真正需要的依赖
    container.bind<LoggerService>(TYPES.LoggerService).to(LoggerService).inSingletonScope();
    container.bind<ErrorHandlerService>(TYPES.ErrorHandlerService).to(ErrorHandlerService).inSingletonScope();
    container.bind<ConfigService>(TYPES.ConfigService).to(ConfigService).inSingletonScope();

    // 注册SQLite服务
    container.bind<SqliteDatabaseService>(TYPES.SqliteDatabaseService).to(SqliteDatabaseService).inSingletonScope();
    container.bind<SqliteConnectionManager>(TYPES.SqliteConnectionManager).to(SqliteConnectionManager).inSingletonScope();
    container.bind<SqliteProjectManager>(TYPES.SqliteProjectManager).to(SqliteProjectManager).inSingletonScope();

    // 解析服务
    sqliteService = container.get<SqliteDatabaseService>(TYPES.SqliteDatabaseService);
    connectionManager = container.get<SqliteConnectionManager>(TYPES.SqliteConnectionManager);
    projectManager = container.get<SqliteProjectManager>(TYPES.SqliteProjectManager);

    // 初始化数据库连接
    await sqliteService.connect();

    // 初始化连接管理器
    await connectionManager.initialize();
  });

  afterAll(async () => {
    // 清理资源 - SQLite 服务会在容器销毁时自动清理
  });

  describe('SqliteDatabaseService', () => {
    test('should initialize and connect to database', async () => {
      expect(sqliteService.isConnected()).toBe(true);
    });

    test('should create tables on initialization', () => {
      const db = sqliteService.getDatabase();
      const tables = db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'").all();
      expect(tables).toEqual(
        expect.arrayContaining([
          expect.objectContaining({ name: 'projects' }),
          expect.objectContaining({ name: 'file_index_states' }),
          expect.objectContaining({ name: 'project_status' }),
          expect.objectContaining({ name: 'file_change_history' })
        ])
      );
    });

    test('should get database statistics', () => {
      const stats = sqliteService.getStats();
      expect(stats).toHaveProperty('projects');
      expect(stats).toHaveProperty('fileStates');
      expect(stats).toHaveProperty('projectStatus');
      expect(stats).toHaveProperty('changeHistory');
      expect(stats).toHaveProperty('databaseSize');
    });
  });

  describe('SqliteConnectionManager', () => {
    test('should connect and disconnect', async () => {
      expect(connectionManager.isConnected()).toBe(true);

      await connectionManager.disconnect();
      expect(connectionManager.isConnected()).toBe(false);

      await connectionManager.connect();
      expect(connectionManager.isConnected()).toBe(true);
    });

    test('should get connection status', () => {
      const status = connectionManager.getConnectionStatus();
      expect(status).toHaveProperty('connected');
      expect(status).toHaveProperty('databasePath');
      expect(status).toHaveProperty('stats');
    });
  });

  describe('SqliteProjectManager', () => {
    const testProjectPath = '/test/project/path';

    test('should create project space', async () => {
      const result = await projectManager.createProjectSpace(testProjectPath, {
        name: 'Test Project',
        description: 'A test project for SQLite integration'
      });
      expect(result).toBe(true);
    });

    test('should get project space info', async () => {
      const info = await projectManager.getProjectSpaceInfo(testProjectPath);
      expect(info).not.toBeNull();
      expect(info.project).toHaveProperty('id');
      expect(info.project).toHaveProperty('path', testProjectPath);
      expect(info.project).toHaveProperty('name', 'Test Project');
      expect(info).toHaveProperty('status');
      expect(info).toHaveProperty('fileStats');
    });

    test('should list project spaces', async () => {
      const projects = await projectManager.listProjectSpaces();
      expect(Array.isArray(projects)).toBe(true);
      expect(projects.length).toBeGreaterThan(0);

      const testProject = projects.find(p => p.path === testProjectPath);
      expect(testProject).toBeDefined();
      expect(testProject.name).toBe('Test Project');
    });

    test('should update project status', async () => {
      const project = await projectManager.getProjectSpaceInfo(testProjectPath);
      const status = {
        project_id: project.project.id,
        vector_status: { indexed: true },
        graph_status: { indexed: false },
        indexing_progress: 0.5,
        total_files: 10,
        indexed_files: 5,
        failed_files: 0,
        last_updated: new Date()
      };

      const result = await projectManager.updateProjectStatus(project.project.id, status);
      expect(result).toBe(true);
    });

    test('should clear project space', async () => {
      const result = await projectManager.clearProjectSpace(testProjectPath);
      expect(result).toBe(true);
    });

    test('should delete project space', async () => {
      const result = await projectManager.deleteProjectSpace(testProjectPath);
      expect(result).toBe(true);
    });
  });


});