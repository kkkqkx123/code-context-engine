import { FileWatcherService, FileWatcherOptions, FileWatcherCallbacks } from '../FileWatcherService';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { FileSystemTraversal, FileInfo } from '../FileSystemTraversal';
import { GitignoreParser } from '../../ignore/GitignoreParser';
import { HotReloadRecoveryService } from '../HotReloadRecoveryService';
import { TYPES } from '../../../types';
import * as fs from 'fs/promises';
import * as path from 'path';
import { Container } from 'inversify';
import chokidar from 'chokidar';

// Mock modules
jest.mock('fs/promises');
jest.mock('chokidar');
jest.mock('../../../utils/LoggerService');
jest.mock('../../../utils/ErrorHandlerService');
jest.mock('../FileSystemTraversal');
jest.mock('../../ignore/GitignoreParser');
jest.mock('../HotReloadRecoveryService');

const mockedFs = fs as jest.Mocked<typeof fs>;
const mockedChokidar = chokidar as jest.Mocked<typeof chokidar>;
const MockedLoggerService = LoggerService as jest.Mocked<typeof LoggerService>;
const MockedErrorHandlerService = ErrorHandlerService as jest.Mocked<typeof ErrorHandlerService>;
const MockedFileSystemTraversal = FileSystemTraversal as jest.Mocked<typeof FileSystemTraversal>;
const mockedGitignoreParser = GitignoreParser as jest.Mocked<typeof GitignoreParser>;
const MockedHotReloadRecoveryService = HotReloadRecoveryService as jest.Mocked<typeof HotReloadRecoveryService>;

describe('FileWatcherService', () => {
  let fileWatcherService: FileWatcherService;
  let mockLogger: LoggerService;
  let mockErrorHandler: ErrorHandlerService;
  let mockFileSystemTraversal: FileSystemTraversal;
  let mockHotReloadRecoveryService: HotReloadRecoveryService;
  let container: Container;

   beforeEach(() => {
     jest.clearAllMocks();
 
     // Mock GitignoreParser methods
     mockedGitignoreParser.getAllGitignorePatterns.mockResolvedValue([]);
     mockedGitignoreParser.parseIndexignore.mockResolvedValue([]);
 
     // Create a container for dependency injection
     container = new Container();
 
     // Create mock services with proper mocks
     mockLogger = {
       info: jest.fn(),
       error: jest.fn(),
       warn: jest.fn(),
       debug: jest.fn(),
       getLogFilePath: jest.fn(),
       markAsNormalExit: jest.fn(),
     } as any;
 
     mockErrorHandler = {
       handleError: jest.fn(),
       getErrorReport: jest.fn(),
       getAllErrorReports: jest.fn(),
       clearErrorReport: jest.fn(),
       clearAllErrorReports: jest.fn(),
       getErrorStats: jest.fn(),
     } as any;
 
     mockFileSystemTraversal = {
       traverseDirectory: jest.fn(),
       getFileContent: jest.fn(),
       getDirectoryStats: jest.fn(),
       isBinaryFile: jest.fn(),
       calculateFileHash: jest.fn(),
     } as any;
 
     mockHotReloadRecoveryService = {
       handleError: jest.fn(),
       getRecoveryStrategy: jest.fn(),
     } as any;
 
     // Bind services to container
     container.bind<LoggerService>(TYPES.LoggerService).toConstantValue(mockLogger);
     container.bind<ErrorHandlerService>(TYPES.ErrorHandlerService).toConstantValue(mockErrorHandler);
     container.bind<FileSystemTraversal>(TYPES.FileSystemTraversal).toConstantValue(mockFileSystemTraversal);
     container.bind<HotReloadRecoveryService>(TYPES.HotReloadRecoveryService).toConstantValue(mockHotReloadRecoveryService);
     container.bind<FileWatcherService>(TYPES.FileWatcherService).to(FileWatcherService);
 
     // Create FileWatcherService instance
     fileWatcherService = container.get<FileWatcherService>(TYPES.FileWatcherService);
   });

  describe('startWatching', () => {
    it('should start watching specified paths', async () => {
      const watchPaths = ['/test/path1', '/test/path2'];
      const options: FileWatcherOptions = {
        watchPaths,
        ignored: ['**/node_modules/**'],
        ignoreInitial: true,
      };

      // Mock fs.access to resolve (paths exist)
      mockedFs.access.mockResolvedValue(undefined);

      // Mock chokidar.watch
      const mockWatcher = {
        on: jest.fn().mockReturnThis(),
        close: jest.fn().mockResolvedValue(undefined),
      } as any;
      mockedChokidar.watch.mockReturnValue(mockWatcher);

      await fileWatcherService.startWatching(options);

      expect(mockedChokidar.watch).toHaveBeenCalledTimes(2);
      expect(mockWatcher.on).toHaveBeenCalledWith('ready', expect.any(Function));
      expect(mockWatcher.on).toHaveBeenCalledWith('add', expect.any(Function));
      expect(mockWatcher.on).toHaveBeenCalledWith('change', expect.any(Function));
      expect(mockWatcher.on).toHaveBeenCalledWith('unlink', expect.any(Function));
    });

    it('should handle paths that do not exist', async () => {
      const watchPaths = ['/nonexistent/path'];
      const options: FileWatcherOptions = {
        watchPaths,
        ignored: ['**/node_modules/**'],
        ignoreInitial: true,
      };

      // Mock fs.access to reject (path does not exist)
      mockedFs.access.mockRejectedValue(new Error('ENOENT: no such file or directory'));

      // Mock chokidar.watch
      const mockWatcher = {
        on: jest.fn().mockReturnThis(),
        close: jest.fn().mockResolvedValue(undefined),
      } as any;
      mockedChokidar.watch.mockReturnValue(mockWatcher);

      await fileWatcherService.startWatching(options);

      // Should not throw an error, but should log a warning
      expect(mockLogger.warn).toHaveBeenCalledWith('Watch path does not exist: /nonexistent/path');
    });

    it('should not start watching if already watching', async () => {
      const watchPaths = ['/test/path'];
      const options: FileWatcherOptions = {
        watchPaths,
        ignored: ['**/node_modules/**'],
        ignoreInitial: true,
      };

      // Set isWatching to true
      (fileWatcherService as any).isWatching = true;

      await fileWatcherService.startWatching(options);

      expect(mockLogger.warn).toHaveBeenCalledWith('FileWatcherService is already watching');
      expect(mockedChokidar.watch).not.toHaveBeenCalled();
    });
  });

  describe('stopWatching', () => {
    it('should stop watching all paths', async () => {
      // Set up initial state
      const mockWatcher1 = {
        close: jest.fn().mockResolvedValue(undefined),
      } as any;
      const mockWatcher2 = {
        close: jest.fn().mockResolvedValue(undefined),
      } as any;

      (fileWatcherService as any).watchers.set('/path1', mockWatcher1);
      (fileWatcherService as any).watchers.set('/path2', mockWatcher2);
      (fileWatcherService as any).isWatching = true;

      await fileWatcherService.stopWatching();

      expect(mockWatcher1.close).toHaveBeenCalled();
      expect(mockWatcher2.close).toHaveBeenCalled();
      expect((fileWatcherService as any).isWatching).toBe(false);
      expect((fileWatcherService as any).watchers.size).toBe(0);
    });

    it('should warn if not watching when stop is called', async () => {
      await fileWatcherService.stopWatching();

      expect(mockLogger.warn).toHaveBeenCalledWith('FileWatcherService is not watching');
    });
  });

  describe('file event handling', () => {
    let mockFileInfo: FileInfo;
    let callbacks: FileWatcherCallbacks;

    beforeEach(() => {
      mockFileInfo = {
        path: '/test/file.ts',
        relativePath: 'file.ts',
        name: 'file.ts',
        extension: '.ts',
        size: 1024,
        hash: 'testhash',
        lastModified: new Date(),
        language: 'typescript',
        isBinary: false,
      };

      callbacks = {
        onFileAdded: jest.fn(),
        onFileChanged: jest.fn(),
        onFileDeleted: jest.fn(),
        onFileRenamed: jest.fn(),
      };

      fileWatcherService.setCallbacks(callbacks);
    });

    it('should handle file added event', async () => {
      // Mock the getFileInfo method to return our mock file info
      (fileWatcherService as any).getFileInfo = jest.fn().mockResolvedValue(mockFileInfo);

      // Simulate file add event
      await (fileWatcherService as any).handleFileAdd('/test/file.ts', { size: 1024 }, '/test');

      expect(callbacks.onFileAdded).toHaveBeenCalledWith(mockFileInfo);
    });

    it('should handle file changed event', async () => {
      // Mock the getFileInfo method to return our mock file info
      (fileWatcherService as any).getFileInfo = jest.fn().mockResolvedValue(mockFileInfo);

      // Simulate file change event
      await (fileWatcherService as any).handleFileChange('/test/file.ts', { size: 1024 }, '/test');

      expect(callbacks.onFileChanged).toHaveBeenCalledWith(mockFileInfo);
    });

    it('should handle file deleted event', async () => {
      // Mock fs.stat to throw error (file doesn't exist)
      mockedFs.stat.mockRejectedValue(new Error('ENOENT: no such file or directory'));

      // Simulate file delete event
      await (fileWatcherService as any).handleFileDelete('/test/file.ts', '/test');

      expect(callbacks.onFileDeleted).toHaveBeenCalledWith('/test/file.ts');
    });

    it('should detect file rename operation', async () => {
      const oldPath = '/test/oldFile.ts';
      const newPath = '/test/newFile.ts';
      const fileHash = 'testhash123';
      const fileSize = 1024;

      // Mock fs.stat for the old file (during delete)
      mockedFs.stat.mockResolvedValue({
        size: fileSize,
        mtime: new Date(),
      } as any);

      // Mock calculateFileHash for the old file
      (mockFileSystemTraversal as any).calculateFileHash = jest.fn().mockResolvedValue(fileHash);

      // Mock getFileInfo for the new file
      const newFileInfo: FileInfo = {
        path: path.resolve(newPath),
        relativePath: 'newFile.ts',
        name: 'newFile.ts',
        extension: '.ts',
        size: fileSize,
        hash: fileHash,
        lastModified: new Date(),
        language: 'typescript',
        isBinary: false,
      };
      (fileWatcherService as any).getFileInfo = jest.fn().mockResolvedValue(newFileInfo);

      // Simulate file delete event (first part of rename)
      await (fileWatcherService as any).handleFileDelete(oldPath, '/test');

      // Simulate file add event (second part of rename)
      await (fileWatcherService as any).handleFileAdd(newPath, { size: fileSize }, '/test');

      // Wait for rename timeout
      await new Promise(resolve => setTimeout(resolve, 100));

      // Verify rename was detected
      expect(callbacks.onFileRenamed).toHaveBeenCalledWith(oldPath, newPath, newFileInfo);
      expect(callbacks.onFileDeleted).not.toHaveBeenCalled();
      expect(callbacks.onFileAdded).not.toHaveBeenCalled();
    });

    it('should handle separate delete and add operations (not rename)', async () => {
      const oldPath = '/test/oldFile.ts';
      const newPath = '/test/newFile.ts';
      const oldFileHash = 'oldhash123';
      const newFileHash = 'newhash456';
      const fileSize = 1024;

      // Mock fs.stat for the old file (during delete)
      mockedFs.stat.mockResolvedValue({
        size: fileSize,
        mtime: new Date(),
      } as any);

      // Mock calculateFileHash for the old file
      (mockFileSystemTraversal as any).calculateFileHash = jest.fn().mockResolvedValue(oldFileHash);

      // Mock getFileInfo for the new file
      const newFileInfo: FileInfo = {
        path: path.resolve(newPath),
        relativePath: 'newFile.ts',
        name: 'newFile.ts',
        extension: '.ts',
        size: fileSize,
        hash: newFileHash,
        lastModified: new Date(),
        language: 'typescript',
        isBinary: false,
      };
      (fileWatcherService as any).getFileInfo = jest.fn().mockResolvedValue(newFileInfo);

      // Simulate file delete event
      await (fileWatcherService as any).handleFileDelete(oldPath, '/test');

      // Wait for rename timeout to expire
      await new Promise(resolve => setTimeout(resolve, 1100));

      // Simulate file add event (after timeout, so not a rename)
      await (fileWatcherService as any).handleFileAdd(newPath, { size: fileSize }, '/test');

      // Verify separate operations were handled
      expect(callbacks.onFileDeleted).toHaveBeenCalledWith(oldPath);
      expect(callbacks.onFileAdded).toHaveBeenCalledWith(newFileInfo);
      expect(callbacks.onFileRenamed).not.toHaveBeenCalled();
    });
  });

  describe('getFileInfo', () => {
    it('should return file info for a valid file', async () => {
      const filePath = '/test/file.ts';
      const watchPath = '/test';
      const mockStats = {
        size: 1024,
        mtime: new Date(),
      };

      mockedFs.stat.mockResolvedValue(mockStats as any);
      (mockFileSystemTraversal as any).isBinaryFile = jest.fn().mockResolvedValue(false);
      (mockFileSystemTraversal as any).calculateFileHash = jest.fn().mockResolvedValue('testhash');
      
      // Mock the shouldIgnoreFile method to return false
      (fileWatcherService as any).shouldIgnoreFile = jest.fn().mockReturnValue(false);

      const fileInfo = await (fileWatcherService as any).getFileInfo(filePath, watchPath);

      expect(fileInfo).toEqual({
        path: path.resolve(filePath),
        relativePath: 'file.ts',
        name: 'file.ts',
        extension: '.ts',
        size: 1024,
        hash: 'testhash',
        lastModified: mockStats.mtime,
        language: 'typescript',
        isBinary: false,
      });
    });

    it('should return null for a file outside the watch path', async () => {
      const filePath = '/other/file.ts';
      const watchPath = '/test';

      const fileInfo = await (fileWatcherService as any).getFileInfo(filePath, watchPath);

      expect(fileInfo).toBeNull();
      expect(mockLogger.warn).toHaveBeenCalledWith('File path is outside watch path: /other/file.ts');
    });

    it('should return null for a file that should be ignored', async () => {
      const filePath = '/test/.gitignore';
      const watchPath = '/test';
      const mockStats = {
        size: 1024,
        mtime: new Date(),
      };

      mockedFs.stat.mockResolvedValue(mockStats as any);

      const fileInfo = await (fileWatcherService as any).getFileInfo(filePath, watchPath);

      expect(fileInfo).toBeNull();
    });
  });
});