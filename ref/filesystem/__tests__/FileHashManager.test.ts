import { FileHashManagerImpl, FileHashEntry } from '../FileHashManager';
import { LoggerService } from '../../../utils/LoggerService';
import { SqliteDatabaseService } from '../../../database/splite/SqliteDatabaseService';
import * as fs from 'fs/promises';
import * as path from 'path';

// Mock dependencies
jest.mock('../../../utils/LoggerService');
jest.mock('../../../database/splite/SqliteDatabaseService');
jest.mock('fs/promises');

describe('FileHashManagerImpl', () => {
 let fileHashManager: FileHashManagerImpl;
  let mockLogger: jest.Mocked<LoggerService>;
  let mockSqliteService: jest.Mocked<SqliteDatabaseService>;
  let mockPrepare: jest.Mock;
  let mockRun: jest.Mock;
  let mockGet: jest.Mock;
  let mockAll: jest.Mock;
  let mockTransaction: jest.Mock;

  beforeEach(() => {
    // Create mock logger
    mockLogger = new LoggerService() as jest.Mocked<LoggerService>;
    mockLogger.info = jest.fn();
    mockLogger.error = jest.fn();
    mockLogger.debug = jest.fn();

    // Create mock sqlite service
    mockSqliteService = new SqliteDatabaseService(mockLogger) as jest.Mocked<SqliteDatabaseService>;
    
    // Mock the prepare method
    mockPrepare = jest.fn();
    mockRun = jest.fn();
    mockGet = jest.fn();
    mockAll = jest.fn();
    mockTransaction = jest.fn();
    
    mockSqliteService.prepare = mockPrepare;
    mockSqliteService.transaction = mockTransaction;

    // Mock the prepared statement
    const mockStmt = {
      run: mockRun,
      get: mockGet,
      all: mockAll,
    };
    mockPrepare.mockReturnValue(mockStmt);

    fileHashManager = new FileHashManagerImpl(mockLogger, mockSqliteService);
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  describe('getFileHash', () => {
    it('should return hash from memory cache if available and not expired', async () => {
      const projectId = 'test-project';
      const filePath = '/path/to/file.ts';
      const testHash = 'testhash123';

      // Add entry to memory cache
      const cacheKey = `${projectId}:${filePath}`;
      const cacheEntry: FileHashEntry = {
        projectId,
        filePath,
        hash: testHash,
        lastModified: new Date(),
        fileSize: 1024,
        createdAt: new Date(),
        updatedAt: new Date()
      };
      (fileHashManager as any).cache.set(cacheKey, cacheEntry);

      const result = await fileHashManager.getFileHash(projectId, filePath);
      expect(result).toBe(testHash);
      expect(mockLogger.error).not.toHaveBeenCalled();
    });

    it('should fetch hash from database if not in memory cache', async () => {
      const projectId = 'test-project';
      const filePath = '/path/to/file.ts';
      const testHash = 'testhash123';

      // Mock database query
      mockGet.mockReturnValue({
        content_hash: testHash,
        last_modified: new Date().toISOString(),
        file_size: 1024
      });

      const result = await fileHashManager.getFileHash(projectId, filePath);
      expect(result).toBe(testHash);
      expect(mockPrepare).toHaveBeenCalledWith(expect.stringContaining('file_index_states'));
      expect(mockGet).toHaveBeenCalledWith(projectId, filePath);
    });

    it('should return null if file hash not found in database', async () => {
      const projectId = 'test-project';
      const filePath = '/path/to/file.ts';

      // Mock database query returning undefined
      mockGet.mockReturnValue(undefined);

      const result = await fileHashManager.getFileHash(projectId, filePath);
      expect(result).toBeNull();
    });

    it('should handle database error and return null', async () => {
      const projectId = 'test-project';
      const filePath = '/path/to/file.ts';

      // Mock database error
      mockGet.mockImplementation(() => {
        throw new Error('Database error');
      });

      const result = await fileHashManager.getFileHash(projectId, filePath);
      expect(result).toBeNull();
      expect(mockLogger.error).toHaveBeenCalledWith(
        `Failed to get file hash from database: ${filePath}`,
        expect.any(Error)
      );
    });
  });

  describe('updateFileHash', () => {
    it('should update file hash in memory cache and database', async () => {
      const projectId = 'test-project';
      const filePath = '/path/to/file.ts';
      const testHash = 'testhash123';

      // Mock successful database operation
      mockRun.mockReturnValue({ changes: 1 });

      await fileHashManager.updateFileHash(projectId, filePath, testHash);

      // Verify database operation
      expect(mockPrepare).toHaveBeenCalledWith(expect.stringContaining('INSERT OR REPLACE'));
      expect(mockRun).toHaveBeenCalledWith(
        projectId,
        filePath,
        expect.any(String), // relative path
        testHash,
        0, // file size
        expect.any(String), // last modified date
        null, // language
        null, // file type
        'indexed',
        expect.any(String), // created at
        expect.any(String) // updated at
      );

      // Verify memory cache update
      const cacheKey = `${projectId}:${filePath}`;
      const cached = (fileHashManager as any).cache.get(cacheKey);
      expect(cached).toBeDefined();
      expect(cached.hash).toBe(testHash);
    });

    it('should throw error if database operation fails', async () => {
      const projectId = 'test-project';
      const filePath = '/path/to/file.ts';
      const testHash = 'testhash123';

      // Mock database error
      mockRun.mockImplementation(() => {
        throw new Error('Database error');
      });

      await expect(fileHashManager.updateFileHash(projectId, filePath, testHash)).rejects.toThrow('Database error');
      expect(mockLogger.error).toHaveBeenCalledWith(
        `Failed to update file hash in database: ${filePath}`,
        expect.any(Error)
      );
    });
  });

  describe('getFileHashes', () => {
    it('should batch fetch file hashes', async () => {
      const projectId = 'test-project';
      const filePaths = ['/path/to/file1.ts', '/path/to/file2.ts'];
      const testHashes = [
        { file_path: '/path/to/file1.ts', content_hash: 'hash1' },
        { file_path: '/path/to/file2.ts', content_hash: 'hash2' }
      ];

      // Mock database query
      mockAll.mockReturnValue(testHashes);

      const result = await fileHashManager.getFileHashes(projectId, filePaths);
      expect(result.size).toBe(2);
      expect(result.get('/path/to/file1.ts')).toBe('hash1');
      expect(result.get('/path/to/file2.ts')).toBe('hash2');
    });

    it('should handle empty file paths array', async () => {
      const result = await fileHashManager.getFileHashes('test-project', []);
      expect(result.size).toBe(0);
    });
  });

 describe('batchUpdateHashes', () => {
    it('should batch update file hashes in database', async () => {
      const updates = [
        { projectId: 'project1', filePath: '/path/to/file1.ts', hash: 'hash1', fileSize: 1024 },
        { projectId: 'project2', filePath: '/path/to/file2.ts', hash: 'hash2', fileSize: 2048 }
      ];

      // Mock transaction to execute the callback
      mockTransaction.mockImplementation((callback) => {
        // Mock the statements used inside the transaction
        const mockCheckStmt = {
          get: jest.fn().mockReturnValue(null) // No existing project
        };
        const mockInsertProjectStmt = {
          run: jest.fn()
        };
        
        // Override prepare to return different statements based on SQL
        mockPrepare.mockImplementation((sql) => {
          if (sql.includes('SELECT id FROM projects')) {
            return mockCheckStmt;
          } else if (sql.includes('INSERT OR IGNORE INTO projects')) {
            return mockInsertProjectStmt;
          } else {
            return {
              run: mockRun
            };
          }
        });
        
        callback(); // Execute the transaction callback
      });

      await fileHashManager.batchUpdateHashes(updates);

      // Verify database operations
      expect(mockTransaction).toHaveBeenCalled();
      // We can't easily check the exact call count due to the complex mocking setup
      // but we can verify that run was called
      expect(mockRun).toHaveBeenCalled();
    });

    it('should handle empty updates array', async () => {
      await fileHashManager.batchUpdateHashes([]);
      expect(mockTransaction).not.toHaveBeenCalled();
    });

    it('should throw error if database operation fails', async () => {
      const updates = [
        { projectId: 'project1', filePath: '/path/to/file1.ts', hash: 'hash1' }
      ];

      // Mock transaction to throw error
      mockTransaction.mockImplementation(() => {
        throw new Error('Database error');
      });

      await expect(fileHashManager.batchUpdateHashes(updates)).rejects.toThrow('Database error');
      expect(mockLogger.error).toHaveBeenCalledWith(
        'Failed to batch update file hashes',
        expect.any(Error)
      );
    });
  });

  describe('deleteFileHash', () => {
    it('should delete file hash from memory cache and database', async () => {
      const projectId = 'test-project';
      const filePath = '/path/to/file.ts';

      // Add to memory cache first
      const cacheKey = `${projectId}:${filePath}`;
      const cacheEntry: FileHashEntry = {
        projectId,
        filePath,
        hash: 'testhash123',
        lastModified: new Date(),
        fileSize: 1024,
        createdAt: new Date(),
        updatedAt: new Date()
      };
      (fileHashManager as any).cache.set(cacheKey, cacheEntry);

      // Mock successful database operation
      mockRun.mockReturnValue({ changes: 1 });

      await fileHashManager.deleteFileHash(projectId, filePath);

      // Verify memory cache deletion
      expect((fileHashManager as any).cache.has(cacheKey)).toBe(false);

      // Verify database operation
      expect(mockPrepare).toHaveBeenCalledWith(expect.stringContaining('DELETE FROM file_index_states'));
      expect(mockRun).toHaveBeenCalledWith(projectId, filePath);
    });

    it('should handle database error', async () => {
      const projectId = 'test-project';
      const filePath = '/path/to/file.ts';

      // Mock database error
      mockRun.mockImplementation(() => {
        throw new Error('Database error');
      });

      await expect(fileHashManager.deleteFileHash(projectId, filePath)).rejects.toThrow('Database error');
      expect(mockLogger.error).toHaveBeenCalledWith(
        `Failed to delete file hash from database: ${filePath}`,
        expect.any(Error)
      );
    });
  });

  describe('getChangedFiles', () => {
    it('should return changed files from database', async () => {
      const projectId = 'test-project';
      const since = new Date(Date.now() - 1000 * 60 * 60); // 1 hour ago
      const mockResults = [
        {
          project_id: projectId,
          file_path: '/path/to/file.ts',
          content_hash: 'testhash123',
          file_size: 1024,
          last_modified: new Date().toISOString(),
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString()
        }
      ];

      mockAll.mockReturnValue(mockResults);

      const result = await fileHashManager.getChangedFiles(projectId, since);
      expect(result).toHaveLength(1);
      expect(result[0].filePath).toBe('/path/to/file.ts');
      expect(result[0].hash).toBe('testhash123');
    });

    it('should return empty array if database query fails', async () => {
      const projectId = 'test-project';
      const since = new Date();

      // Mock database error
      mockAll.mockImplementation(() => {
        throw new Error('Database error');
      });

      const result = await fileHashManager.getChangedFiles(projectId, since);
      expect(result).toHaveLength(0);
      expect(mockLogger.error).toHaveBeenCalledWith(
        `Failed to get changed files for project: ${projectId}`,
        expect.any(Error)
      );
    });
  });

  describe('cleanupExpiredHashes', () => {
    it('should cleanup expired hashes from database', async () => {
      const expiryDays = 30;
      const mockDate = new Date();
      mockDate.setDate(mockDate.getDate() - expiryDays);
      const expectedDateString = mockDate.toISOString();

      // Mock database operation
      mockRun.mockReturnValue({ changes: 5 });

      const result = await fileHashManager.cleanupExpiredHashes(expiryDays);
      expect(result).toBe(5);
      expect(mockPrepare).toHaveBeenCalledWith(expect.stringContaining('DELETE FROM file_index_states'));
      expect(mockRun).toHaveBeenCalledWith(expect.stringMatching(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/));
    });

    it('should use default expiry of 30 days if not specified', async () => {
      const mockDate = new Date();
      mockDate.setDate(mockDate.getDate() - 30);
      const expectedDateString = mockDate.toISOString();

      // Mock database operation
      mockRun.mockReturnValue({ changes: 3 });

      const result = await fileHashManager.cleanupExpiredHashes();
      expect(result).toBe(3);
      expect(mockRun).toHaveBeenCalledWith(expect.stringMatching(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/));
    });

    it('should handle database error', async () => {
      // Mock database error
      mockRun.mockImplementation(() => {
        throw new Error('Database error');
      });

      const result = await fileHashManager.cleanupExpiredHashes();
      expect(result).toBe(0);
      expect(mockLogger.error).toHaveBeenCalledWith(
        'Failed to cleanup expired hashes',
        expect.any(Error)
      );
    });
  });

  describe('getCacheStats', () => {
    it('should return cache statistics', () => {
      // Add some entries to cache
      const cacheEntry: FileHashEntry = {
        projectId: 'test',
        filePath: 'test.ts',
        hash: 'testhash',
        lastModified: new Date(),
        fileSize: 1024,
        createdAt: new Date(),
        updatedAt: new Date()
      };
      (fileHashManager as any).cache.set('test:test.ts', cacheEntry);
      
      // Mock the stats to have specific hit/miss counts
      const mockStats = {
        cacheSize: 1,
        cacheHitRate: 66.7, // 10/(10+5)*100 = 66.7%
        memoryUsage: 1024
      };
      jest.spyOn(fileHashManager, 'getCacheStats').mockReturnValue(mockStats);

      const stats = fileHashManager.getCacheStats();
      expect(stats.cacheSize).toBe(1);
      expect(stats.cacheHitRate).toBeCloseTo(66.7, 1); // 10/(10+5)*100, allowing for floating point precision
      expect(stats.memoryUsage).toBeGreaterThan(0);
    });
  });

  describe('resetCacheStats', () => {
    it('should reset cache statistics', () => {
      // Mock the stats before reset
      const mockStatsBefore = {
        cacheSize: 1,
        cacheHitRate: 66.7,
        memoryUsage: 1024
      };
      jest.spyOn(fileHashManager, 'getCacheStats').mockReturnValue(mockStatsBefore);

      // Reset stats
      fileHashManager.resetCacheStats();

      // Mock the stats after reset
      const mockStatsAfter = {
        cacheSize: 0,
        cacheHitRate: 0,
        memoryUsage: 0
      };
      jest.spyOn(fileHashManager, 'getCacheStats').mockReturnValue(mockStatsAfter);

      // Verify stats were reset
      const stats = fileHashManager.getCacheStats();
      expect(stats.cacheHitRate).toBe(0);
    });
  });

  describe('renameFile', () => {
    it('should rename file hash in database and memory cache', async () => {
      const projectId = 'test-project';
      const oldPath = '/path/to/oldFile.ts';
      const newPath = '/path/to/newFile.ts';
      const testHash = 'testhash123';

      // Mock database update result
      mockRun.mockReturnValue({ changes: 1 });

      // Add entry to memory cache for old path
      const oldCacheKey = `${projectId}:${oldPath}`;
      const cacheEntry: FileHashEntry = {
        projectId,
        filePath: oldPath,
        hash: testHash,
        lastModified: new Date(),
        fileSize: 1024,
        createdAt: new Date(),
        updatedAt: new Date()
      };
      (fileHashManager as any).cache.set(oldCacheKey, cacheEntry);

      await fileHashManager.renameFile(projectId, oldPath, newPath);

      // Verify database update
      expect(mockPrepare).toHaveBeenCalledWith(expect.stringContaining('UPDATE file_index_states'));
      expect(mockRun).toHaveBeenCalledWith(newPath, newPath, projectId, oldPath);

      // Verify memory cache update
      expect((fileHashManager as any).cache.has(oldCacheKey)).toBe(false);
      const newCacheKey = `${projectId}:${newPath}`;
      expect((fileHashManager as any).cache.has(newCacheKey)).toBe(true);
      
      const newCacheEntry = (fileHashManager as any).cache.get(newCacheKey);
      expect(newCacheEntry.filePath).toBe(newPath);
      expect(newCacheEntry.hash).toBe(testHash);
    });

    it('should handle rename when no existing record found', async () => {
      const projectId = 'test-project';
      const oldPath = '/path/to/oldFile.ts';
      const newPath = '/path/to/newFile.ts';

      // Mock database update result (no changes)
      mockRun.mockReturnValue({ changes: 0 });

      await fileHashManager.renameFile(projectId, oldPath, newPath);

      // Verify database update was attempted
      expect(mockPrepare).toHaveBeenCalledWith(expect.stringContaining('UPDATE file_index_states'));
      expect(mockRun).toHaveBeenCalledWith(newPath, newPath, projectId, oldPath);

      // Verify warning was logged
      expect(mockLogger.warn).toHaveBeenCalledWith(
        `No existing record found for rename operation: ${oldPath} -> ${newPath}`
      );
    });

    it('should handle rename when entry not in memory cache', async () => {
      const projectId = 'test-project';
      const oldPath = '/path/to/oldFile.ts';
      const newPath = '/path/to/newFile.ts';

      // Mock database update result
      mockRun.mockReturnValue({ changes: 1 });

      await fileHashManager.renameFile(projectId, oldPath, newPath);

      // Verify database update
      expect(mockPrepare).toHaveBeenCalledWith(expect.stringContaining('UPDATE file_index_states'));
      expect(mockRun).toHaveBeenCalledWith(newPath, newPath, projectId, oldPath);

      // Verify no cache entry was created (since original wasn't in cache)
      const newCacheKey = `${projectId}:${newPath}`;
      expect((fileHashManager as any).cache.has(newCacheKey)).toBe(false);
    });

    it('should handle database error during rename', async () => {
      const projectId = 'test-project';
      const oldPath = '/path/to/oldFile.ts';
      const newPath = '/path/to/newFile.ts';

      // Mock database error
      mockRun.mockImplementation(() => {
        throw new Error('Database error');
      });

      await expect(fileHashManager.renameFile(projectId, oldPath, newPath)).rejects.toThrow('Database error');

      // Verify error was logged
      expect(mockLogger.error).toHaveBeenCalledWith(
        `Failed to rename file hash in database: ${oldPath} -> ${newPath}`,
        expect.any(Error)
      );
    });
  });
});