import { ChangeDetectionService, ChangeDetectionOptions, ChangeDetectionCallbacks, FileChangeEvent } from '../ChangeDetectionService';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { FileWatcherService, FileWatcherOptions, FileWatcherCallbacks } from '../FileWatcherService';
import { FileSystemTraversal, FileInfo, TraversalResult } from '../FileSystemTraversal';
import { HotReloadRecoveryService } from '../HotReloadRecoveryService';
import { FileHashManager } from '../FileHashManager';
import { ProjectIdManager } from '../../../database/ProjectIdManager';
import { FileUtils } from '../../../utils/filesystem/FileUtils';
import { TYPES } from '../../../types';
import { Container } from 'inversify';
import * as path from 'path';

// Mock modules
jest.mock('../../../utils/LoggerService');
jest.mock('../../../utils/ErrorHandlerService');
jest.mock('../FileWatcherService');
jest.mock('../FileSystemTraversal');
jest.mock('../HotReloadRecoveryService');
jest.mock('../FileHashManager');
jest.mock('../../../database/ProjectIdManager');
jest.mock('../../../utils/filesystem/FileUtils');

const MockedLoggerService = LoggerService as jest.Mocked<typeof LoggerService>;
const MockedErrorHandlerService = ErrorHandlerService as jest.Mocked<typeof ErrorHandlerService>;
const MockedFileWatcherService = FileWatcherService as jest.Mocked<typeof FileWatcherService>;
const MockedFileSystemTraversal = FileSystemTraversal as jest.Mocked<typeof FileSystemTraversal>;
const MockedHotReloadRecoveryService = HotReloadRecoveryService as jest.Mocked<typeof HotReloadRecoveryService>;
const MockedProjectIdManager = ProjectIdManager as jest.Mocked<typeof ProjectIdManager>;
const MockedFileUtils = FileUtils as jest.Mocked<typeof FileUtils>;

describe('ChangeDetectionService', () => {
    let changeDetectionService: ChangeDetectionService;
    let mockLogger: LoggerService;
    let mockErrorHandler: ErrorHandlerService;
    let mockFileWatcherService: FileWatcherService;
    let mockFileSystemTraversal: FileSystemTraversal;
    let mockHotReloadRecoveryService: HotReloadRecoveryService;
    let mockFileHashManager: FileHashManager;
    let mockProjectIdManager: ProjectIdManager;
    let container: Container;

    beforeEach(() => {
        jest.clearAllMocks();
        jest.useFakeTimers();

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

        mockFileWatcherService = {
            startWatching: jest.fn(),
            stopWatching: jest.fn(),
            setCallbacks: jest.fn(),
            isWatchingPath: jest.fn(),
            getWatchedPaths: jest.fn(),
            isTestMode: jest.fn(),
            waitForEvents: jest.fn(),
            flushEventQueue: jest.fn(),
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

        mockFileHashManager = {
            getFileHash: jest.fn(),
            updateFileHash: jest.fn(),
            getFileHashes: jest.fn(),
            batchUpdateHashes: jest.fn(),
            deleteFileHash: jest.fn(),
            renameFile: jest.fn(),
            getChangedFiles: jest.fn(),
            cleanupExpiredHashes: jest.fn(),
        } as any;

        mockProjectIdManager = {
            getProjectId: jest.fn(),
            generateProjectId: jest.fn(),
            setProjectId: jest.fn(),
            deleteProjectId: jest.fn(),
            getAllProjectMappings: jest.fn(),
            clearAllMappings: jest.fn(),
        } as any;

        // Bind services to container
        container.bind<LoggerService>(TYPES.LoggerService).toConstantValue(mockLogger);
        container.bind<ErrorHandlerService>(TYPES.ErrorHandlerService).toConstantValue(mockErrorHandler);
        container.bind<FileWatcherService>(TYPES.FileWatcherService).toConstantValue(mockFileWatcherService);
        container.bind<FileSystemTraversal>(TYPES.FileSystemTraversal).toConstantValue(mockFileSystemTraversal);
        container.bind<HotReloadRecoveryService>(TYPES.HotReloadRecoveryService).toConstantValue(mockHotReloadRecoveryService);
        container.bind<FileHashManager>(TYPES.FileHashManager).toConstantValue(mockFileHashManager);
        container.bind<ProjectIdManager>(TYPES.ProjectIdManager).toConstantValue(mockProjectIdManager);
        container.bind<ChangeDetectionService>(TYPES.ChangeDetectionService).to(ChangeDetectionService);

        // Create ChangeDetectionService instance
        changeDetectionService = container.get<ChangeDetectionService>(TYPES.ChangeDetectionService);
    });

    afterEach(() => {
        jest.useRealTimers();
        // Clean up any pending changes to prevent test timeouts
        if (changeDetectionService) {
            for (const timeoutId of (changeDetectionService as any).pendingChanges.values()) {
                clearTimeout(timeoutId);
            }
            (changeDetectionService as any).pendingChanges.clear();
        }
    });

    describe('initialize', () => {
        it('should initialize with root paths and set up file watcher', async () => {
            // Use fake timers to prevent waitForProjectIdManager from waiting for real time
            jest.useFakeTimers();

            const rootPaths = ['/test/path1', '/test/path2'];
            const mockTraversalResult: TraversalResult = {
                files: [
                    {
                        path: '/test/path1/file1.ts',
                        relativePath: 'file1.ts',
                        name: 'file1.ts',
                        extension: '.ts',
                        size: 1024,
                        hash: 'hash1',
                        lastModified: new Date(),
                        language: 'typescript',
                        isBinary: false,
                    },
                ],
                directories: [],
                errors: [],
                totalSize: 1024,
                processingTime: 100,
            };

            (mockFileSystemTraversal.traverseDirectory as jest.Mock).mockResolvedValue(mockTraversalResult);
            mockFileWatcherService.setCallbacks = jest.fn();
            mockFileWatcherService.startWatching = jest.fn().mockResolvedValue(undefined);
            (mockFileHashManager.batchUpdateHashes as jest.Mock).mockResolvedValue(undefined);
            (mockProjectIdManager.getProjectId as jest.Mock).mockReturnValue('test-project-id');

            // Fast forward timers to skip the waitForProjectIdManager delay
            const initPromise = changeDetectionService.initialize(rootPaths);
            jest.advanceTimersByTime(1100); // Advance past the 1000ms wait
            await initPromise;

            expect(mockFileSystemTraversal.traverseDirectory).toHaveBeenCalledTimes(2);
            expect(mockFileHashManager.batchUpdateHashes).toHaveBeenCalled();
            expect(mockFileWatcherService.setCallbacks).toHaveBeenCalledWith(
                expect.objectContaining({
                    onFileAdded: expect.any(Function),
                    onFileChanged: expect.any(Function),
                    onFileDeleted: expect.any(Function),
                    onFileRenamed: expect.any(Function),
                    onError: expect.any(Function),
                    onReady: expect.any(Function),
                })
            );
            expect(mockFileWatcherService.startWatching).toHaveBeenCalledWith(
                expect.objectContaining({
                    watchPaths: rootPaths,
                    ignored: ['**/node_modules/**', '**/.git/**', '**/dist/**', '**/build/**'],
                    ignoreInitial: true,
                    awaitWriteFinish: true,
                })
            );
            expect((changeDetectionService as any).isRunning).toBe(true);
        });

        it('should not initialize if already running', async () => {
            (changeDetectionService as any).isRunning = true;

            await changeDetectionService.initialize(['/test/path']);

            expect(mockLogger.warn).toHaveBeenCalledWith('ChangeDetectionService is already initialized');
            expect(mockFileSystemTraversal.traverseDirectory).not.toHaveBeenCalled();
        });
    });

    describe('stop', () => {
        it('should stop the service and clear pending changes', async () => {
            // Set up initial state
            (changeDetectionService as any).isRunning = true;
            (changeDetectionService as any).pendingChanges.set('file.ts', setTimeout(() => { }, 100));
            mockFileWatcherService.stopWatching = jest.fn().mockResolvedValue(undefined);

            await changeDetectionService.stop();

            expect(mockFileWatcherService.stopWatching).toHaveBeenCalled();
            expect((changeDetectionService as any).pendingChanges.size).toBe(0);
            expect((changeDetectionService as any).isRunning).toBe(false);
        });

        it('should warn if not running when stop is called', async () => {
            await changeDetectionService.stop();

            expect(mockLogger.warn).toHaveBeenCalledWith('ChangeDetectionService is not running');
        });
    });

    describe('file event handling', () => {
        let mockFileInfo: FileInfo;
        let callbacks: ChangeDetectionCallbacks;

        beforeEach(() => {
            mockFileInfo = {
                path: '/test/file.ts',
                relativePath: 'file.ts',
                name: 'file.ts',
                extension: '.ts',
                size: 1024,
                hash: 'hash1',
                lastModified: new Date(),
                language: 'typescript',
                isBinary: false,
            };

            callbacks = {
                onFileCreated: jest.fn(),
                onFileModified: jest.fn(),
                onFileDeleted: jest.fn(),
            };

            changeDetectionService.setCallbacks(callbacks);
        });

        it('should handle file added event', async () => {
            // Mock getProjectIdForPath to return a test project ID
            const mockProjectId = 'test-project-id';
            (mockProjectIdManager.getProjectId as jest.Mock).mockReturnValue(mockProjectId);
            
            (mockFileHashManager.getFileHash as jest.Mock).mockResolvedValue(null);
            (mockFileHashManager.updateFileHash as jest.Mock).mockResolvedValue(undefined);

            await (changeDetectionService as any).handleFileAdded(mockFileInfo);

            const expectedEvent: FileChangeEvent = {
                type: 'created',
                path: mockFileInfo.path,
                relativePath: mockFileInfo.relativePath,
                currentHash: mockFileInfo.hash,
                timestamp: expect.any(Date),
                size: mockFileInfo.size,
                language: mockFileInfo.language,
            };

            expect(callbacks.onFileCreated).toHaveBeenCalledWith(expectedEvent);
            expect(mockFileHashManager.getFileHash).toHaveBeenCalledWith('test-project-id', mockFileInfo.relativePath);
            expect(mockFileHashManager.updateFileHash).toHaveBeenCalledWith('test-project-id', mockFileInfo.relativePath, mockFileInfo.hash, expect.any(Object));
        });

        it('should handle file changed event with debounce', async () => {
            // Mock getProjectIdForPath to return a test project ID
            const mockProjectId = 'test-project-id';
            (mockProjectIdManager.getProjectId as jest.Mock).mockReturnValue(mockProjectId);
            
            // Set up initial hash
            (mockFileHashManager.getFileHash as jest.Mock).mockResolvedValue('oldhash');
            MockedFileUtils.calculateFileHash = jest.fn().mockResolvedValue('newhash');
            (mockFileHashManager.updateFileHash as jest.Mock).mockResolvedValue(undefined);

            // Use real timers for this test since debounce uses real timing
            jest.useRealTimers();

            try {
                await (changeDetectionService as any).handleFileChanged(mockFileInfo);

                // Should not trigger callback immediately due to debounce
                expect(callbacks.onFileModified).not.toHaveBeenCalled();

                // Wait for debounce to complete (test mode uses 100ms debounce)
                await new Promise(resolve => setTimeout(resolve, 150));

                const expectedEvent: FileChangeEvent = {
                    type: 'modified',
                    path: mockFileInfo.path,
                    relativePath: mockFileInfo.relativePath,
                    previousHash: 'oldhash',
                    currentHash: 'newhash',
                    timestamp: expect.any(Date),
                    size: mockFileInfo.size,
                    language: mockFileInfo.language,
                };

                expect(callbacks.onFileModified).toHaveBeenCalledWith(expectedEvent);
                expect(mockFileHashManager.updateFileHash).toHaveBeenCalledWith('test-project-id', mockFileInfo.relativePath, 'newhash', expect.any(Object));
            } finally {
                // Restore fake timers for other tests
                jest.useFakeTimers();
            }
        });

        it('should handle file deleted event', async () => {
            // Mock getProjectIdForPath to return a test project ID
            const mockProjectId = 'test-project-id';
            (mockProjectIdManager.getProjectId as jest.Mock).mockReturnValue(mockProjectId);
            
            (mockFileHashManager.getFileHash as jest.Mock).mockResolvedValue('hash1');
            (mockFileHashManager.deleteFileHash as jest.Mock).mockResolvedValue(undefined);

            await (changeDetectionService as any).handleFileDeleted(mockFileInfo.path);

            expect(mockFileHashManager.getFileHash).toHaveBeenCalled();
            expect(mockFileHashManager.deleteFileHash).toHaveBeenCalledWith('test-project-id', expect.any(String));
        });
    });

    describe('utility methods', () => {
        beforeEach(() => {
            // Set up some initial data
            // Note: fileHashes is now managed by FileHashManager, not internal Map
            (changeDetectionService as any).fileHistory.set('file1.ts', [
                {
                    path: '/test/file1.ts',
                    hash: 'oldhash1',
                    timestamp: new Date(Date.now() - 1000),
                    size: 512,
                    language: 'typescript',
                },
            ]);
            (changeDetectionService as any).isRunning = true;
        });

        it('should get file hash', async () => {
            (mockFileHashManager.getFileHash as jest.Mock).mockResolvedValue('hash1');
            const hash = await changeDetectionService.getFileHash('file1.ts');
            expect(hash).toBe('hash1');
        });

        it('should return undefined for non-existent file hash', async () => {
            (mockFileHashManager.getFileHash as jest.Mock).mockResolvedValue(null);
            const hash = await changeDetectionService.getFileHash('nonexistent.ts');
            expect(hash).toBeUndefined();
        });

        it('should get file history', () => {
            const history = changeDetectionService.getFileHistory('file1.ts');
            expect(history).toHaveLength(1);
            expect(history[0].hash).toBe('oldhash1');
        });

        it('should return empty array for non-existent file history', () => {
            const history = changeDetectionService.getFileHistory('nonexistent.ts');
            expect(history).toEqual([]);
        });

        it('should get all file hashes', () => {
            const allHashes = changeDetectionService.getAllFileHashes();
            expect(allHashes.size).toBe(0); // Changed implementation
        });

        it('should check if file is tracked', async () => {
            (mockFileHashManager.getFileHash as jest.Mock).mockResolvedValue('hash1');
            const isTracked = await changeDetectionService.isFileTracked('file1.ts');
            expect(isTracked).toBe(true);
        });

        it('should check if file is not tracked', async () => {
            (mockFileHashManager.getFileHash as jest.Mock).mockResolvedValue(null);
            const isTracked = await changeDetectionService.isFileTracked('nonexistent.ts');
            expect(isTracked).toBe(false);
        });

        it('should get tracked files count', () => {
            const count = changeDetectionService.getTrackedFilesCount();
            expect(count).toBe(0); // Changed implementation
        });

        it('should check if service is running', () => {
            expect(changeDetectionService.isServiceRunning()).toBe(true);
        });
    });

    describe('test mode helpers', () => {
        beforeEach(() => {
            // Set test mode
            (changeDetectionService as any).testMode = true;
        });

        it('should detect test mode', () => {
            expect(changeDetectionService.isTestMode()).toBe(true);
        });

        it('should wait for file processing', async () => {
            // Use real timers for this test since waitForFileProcessing uses real timing
            jest.useRealTimers();

            try {
                // Test with no pending changes - should return true immediately
                let result = await changeDetectionService.waitForFileProcessing('file.ts', 100);
                expect(result).toBe(true);

                // Add a pending change using a long timeout that won't expire during test
                const timeout = setTimeout(() => { }, 10000);
                (changeDetectionService as any).pendingChanges.set('file.ts', timeout);

                // Test with pending changes - should return false due to timeout
                result = await changeDetectionService.waitForFileProcessing('file.ts', 50);
                expect(result).toBe(false);

                // Clean up
                clearTimeout(timeout);
                (changeDetectionService as any).pendingChanges.clear();
            } finally {
                // Restore fake timers for other tests
                jest.useFakeTimers();
            }
        });

        it('should wait for all processing', async () => {
            // Use real timers for this test
            jest.useRealTimers();

            try {
                // Test with no pending changes - should return true immediately
                let result = await changeDetectionService.waitForAllProcessing(100);
                expect(result).toBe(true);

                // Add pending changes
                const timeout1 = setTimeout(() => { }, 10000);
                const timeout2 = setTimeout(() => { }, 10000);
                (changeDetectionService as any).pendingChanges.set('file1.ts', timeout1);
                (changeDetectionService as any).pendingChanges.set('file2.ts', timeout2);

                // Test with pending changes - should return false due to timeout
                result = await changeDetectionService.waitForAllProcessing(50);
                expect(result).toBe(false);

                // Clean up
                clearTimeout(timeout1);
                clearTimeout(timeout2);
                (changeDetectionService as any).pendingChanges.clear();
            } finally {
                // Restore fake timers
                jest.useFakeTimers();
            }
        });

        it('should flush pending changes', async () => {
            // Create a simple mock for waitForAllProcessing
            const mockWaitForAllProcessing = jest.fn().mockResolvedValue(true);

            // Replace the method temporarily
            const originalMethod = (changeDetectionService as any).waitForAllProcessing;
            (changeDetectionService as any).waitForAllProcessing = mockWaitForAllProcessing;

            // Use real timers for this test since flushPendingChanges uses setTimeout
            jest.useRealTimers();

            try {
                await changeDetectionService.flushPendingChanges();

                expect(mockWaitForAllProcessing).toHaveBeenCalledWith(); // Called with default parameters
            } finally {
                // Restore original method and fake timers
                (changeDetectionService as any).waitForAllProcessing = originalMethod;
                jest.useFakeTimers();
            }
        });
    });

    describe('getProjectIdForPath', () => {
        it('should get project ID for path', async () => {
            const mockProjectId = 'test-project-id';
            (mockProjectIdManager.getProjectId as jest.Mock).mockReturnValue(mockProjectId);

            const projectId = await changeDetectionService.getProjectIdForPath('/test/path');

            expect(projectId).toBe(mockProjectId);
            expect(mockProjectIdManager.getProjectId).toHaveBeenCalledWith('/test/path');
        });

        it('should get project ID for path with error handling', async () => {
            (mockProjectIdManager.getProjectId as jest.Mock).mockImplementation(() => {
                throw new Error('Project ID not found');
            });

            await expect(changeDetectionService.getProjectIdForPath('/test/path')).rejects.toThrow('Project ID not found');
        });
    });

    describe('file rename handling', () => {
        let mockFileInfo: FileInfo;
        let callbacks: ChangeDetectionCallbacks;

        beforeEach(() => {
            mockFileInfo = {
                path: '/test/newFile.ts',
                relativePath: 'newFile.ts',
                name: 'newFile.ts',
                extension: '.ts',
                size: 1024,
                hash: 'testhash123',
                lastModified: new Date(),
                language: 'typescript',
                isBinary: false,
            };

            callbacks = {
                onFileCreated: jest.fn(),
                onFileModified: jest.fn(),
                onFileDeleted: jest.fn(),
                onFileRenamed: jest.fn(),
                onError: jest.fn(),
            };

            changeDetectionService.setCallbacks(callbacks);
        });

        it('should handle file renamed event', async () => {
            const oldPath = '/test/oldFile.ts';
            const newPath = '/test/newFile.ts';
            const mockProjectId = 'test-project-id';
            const mockPreviousHash = 'oldhash123';

            // Mock project ID and file hash
            (mockProjectIdManager.getProjectId as jest.Mock).mockReturnValue(mockProjectId);
            (mockFileHashManager.getFileHash as jest.Mock).mockResolvedValue(mockPreviousHash);
            (mockFileHashManager.renameFile as jest.Mock).mockResolvedValue(undefined);

            // Simulate file rename event
            await (changeDetectionService as any).handleFileRenamed(oldPath, newPath, mockFileInfo);

            // Verify rename was handled
            expect(mockFileHashManager.renameFile).toHaveBeenCalledWith(
                mockProjectId,
                expect.stringContaining('oldFile.ts'),
                expect.stringContaining('newFile.ts')
            );
            expect(callbacks.onFileRenamed).toHaveBeenCalledWith(
                expect.objectContaining({
                    type: 'renamed',
                    path: newPath,
                    relativePath: 'newFile.ts',
                    oldPath: expect.stringContaining('oldFile.ts'),
                    oldRelativePath: expect.stringContaining('oldFile.ts'),
                    previousHash: mockPreviousHash,
                    currentHash: mockFileInfo.hash,
                    timestamp: expect.any(Date),
                    size: mockFileInfo.size,
                    language: mockFileInfo.language,
                })
            );
        });

        it('should handle rename when old file is not tracked', async () => {
            const oldPath = '/test/oldFile.ts';
            const newPath = '/test/newFile.ts';
            const mockProjectId = 'test-project-id';

            // Mock project ID and file hash (not found)
            (mockProjectIdManager.getProjectId as jest.Mock).mockReturnValue(mockProjectId);
            (mockFileHashManager.getFileHash as jest.Mock).mockResolvedValue(null);
            (mockFileHashManager.updateFileHash as jest.Mock).mockResolvedValue(undefined);

            // Simulate file rename event
            await (changeDetectionService as any).handleFileRenamed(oldPath, newPath, mockFileInfo);

            // Verify rename was not handled, but file was added
            expect(mockFileHashManager.renameFile).not.toHaveBeenCalled();
            expect(mockFileHashManager.updateFileHash).toHaveBeenCalled();
            expect(callbacks.onFileRenamed).not.toHaveBeenCalled();
            expect(callbacks.onFileCreated).toHaveBeenCalled();
        });

        it('should handle rename event errors', async () => {
            const oldPath = '/test/oldFile.ts';
            const newPath = '/test/newFile.ts';
            const mockProjectId = 'test-project-id';

            // Mock project ID and file hash
            (mockProjectIdManager.getProjectId as jest.Mock).mockReturnValue(mockProjectId);
            (mockFileHashManager.getFileHash as jest.Mock).mockResolvedValue('oldhash123');
            (mockFileHashManager.renameFile as jest.Mock).mockRejectedValue(new Error('Database error'));

            // Simulate file rename event
            await (changeDetectionService as any).handleFileRenamed(oldPath, newPath, mockFileInfo);

            // Verify error was handled
            expect(mockErrorHandler.handleError).toHaveBeenCalled();
            expect(callbacks.onFileRenamed).not.toHaveBeenCalled();
        });
    });
});