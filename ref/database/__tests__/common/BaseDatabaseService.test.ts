import { BaseDatabaseService } from '../../../database/common/BaseDatabaseService';
import { IConnectionManager, IProjectManager } from '../../../database/common/IDatabaseService';
import { EventListener } from '../../../types';

// 创建一个具体的实现类用于测试
class TestDatabaseService extends BaseDatabaseService {
  constructor(
    connectionManager: IConnectionManager,
    projectManager: IProjectManager
  ) {
    super(connectionManager, projectManager);
  }
}

// Mock implementations
const mockConnectionManager: jest.Mocked<IConnectionManager> = {
  connect: jest.fn(),
  disconnect: jest.fn(),
  isConnected: jest.fn(),
  getConfig: jest.fn(),
  updateConfig: jest.fn(),
  getConnectionStatus: jest.fn(),
  subscribe: jest.fn()
};

const mockProjectManager: jest.Mocked<IProjectManager> = {
  createProjectSpace: jest.fn(),
  deleteProjectSpace: jest.fn(),
  getProjectSpaceInfo: jest.fn(),
  clearProjectSpace: jest.fn(),
  listProjectSpaces: jest.fn(),
  insertProjectData: jest.fn(),
  updateProjectData: jest.fn(),
  deleteProjectData: jest.fn(),
  searchProjectData: jest.fn(),
  getProjectDataById: jest.fn(),
  subscribe: jest.fn()
};

describe('BaseDatabaseService', () => {
  let databaseService: TestDatabaseService;
  let connectionManager: jest.Mocked<IConnectionManager>;
  let projectManager: jest.Mocked<IProjectManager>;

  beforeEach(() => {
    // 重置所有mock
    jest.clearAllMocks();
    
    // 创建新的mock实例
    connectionManager = { ...mockConnectionManager };
    projectManager = { ...mockProjectManager };
    
    databaseService = new TestDatabaseService(connectionManager, projectManager);
  });

  describe('initialize', () => {
    it('should initialize successfully when connection manager connects', async () => {
      connectionManager.connect.mockResolvedValue(true);
      
      const result = await databaseService.initialize();
      
      expect(result).toBe(true);
      expect(connectionManager.connect).toHaveBeenCalled();
    });

    it('should fail to initialize when connection manager fails to connect', async () => {
      connectionManager.connect.mockResolvedValue(false);
      
      const result = await databaseService.initialize();
      
      expect(result).toBe(false);
      expect(connectionManager.connect).toHaveBeenCalled();
    });

    it('should handle connection errors during initialization', async () => {
      const error = new Error('Connection failed');
      connectionManager.connect.mockRejectedValue(error);
      
      const result = await databaseService.initialize();
      
      expect(result).toBe(false);
      expect(connectionManager.connect).toHaveBeenCalled();
    });

    it('should return true if already initialized', async () => {
      connectionManager.connect.mockResolvedValue(true);
      
      // 第一次初始化
      await databaseService.initialize();
      
      // 第二次初始化应该直接返回true
      const result = await databaseService.initialize();
      
      expect(result).toBe(true);
      expect(connectionManager.connect).toHaveBeenCalledTimes(1);
    });
  });

  describe('isConnected', () => {
    it('should delegate to connection manager', () => {
      connectionManager.isConnected.mockReturnValue(true);
      
      const result = databaseService.isConnected();
      
      expect(result).toBe(true);
      expect(connectionManager.isConnected).toHaveBeenCalled();
    });
  });

  describe('close', () => {
    it('should disconnect from connection manager', async () => {
      connectionManager.disconnect.mockResolvedValue();
      
      await databaseService.close();
      
      expect(connectionManager.disconnect).toHaveBeenCalled();
      // 检查服务状态是否重置
      expect((databaseService as any).initialized).toBe(false);
    });

    it('should handle errors during close', async () => {
      const error = new Error('Disconnect failed');
      connectionManager.disconnect.mockRejectedValue(error);
      
      await expect(databaseService.close()).rejects.toThrow(error);
      
      expect(connectionManager.disconnect).toHaveBeenCalled();
    });
  });

  describe('project space operations', () => {
    beforeEach(async () => {
      // 确保服务已初始化
      connectionManager.connect.mockResolvedValue(true);
      await databaseService.initialize();
    });

    it('should create project space', async () => {
      projectManager.createProjectSpace.mockResolvedValue(true);
      
      const result = await databaseService.createProjectSpace('/test/path');
      
      expect(result).toBe(true);
      expect(projectManager.createProjectSpace).toHaveBeenCalledWith('/test/path', undefined);
    });

    it('should delete project space', async () => {
      projectManager.deleteProjectSpace.mockResolvedValue(true);
      
      const result = await databaseService.deleteProjectSpace('/test/path');
      
      expect(result).toBe(true);
      expect(projectManager.deleteProjectSpace).toHaveBeenCalledWith('/test/path');
    });

    it('should get project space info', async () => {
      const info = { name: 'test', size: 100 };
      projectManager.getProjectSpaceInfo.mockResolvedValue(info);
      
      const result = await databaseService.getProjectSpaceInfo('/test/path');
      
      expect(result).toEqual(info);
      expect(projectManager.getProjectSpaceInfo).toHaveBeenCalledWith('/test/path');
    });
  });

  describe('data operations', () => {
    beforeEach(async () => {
      // 确保服务已初始化
      connectionManager.connect.mockResolvedValue(true);
      await databaseService.initialize();
    });

    it('should insert data', async () => {
      projectManager.insertProjectData.mockResolvedValue(true);
      const data = { id: '1', content: 'test' };
      
      const result = await databaseService.insertData('/test/path', data);
      
      expect(result).toBe(true);
      expect(projectManager.insertProjectData).toHaveBeenCalledWith('/test/path', data);
    });

    it('should update data', async () => {
      projectManager.updateProjectData.mockResolvedValue(true);
      const data = { content: 'updated' };
      
      const result = await databaseService.updateData('/test/path', '1', data);
      
      expect(result).toBe(true);
      expect(projectManager.updateProjectData).toHaveBeenCalledWith('/test/path', '1', data);
    });

    it('should delete data', async () => {
      projectManager.deleteProjectData.mockResolvedValue(true);
      
      const result = await databaseService.deleteData('/test/path', '1');
      
      expect(result).toBe(true);
      expect(projectManager.deleteProjectData).toHaveBeenCalledWith('/test/path', '1');
    });
  });

  describe('search operations', () => {
    beforeEach(async () => {
      // 确保服务已初始化
      connectionManager.connect.mockResolvedValue(true);
      await databaseService.initialize();
    });

    it('should search data', async () => {
      const results = [{ id: '1', content: 'test' }];
      projectManager.searchProjectData.mockResolvedValue(results);
      const query = { term: 'test' };
      
      const result = await databaseService.searchData('/test/path', query);
      
      expect(result).toEqual(results);
      expect(projectManager.searchProjectData).toHaveBeenCalledWith('/test/path', query);
    });

    it('should get data by id', async () => {
      const data = { id: '1', content: 'test' };
      projectManager.getProjectDataById.mockResolvedValue(data);
      
      const result = await databaseService.getDataById('/test/path', '1');
      
      expect(result).toEqual(data);
      expect(projectManager.getProjectDataById).toHaveBeenCalledWith('/test/path', '1');
    });
  });

  describe('event listeners', () => {
    it('should add event listeners to both managers', () => {
      const listener: EventListener = jest.fn();
      const mockSubscription = {
        id: 'test-id',
        eventType: 'test_event',
        handler: listener,
        unsubscribe: jest.fn()
      };

      // Mock subscribe to return subscription object
      connectionManager.subscribe.mockReturnValue(mockSubscription);
      projectManager.subscribe.mockReturnValue(mockSubscription);

      databaseService.subscribe('test_event', listener);

      expect(connectionManager.subscribe).toHaveBeenCalledWith('test_event', listener);
      expect(projectManager.subscribe).toHaveBeenCalledWith('test_event', listener);
    });

    it('should handle subscription unsubscribe', () => {
      const listener: EventListener = jest.fn();
      const mockSubscription = {
        id: 'test-id',
        eventType: 'test_event',
        handler: listener,
        unsubscribe: jest.fn()
      };

      // Mock subscribe to return subscription object
      connectionManager.subscribe.mockReturnValue(mockSubscription);
      projectManager.subscribe.mockReturnValue(mockSubscription);

      const subscription = databaseService.subscribe('test_event', listener);
      subscription.unsubscribe();

      expect(mockSubscription.unsubscribe).toHaveBeenCalled();
    });
  });

  describe('healthCheck', () => {
    it('should return healthy status when connected and projects exist', async () => {
      connectionManager.isConnected.mockReturnValue(true);
      projectManager.listProjectSpaces.mockResolvedValue([{ name: 'test' }]);
      
      const result = await databaseService.healthCheck();
      
      expect(result).toEqual({
        status: 'healthy',
        details: {
          connected: true,
          projectSpacesCount: 1,
          initialized: false // 未初始化
        }
      });
    });

    it('should return unhealthy status when not connected', async () => {
      connectionManager.isConnected.mockReturnValue(false);
      projectManager.listProjectSpaces.mockResolvedValue([]);
      
      const result = await databaseService.healthCheck();
      
      expect(result).toEqual({
        status: 'unhealthy',
        details: {
          connected: false,
          projectSpacesCount: 0,
          initialized: false
        }
      });
    });

    it('should handle errors during health check', async () => {
      connectionManager.isConnected.mockReturnValue(true);
      projectManager.listProjectSpaces.mockRejectedValue(new Error('Failed to list projects'));
      
      const result = await databaseService.healthCheck();
      
      expect(result).toEqual({
        status: 'unhealthy',
        error: 'Failed to list projects'
      });
    });
  });

  describe('ensureInitialized', () => {
    it('should throw error when not initialized', async () => {
      await expect(databaseService.createProjectSpace('/test/path'))
        .rejects.toThrow('Database service is not initialized');
    });

    it('should not throw error when initialized', async () => {
      connectionManager.connect.mockResolvedValue(true);
      await databaseService.initialize();
      projectManager.createProjectSpace.mockResolvedValue(true);
      
      await expect(databaseService.createProjectSpace('/test/path')).resolves.toBe(true);
    });
  });
});