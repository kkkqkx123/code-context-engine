import { SqliteStateManager, ProjectState } from '../SqliteStateManager';
import { SqliteDatabaseService } from '../SqliteDatabaseService';
import { LoggerService } from '../../../utils/LoggerService';
import { TYPES } from '../../../types';

// Mock dependencies
jest.mock('../SqliteDatabaseService');
jest.mock('../../../utils/LoggerService');

describe('SqliteStateManager', () => {
  let stateManager: SqliteStateManager;
  let mockSqliteService: jest.Mocked<SqliteDatabaseService>;
  let mockLoggerService: jest.Mocked<LoggerService>;
  let mockStatement: jest.Mocked<any>;

  beforeEach(() => {
    // Create mock statement
    mockStatement = {
      run: jest.fn().mockReturnValue({ changes: 1, lastInsertRowid: 1 }),
      get: jest.fn(),
      all: jest.fn()
    };

    // Create mock services
    mockSqliteService = {
      prepare: jest.fn().mockReturnValue(mockStatement),
      transaction: jest.fn((fn) => fn())
    } as any;

    mockLoggerService = {
      info: jest.fn(),
      error: jest.fn(),
      warn: jest.fn()
    } as any;

    stateManager = new SqliteStateManager(mockSqliteService, mockLoggerService);
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  describe('saveProjectState', () => {
    it('should save project state successfully', async () => {
      const state: ProjectState = {
        projectId: 'project123',
        vectorStatus: { status: 'ready', progress: 1.0 },
        graphStatus: { status: 'indexing', progress: 0.5 },
        indexingProgress: 0.75,
        totalFiles: 100,
        indexedFiles: 75,
        failedFiles: 2,
        lastUpdated: new Date()
      };

      const result = await stateManager.saveProjectState(state);

      expect(result).toBe(true);
      expect(mockSqliteService.prepare).toHaveBeenCalledWith(
        expect.stringContaining('INSERT OR REPLACE INTO project_status')
      );
      expect(mockStatement.run).toHaveBeenCalledWith(
        state.projectId,
        JSON.stringify(state.vectorStatus),
        JSON.stringify(state.graphStatus),
        state.indexingProgress,
        state.totalFiles,
        state.indexedFiles,
        state.failedFiles,
        state.lastUpdated.toISOString()
      );
    });

    it('should handle zero values correctly', async () => {
      const state: ProjectState = {
        projectId: 'project123',
        vectorStatus: {},
        graphStatus: {},
        indexingProgress: 0,
        totalFiles: 0,
        indexedFiles: 0,
        failedFiles: 0,
        lastUpdated: new Date()
      };

      const result = await stateManager.saveProjectState(state);

      expect(result).toBe(true);
      expect(mockStatement.run).toHaveBeenCalledWith(
        state.projectId,
        '{}',
        '{}',
        0,
        0,
        0,
        0,
        expect.any(String)
      );
    });

    it('should handle undefined values with defaults', async () => {
      const state: ProjectState = {
        projectId: 'project123',
        vectorStatus: {},
        graphStatus: {},
        indexingProgress: undefined as any,
        totalFiles: undefined as any,
        indexedFiles: undefined as any,
        failedFiles: undefined as any,
        lastUpdated: new Date()
      };

      const result = await stateManager.saveProjectState(state);

      expect(result).toBe(true);
      expect(mockStatement.run).toHaveBeenCalledWith(
        state.projectId,
        '{}',
        '{}',
        0, // default value
        0, // default value
        0, // default value
        0, // default value
        expect.any(String)
      );
    });

    it('should handle database errors', async () => {
      mockStatement.run.mockImplementation(() => {
        throw new Error('Database error');
      });

      const state: ProjectState = {
        projectId: 'project123',
        vectorStatus: {},
        graphStatus: {},
        indexingProgress: 0,
        totalFiles: 0,
        indexedFiles: 0,
        failedFiles: 0,
        lastUpdated: new Date()
      };

      const result = await stateManager.saveProjectState(state);

      expect(result).toBe(false);
      expect(mockLoggerService.error).toHaveBeenCalledWith(
        'Failed to save project state: project123',
        expect.any(Error)
      );
    });

    it('should return false when no changes made', async () => {
      mockStatement.run.mockReturnValue({ changes: 0, lastInsertRowid: 0 });

      const state: ProjectState = {
        projectId: 'project123',
        vectorStatus: {},
        graphStatus: {},
        indexingProgress: 0,
        totalFiles: 0,
        indexedFiles: 0,
        failedFiles: 0,
        lastUpdated: new Date()
      };

      const result = await stateManager.saveProjectState(state);

      expect(result).toBe(false);
    });
  });

  describe('getProjectState', () => {
    it('should get project state successfully', async () => {
      const mockDbResult = {
        project_id: 'project123',
        vector_status: '{"status": "ready", "progress": 1.0}',
        graph_status: '{"status": "indexing", "progress": 0.5}',
        indexing_progress: 0.75,
        total_files: 100,
        indexed_files: 75,
        failed_files: 2,
        last_updated: '2023-01-01T12:00:00.000Z'
      };

      mockStatement.get.mockReturnValue(mockDbResult);

      const result = await stateManager.getProjectState('project123');

      expect(result).toEqual({
        projectId: 'project123',
        vectorStatus: { status: 'ready', progress: 1.0 },
        graphStatus: { status: 'indexing', progress: 0.5 },
        indexingProgress: 0.75,
        totalFiles: 100,
        indexedFiles: 75,
        failedFiles: 2,
        lastUpdated: new Date('2023-01-01T12:00:00.000Z')
      });
      expect(mockSqliteService.prepare).toHaveBeenCalledWith(
        expect.stringContaining('SELECT * FROM project_status WHERE project_id = ?')
      );
      expect(mockStatement.get).toHaveBeenCalledWith('project123');
    });

    it('should return null when project state not found', async () => {
      mockStatement.get.mockReturnValue(undefined);

      const result = await stateManager.getProjectState('nonexistent');

      expect(result).toBe(null);
    });

    it('should handle database errors', async () => {
      mockStatement.get.mockImplementation(() => {
        throw new Error('Database error');
      });

      const result = await stateManager.getProjectState('project123');

      expect(result).toBe(null);
      expect(mockLoggerService.error).toHaveBeenCalledWith(
        'Failed to get project state: project123',
        expect.any(Error)
      );
    });
  });

  describe('getAllProjectStates', () => {
    it('should get all project states', async () => {
      const mockDbResults = [
        {
          project_id: 'project1',
          vector_status: '{"status": "ready"}',
          graph_status: '{"status": "ready"}',
          indexing_progress: 1.0,
          total_files: 50,
          indexed_files: 50,
          failed_files: 0,
          last_updated: '2023-01-01T12:00:00.000Z'
        },
        {
          project_id: 'project2',
          vector_status: '{"status": "indexing"}',
          graph_status: '{"status": "error"}',
          indexing_progress: 0.5,
          total_files: 100,
          indexed_files: 50,
          failed_files: 5,
          last_updated: '2023-01-02T12:00:00.000Z'
        }
      ];

      mockStatement.all.mockReturnValue(mockDbResults);

      const result = await stateManager.getAllProjectStates();

      expect(result).toHaveLength(2);
      expect(result[0]).toEqual({
        projectId: 'project1',
        vectorStatus: { status: 'ready' },
        graphStatus: { status: 'ready' },
        indexingProgress: 1.0,
        totalFiles: 50,
        indexedFiles: 50,
        failedFiles: 0,
        lastUpdated: new Date('2023-01-01T12:00:00.000Z')
      });
      expect(result[1]).toEqual({
        projectId: 'project2',
        vectorStatus: { status: 'indexing' },
        graphStatus: { status: 'error' },
        indexingProgress: 0.5,
        totalFiles: 100,
        indexedFiles: 50,
        failedFiles: 5,
        lastUpdated: new Date('2023-01-02T12:00:00.000Z')
      });
      expect(mockSqliteService.prepare).toHaveBeenCalledWith(
        expect.stringContaining('SELECT * FROM project_status ORDER BY last_updated DESC')
      );
    });

    it('should return empty array when no project states exist', async () => {
      mockStatement.all.mockReturnValue([]);

      const result = await stateManager.getAllProjectStates();

      expect(result).toEqual([]);
    });

    it('should handle database errors', async () => {
      mockStatement.all.mockImplementation(() => {
        throw new Error('Database error');
      });

      const result = await stateManager.getAllProjectStates();

      expect(result).toEqual([]);
      expect(mockLoggerService.error).toHaveBeenCalledWith(
        'Failed to get all project states',
        expect.any(Error)
      );
    });
  });

  describe('deleteProjectState', () => {
    it('should delete project state successfully', async () => {
      const result = await stateManager.deleteProjectState('project123');

      expect(result).toBe(true);
      expect(mockSqliteService.prepare).toHaveBeenCalledWith(
        expect.stringContaining('DELETE FROM project_status WHERE project_id = ?')
      );
      expect(mockStatement.run).toHaveBeenCalledWith('project123');
    });

    it('should return false when no project state to delete', async () => {
      mockStatement.run.mockReturnValue({ changes: 0 });

      const result = await stateManager.deleteProjectState('nonexistent');

      expect(result).toBe(false);
    });

    it('should handle database errors', async () => {
      mockStatement.run.mockImplementation(() => {
        throw new Error('Database error');
      });

      const result = await stateManager.deleteProjectState('project123');

      expect(result).toBe(false);
      expect(mockLoggerService.error).toHaveBeenCalledWith(
        'Failed to delete project state: project123',
        expect.any(Error)
      );
    });
  });

  describe('batchSaveProjectStates', () => {
    it('should batch save project states successfully', async () => {
      const states: ProjectState[] = [
        {
          projectId: 'project1',
          vectorStatus: { status: 'ready' },
          graphStatus: { status: 'ready' },
          indexingProgress: 1.0,
          totalFiles: 50,
          indexedFiles: 50,
          failedFiles: 0,
          lastUpdated: new Date()
        },
        {
          projectId: 'project2',
          vectorStatus: { status: 'indexing' },
          graphStatus: { status: 'indexing' },
          indexingProgress: 0.5,
          totalFiles: 100,
          indexedFiles: 50,
          failedFiles: 2,
          lastUpdated: new Date()
        }
      ];

      const result = await stateManager.batchSaveProjectStates(states);

      expect(result).toBe(true);
      expect(mockSqliteService.transaction).toHaveBeenCalled();
      expect(mockStatement.run).toHaveBeenCalledTimes(2);
      expect(mockLoggerService.info).toHaveBeenCalledWith(
        'Batch saved 2 project states'
      );
    });

    it('should return true for empty array', async () => {
      const result = await stateManager.batchSaveProjectStates([]);

      expect(result).toBe(true);
      expect(mockSqliteService.transaction).not.toHaveBeenCalled();
    });

    it('should handle transaction errors', async () => {
      const states: ProjectState[] = [
        {
          projectId: 'project1',
          vectorStatus: {},
          graphStatus: {},
          indexingProgress: 0,
          totalFiles: 0,
          indexedFiles: 0,
          failedFiles: 0,
          lastUpdated: new Date()
        }
      ];

      mockSqliteService.transaction.mockImplementation(() => {
        throw new Error('Transaction failed');
      });

      const result = await stateManager.batchSaveProjectStates(states);

      expect(result).toBe(false);
      expect(mockLoggerService.error).toHaveBeenCalledWith(
        'Failed to batch save project states',
        expect.any(Error)
      );
    });
  });

  describe('updateProjectStateFields', () => {
    it('should update project state fields successfully', async () => {
      const existingState: ProjectState = {
        projectId: 'project123',
        vectorStatus: { status: 'ready' },
        graphStatus: { status: 'ready' },
        indexingProgress: 0.5,
        totalFiles: 100,
        indexedFiles: 50,
        failedFiles: 2,
        lastUpdated: new Date('2023-01-01T12:00:00.000Z')
      };

      const updates = {
        indexingProgress: 0.75,
        indexedFiles: 75
      };

      // Mock getProjectState to return existing state
      mockStatement.get.mockReturnValue({
        project_id: existingState.projectId,
        vector_status: JSON.stringify(existingState.vectorStatus),
        graph_status: JSON.stringify(existingState.graphStatus),
        indexing_progress: existingState.indexingProgress,
        total_files: existingState.totalFiles,
        indexed_files: existingState.indexedFiles,
        failed_files: existingState.failedFiles,
        last_updated: existingState.lastUpdated.toISOString()
      });

      const result = await stateManager.updateProjectStateFields('project123', updates);

      expect(result).toBe(true);
      expect(mockStatement.run).toHaveBeenCalledWith(
        'project123',
        JSON.stringify(existingState.vectorStatus),
        JSON.stringify(existingState.graphStatus),
        updates.indexingProgress,
        existingState.totalFiles,
        updates.indexedFiles,
        existingState.failedFiles,
        expect.any(String) // updated lastUpdated
      );
    });

    it('should return false when project state does not exist', async () => {
      mockStatement.get.mockReturnValue(undefined);

      const result = await stateManager.updateProjectStateFields('nonexistent', {
        indexingProgress: 0.75
      });

      expect(result).toBe(false);
    });

    it('should handle update errors', async () => {
      mockStatement.get.mockReturnValue({
        project_id: 'project123',
        vector_status: '{}',
        graph_status: '{}',
        indexing_progress: 0,
        total_files: 0,
        indexed_files: 0,
        failed_files: 0,
        last_updated: new Date().toISOString()
      });

      mockStatement.run.mockImplementation(() => {
        throw new Error('Database error');
      });

      const result = await stateManager.updateProjectStateFields('project123', {
        indexingProgress: 0.75
      });

      expect(result).toBe(false);
      expect(mockLoggerService.error).toHaveBeenCalledWith(
        'Failed to save project state: project123',
        expect.any(Error)
      );
    });
  });

  describe('getProjectStateStats', () => {
    it('should get project state statistics successfully', async () => {
      const mockDbResult = {
        total_projects: 10,
        indexing_projects: 3,
        error_projects: 1
      };

      mockStatement.get.mockReturnValue(mockDbResult);

      const result = await stateManager.getProjectStateStats();

      expect(result).toEqual({
        totalProjects: 10,
        activeProjects: 9, // total - error
        indexingProjects: 3,
        errorProjects: 1
      });
      expect(mockSqliteService.prepare).toHaveBeenCalledWith(
        expect.stringContaining('SELECT')
      );
    });

    it('should handle database errors gracefully', async () => {
      mockStatement.get.mockImplementation(() => {
        throw new Error('Database error');
      });

      const result = await stateManager.getProjectStateStats();

      expect(result).toEqual({
        totalProjects: 0,
        activeProjects: 0,
        indexingProjects: 0,
        errorProjects: 0
      });
      expect(mockLoggerService.error).toHaveBeenCalledWith(
        'Failed to get project state stats',
        expect.any(Error)
      );
    });

    it('should handle null results', async () => {
      mockStatement.get.mockReturnValue(null);

      const result = await stateManager.getProjectStateStats();

      expect(result).toEqual({
        totalProjects: 0,
        activeProjects: 0,
        indexingProjects: 0,
        errorProjects: 0
      });
    });
  });
});