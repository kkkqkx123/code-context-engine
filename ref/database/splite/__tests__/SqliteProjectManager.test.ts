import { SqliteProjectManager, Project, FileIndexState, ProjectStatus } from '../SqliteProjectManager';
import { SqliteDatabaseService } from '../SqliteDatabaseService';
import { TYPES } from '../../../types';

// Mock dependencies
jest.mock('../SqliteDatabaseService');

describe('SqliteProjectManager', () => {
  let projectManager: SqliteProjectManager;
  let mockSqliteService: jest.Mocked<SqliteDatabaseService>;
  let mockStatement: jest.Mocked<any>;
  let mockLogger: jest.Mocked<any>;

  beforeEach(() => {
    // Create mock statement
    mockStatement = {
      run: jest.fn().mockReturnValue({ changes: 1, lastInsertRowid: 1 }),
      get: jest.fn(),
      all: jest.fn()
    };

    // Create mock SQLite service
    mockSqliteService = {
      prepare: jest.fn().mockReturnValue(mockStatement),
      transaction: jest.fn((fn) => fn()),
      exec: jest.fn()
    } as any;

    // Create mock logger
    mockLogger = {
      info: jest.fn(),
      warn: jest.fn(),
      error: jest.fn(),
      debug: jest.fn()
    } as any;

    projectManager = new SqliteProjectManager(mockSqliteService, mockLogger);
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  describe('createProjectSpace', () => {
    it('should create project space successfully', async () => {
      const projectPath = '/test/project';
      const config = {
        name: 'Test Project',
        description: 'A test project',
        collectionName: 'test_collection',
        spaceName: 'test_space'
      };

      const result = await projectManager.createProjectSpace(projectPath, config);

      expect(result).toBe(true);
      expect(mockSqliteService.prepare).toHaveBeenCalledWith(
        expect.stringContaining('INSERT INTO projects')
      );
      expect(mockStatement.run).toHaveBeenCalledWith(
        expect.any(String), // project ID
        projectPath,
        config.name,
        config.description,
        config.collectionName,
        config.spaceName,
        expect.any(String), // created_at
        expect.any(String), // updated_at
        'active',
        expect.any(String), // settings JSON
        expect.any(String)  // metadata JSON
      );
    });

    it('should create project with default values', async () => {
      const projectPath = '/test/project';
      
      const result = await projectManager.createProjectSpace(projectPath);

      expect(result).toBe(true);
      expect(mockStatement.run).toHaveBeenCalledWith(
        expect.any(String),
        projectPath,
        'project', // default name from path
        undefined,
        undefined,
        undefined,
        expect.any(String),
        expect.any(String),
        'active',
        '{}',
        '{}'
      );
    });

    it('should handle creation errors', async () => {
      mockStatement.run.mockImplementation(() => {
        throw new Error('Database error');
      });

      const result = await projectManager.createProjectSpace('/test/project');

      expect(result).toBe(false);
    });
  });

  describe('deleteProjectSpace', () => {
    it('should delete project space successfully', async () => {
      const projectPath = '/test/project';
      const mockProject = {
        id: 'project123',
        path: projectPath,
        name: 'Test Project',
        status: 'active' as const,
        created_at: new Date(),
        updated_at: new Date()
      };

      mockStatement.get.mockReturnValue(mockProject);

      const result = await projectManager.deleteProjectSpace(projectPath);

      expect(result).toBe(true);
      expect(mockSqliteService.prepare).toHaveBeenCalledWith(
        'DELETE FROM projects WHERE id = ?'
      );
      expect(mockStatement.run).toHaveBeenCalledWith('project123');
    });

    it('should return false when project does not exist', async () => {
      mockStatement.get.mockReturnValue(undefined);

      const result = await projectManager.deleteProjectSpace('/nonexistent/project');

      expect(result).toBe(false);
      expect(mockStatement.run).not.toHaveBeenCalled();
    });

    it('should handle deletion errors', async () => {
      mockStatement.get.mockReturnValue({ id: 'project123' });
      mockStatement.run.mockImplementation(() => {
        throw new Error('Database error');
      });

      const result = await projectManager.deleteProjectSpace('/test/project');

      expect(result).toBe(false);
    });
  });

  describe('getProjectSpaceInfo', () => {
    it('should return project space info', async () => {
      const projectPath = '/test/project';
      const mockProject = {
        id: 'project123',
        path: projectPath,
        name: 'Test Project',
        status: 'active' as const,
        created_at: new Date(),
        updated_at: new Date()
      };

      const mockStatus = {
        project_id: 'project123',
        vector_status: '{"status": "ready"}',
        graph_status: '{"status": "ready"}',
        indexing_progress: 0.5,
        total_files: 100,
        indexed_files: 50,
        failed_files: 2,
        last_updated: new Date().toISOString()
      };

      mockStatement.get
        .mockReturnValueOnce(mockProject) // project query
        .mockReturnValueOnce(mockStatus) // status query
        .mockReturnValueOnce({ count: 50 }) // indexed files
        .mockReturnValueOnce({ count: 30 }) // pending files
        .mockReturnValueOnce({ count: 2 }); // failed files

      const result = await projectManager.getProjectSpaceInfo(projectPath);

      expect(result).toEqual({
        project: mockProject,
        status: expect.objectContaining({
          project_id: 'project123',
          indexing_progress: 0.5
        }),
        fileStats: {
          indexed: 50,
          pending: 30,
          failed: 2,
          total: 82
        }
      });
    });

    it('should return null when project does not exist', async () => {
      mockStatement.get.mockReturnValue(undefined);

      const result = await projectManager.getProjectSpaceInfo('/nonexistent/project');

      expect(result).toBe(null);
    });

    it('should return default status when no status exists', async () => {
      const projectPath = '/test/project';
      const mockProject = {
        id: 'project123',
        path: projectPath,
        name: 'Test Project',
        status: 'active' as const,
        created_at: new Date(),
        updated_at: new Date()
      };

      mockStatement.get
        .mockReturnValueOnce(mockProject) // project query
        .mockReturnValueOnce(undefined) // no status
        .mockReturnValueOnce({ count: 0 }) // indexed files
        .mockReturnValueOnce({ count: 0 }) // pending files
        .mockReturnValueOnce({ count: 0 }); // failed files

      const result = await projectManager.getProjectSpaceInfo(projectPath);

      expect(result.status).toEqual({
        project_id: 'project123',
        vector_status: {},
        graph_status: {},
        indexing_progress: 0,
        total_files: 0,
        indexed_files: 0,
        failed_files: 0,
        last_updated: expect.any(Date)
      });
    });
  });

  describe('clearProjectSpace', () => {
    it('should clear project space successfully', async () => {
      const projectPath = '/test/project';
      const mockProject = {
        id: 'project123',
        path: projectPath,
        name: 'Test Project',
        status: 'active' as const,
        created_at: new Date(),
        updated_at: new Date()
      };

      mockStatement.get.mockReturnValue(mockProject);

      const result = await projectManager.clearProjectSpace(projectPath);

      expect(result).toBe(true);
      expect(mockSqliteService.transaction).toHaveBeenCalled();
      expect(mockSqliteService.prepare).toHaveBeenCalledWith(
        'DELETE FROM file_index_states WHERE project_id = ?'
      );
      expect(mockSqliteService.prepare).toHaveBeenCalledWith(
        'DELETE FROM project_status WHERE project_id = ?'
      );
      expect(mockSqliteService.prepare).toHaveBeenCalledWith(
        'DELETE FROM file_change_history WHERE project_id = ?'
      );
    });

    it('should return false when project does not exist', async () => {
      mockStatement.get.mockReturnValue(undefined);

      const result = await projectManager.clearProjectSpace('/nonexistent/project');

      expect(result).toBe(false);
      expect(mockSqliteService.transaction).not.toHaveBeenCalled();
    });

    it('should handle clear errors', async () => {
      mockStatement.get.mockReturnValue({ id: 'project123' });
      mockSqliteService.transaction.mockImplementation(() => {
        throw new Error('Transaction failed');
      });

      const result = await projectManager.clearProjectSpace('/test/project');

      expect(result).toBe(false);
    });
  });

  describe('listProjectSpaces', () => {
    it('should list all project spaces', async () => {
      const mockProjects = [
        {
          id: 'project1',
          path: '/test/project1',
          name: 'Project 1',
          status: 'active' as const,
          created_at: new Date('2023-01-01'),
          last_indexed_at: new Date('2023-01-02')
        },
        {
          id: 'project2',
          path: '/test/project2',
          name: 'Project 2',
          status: 'inactive' as const,
          created_at: new Date('2023-01-03'),
          last_indexed_at: null
        }
      ];

      mockStatement.all.mockReturnValue(mockProjects);

      const result = await projectManager.listProjectSpaces();

      expect(result).toEqual([
        {
          id: 'project1',
          path: '/test/project1',
          name: 'Project 1',
          status: 'active',
          created_at: mockProjects[0].created_at,
          last_indexed_at: mockProjects[0].last_indexed_at
        },
        {
          id: 'project2',
          path: '/test/project2',
          name: 'Project 2',
          status: 'inactive',
          created_at: mockProjects[1].created_at,
          last_indexed_at: null
        }
      ]);
      expect(mockSqliteService.prepare).toHaveBeenCalledWith(
        'SELECT * FROM projects ORDER BY created_at DESC'
      );
    });

    it('should return empty array when no projects exist', async () => {
      mockStatement.all.mockReturnValue([]);

      const result = await projectManager.listProjectSpaces();

      expect(result).toEqual([]);
    });
  });

  describe('project data operations', () => {
    it('should insert project data', async () => {
      const projectPath = '/test/project';
      const data = { key: 'value' };

      const result = await projectManager.insertProjectData(projectPath, data);

      expect(result).toBe(true);
    });

    it('should update project data', async () => {
      const projectPath = '/test/project';
      const id = 'data123';
      const data = { key: 'updated_value' };

      const result = await projectManager.updateProjectData(projectPath, id, data);

      expect(result).toBe(true);
    });

    it('should delete project data', async () => {
      const projectPath = '/test/project';
      const id = 'data123';

      const result = await projectManager.deleteProjectData(projectPath, id);

      expect(result).toBe(true);
    });

    it('should search project data', async () => {
      const projectPath = '/test/project';
      const query = { search: 'test' };

      const result = await projectManager.searchProjectData(projectPath, query);

      expect(result).toEqual([]);
    });

    it('should get project data by ID', async () => {
      const projectPath = '/test/project';
      const id = 'data123';

      const result = await projectManager.getProjectDataById(projectPath, id);

      expect(result).toBe(null);
    });
  });

  describe('SQLite-specific methods', () => {
    describe('getProjectById', () => {
      it('should return project by ID', async () => {
        const mockProject = {
          id: 'project123',
          path: '/test/project',
          name: 'Test Project',
          status: 'active' as const,
          created_at: new Date(),
          updated_at: new Date()
        };

        mockStatement.get.mockReturnValue(mockProject);

        const result = await projectManager.getProjectById('project123');

        expect(result).toEqual(mockProject);
        expect(mockSqliteService.prepare).toHaveBeenCalledWith(
          'SELECT * FROM projects WHERE id = ?'
        );
        expect(mockStatement.get).toHaveBeenCalledWith('project123');
      });

      it('should return null when project not found', async () => {
        mockStatement.get.mockReturnValue(undefined);

        const result = await projectManager.getProjectById('nonexistent');

        expect(result).toBe(null);
      });
    });

    describe('updateProjectStatus', () => {
      it('should update project status successfully', async () => {
        const status: ProjectStatus = {
          project_id: 'project123',
          vector_status: { status: 'indexing' },
          graph_status: { status: 'ready' },
          indexing_progress: 0.5,
          total_files: 100,
          indexed_files: 50,
          failed_files: 2,
          last_updated: new Date()
        };

        const result = await projectManager.updateProjectStatus('project123', status);

        expect(result).toBe(true);
        expect(mockSqliteService.prepare).toHaveBeenCalledWith(
          expect.stringContaining('INSERT OR REPLACE INTO project_status')
        );
        expect(mockStatement.run).toHaveBeenCalledWith(
          'project123',
          JSON.stringify(status.vector_status),
          JSON.stringify(status.graph_status),
          status.indexing_progress,
          status.total_files,
          status.indexed_files,
          status.failed_files,
          expect.any(String)
        );
      });

      it('should handle update errors', async () => {
        mockStatement.run.mockImplementation(() => {
          throw new Error('Database error');
        });

        const status: ProjectStatus = {
          project_id: 'project123',
          vector_status: {},
          graph_status: {},
          indexing_progress: 0,
          total_files: 0,
          indexed_files: 0,
          failed_files: 0,
          last_updated: new Date()
        };

        const result = await projectManager.updateProjectStatus('project123', status);

        expect(result).toBe(false);
      });
    });

    describe('getFileIndexStates', () => {
      it('should return file index states for project', async () => {
        const mockFileStates: FileIndexState[] = [
          {
            id: 1,
            project_id: 'project123',
            file_path: '/test/file1.js',
            relative_path: 'file1.js',
            content_hash: 'hash1',
            last_modified: new Date(),
            indexing_version: 1,
            status: 'indexed' as const,
            created_at: new Date(),
            updated_at: new Date()
          },
          {
            id: 2,
            project_id: 'project123',
            file_path: '/test/file2.js',
            relative_path: 'file2.js',
            content_hash: 'hash2',
            last_modified: new Date(),
            indexing_version: 1,
            status: 'pending' as const,
            created_at: new Date(),
            updated_at: new Date()
          }
        ];

        mockStatement.all.mockReturnValue(mockFileStates);

        const result = await projectManager.getFileIndexStates('project123');

        expect(result).toEqual(mockFileStates);
        expect(mockSqliteService.prepare).toHaveBeenCalledWith(
          'SELECT * FROM file_index_states WHERE project_id = ? ORDER BY file_path'
        );
        expect(mockStatement.all).toHaveBeenCalledWith('project123');
      });
    });
  });

  describe('event listeners', () => {
    it('should add and remove event listeners', () => {
      const mockListener1 = jest.fn();
      const mockListener2 = jest.fn();

      const subscription1 = projectManager.subscribe('space_created', mockListener1);
      const subscription2 = projectManager.subscribe('space_created', mockListener2);
      const subscription3 = projectManager.subscribe('error', mockListener1);

      subscription1.unsubscribe();

      // Verify listeners can be managed
      expect(() => subscription1.unsubscribe()).not.toThrow();
    });

    it('should handle errors in event listeners gracefully', async () => {
      const errorListener = jest.fn(() => {
        throw new Error('Listener error');
      });

      const consoleSpy = jest.spyOn(console, 'error').mockImplementation();

      const subscription = projectManager.subscribe('error', errorListener);

      // Trigger an error by making database operation fail
      mockStatement.run.mockImplementation(() => {
        throw new Error('Database error');
      });

      await projectManager.createProjectSpace('/test/project');

      expect(consoleSpy).toHaveBeenCalled();
      expect(errorListener).toHaveBeenCalled();

      consoleSpy.mockRestore();
      subscription.unsubscribe();
    });
  });
});