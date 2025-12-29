import { FileSystemTraversal, TraversalOptions, TraversalResult, FileInfo } from '../FileSystemTraversal';
import { GitignoreParser } from '../../ignore/GitignoreParser';
import { LoggerService } from '../../../utils/LoggerService';
import * as fs from 'fs/promises';
import * as path from 'path';
import { createHash } from 'crypto';

// Mock fs module
jest.mock('fs/promises');
const mockedFs = fs as jest.Mocked<typeof fs>;

// Mock GitignoreParser
jest.mock('../../ignore/GitignoreParser');
const mockedGitignoreParser = GitignoreParser as jest.Mocked<typeof GitignoreParser>;

// Mock LoggerService
jest.mock('../../../utils/LoggerService');
const MockedLoggerService = LoggerService as jest.Mocked<typeof LoggerService>;

// Mock FileUtils
jest.mock('../../../utils/filesystem/FileUtils');
const MockedFileUtils = require('../../../utils/filesystem/FileUtils');

// Mock PatternMatcher
jest.mock('../../../utils/filesystem/PatternMatcher');
const MockedPatternMatcher = require('../../../utils/filesystem/PatternMatcher');

// Mock IgnoreRulesManager
jest.mock('../../../utils/filesystem/IgnoreRulesManager');
const MockedIgnoreRulesManager = require('../../../utils/filesystem/IgnoreRulesManager');

// Mock fsSync for realpathSync
jest.mock('fs');
const fsSync = require('fs') as jest.Mocked<typeof import('fs')>;

describe('FileSystemTraversal', () => {
    let fileSystemTraversal: FileSystemTraversal;
    let mockOptions: TraversalOptions;
    let mockLogger: LoggerService;
    let originalCreateReadStream: any;
    let mockIgnoreRulesManager: any;

    beforeEach(() => {
        jest.clearAllMocks();
        mockOptions = {
            includePatterns: ['**/*.ts', '**/*.js'],
            excludePatterns: ['**/node_modules/**', '**/.git/**'],
            maxFileSize: 512000, // 500KB
            supportedExtensions: ['.ts', '.js', '.tsx', '.jsx'],
            followSymlinks: false,
            ignoreHiddenFiles: true,
            ignoreDirectories: ['node_modules', '.git', 'dist', 'build'],
            respectGitignore: true,
        };

        // Create mock logger
        mockLogger = {
            info: jest.fn(),
            error: jest.fn(),
            warn: jest.fn(),
            debug: jest.fn(),
            getLogFilePath: jest.fn(),
            markAsNormalExit: jest.fn(),
        } as any;

        // Store original createReadStream
        originalCreateReadStream = require('fs').createReadStream;

        // Mock realpathSync to prevent circular reference issues
        (fsSync.realpathSync as unknown as jest.Mock).mockImplementation((p: string) => {
            // Return a unique path for each input to prevent circular references
            return p.replace(/\\/g, '/');
        });

        // Mock GitignoreParser.parseGitignore to return empty array by default
        mockedGitignoreParser.parseGitignore.mockResolvedValue([]);

        // Mock IgnoreRulesManager
        mockIgnoreRulesManager = {
            refreshIgnoreRules: jest.fn().mockResolvedValue(undefined),
            getIgnorePatternsForPath: jest.fn().mockReturnValue([]),
        };
        MockedIgnoreRulesManager.IgnoreRulesManager = jest.fn().mockImplementation(() => mockIgnoreRulesManager);

        // Mock FileUtils methods
        MockedFileUtils.FileUtils = {
            calculateFileHash: jest.fn().mockResolvedValue('testhash123'),
            isBinaryFile: jest.fn().mockResolvedValue(false),
            getFileContent: jest.fn().mockResolvedValue('export const test = true;'),
            getFileExtension: jest.fn().mockReturnValue('.ts'),
            getFileName: jest.fn().mockReturnValue('file1'),
            detectLanguage: jest.fn().mockReturnValue('typescript'),
        };

        // Mock PatternMatcher methods
        MockedPatternMatcher.PatternMatcher = {
            shouldIgnoreFile: jest.fn().mockReturnValue(false),
            shouldIgnoreDirectory: jest.fn().mockReturnValue(false),
            matchesPattern: jest.fn().mockReturnValue(false),
        };

        // Create a new instance for each test
        fileSystemTraversal = new FileSystemTraversal(mockLogger, mockOptions as any);
    });

    afterEach(() => {
        // Restore original createReadStream
        if (originalCreateReadStream) {
            require('fs').createReadStream = originalCreateReadStream;
        }
        jest.restoreAllMocks();
    });

    describe('traverseDirectory', () => {
        it('should successfully traverse a directory and return results', async () => {
            // Setup mock data
            const testPath = '/test/path';
            const mockStats = {
                isDirectory: () => true,
                isFile: () => false,
                size: 1024,
                mtime: new Date(),
            };

            const mockFileStats = {
                isDirectory: () => false,
                isFile: () => true,
                size: 1024,
                mtime: new Date(),
            };

            // Mock fs calls
            (mockedFs.stat as jest.Mock).mockImplementation((filePath: string) => {
                if (filePath === path.join(testPath, 'file1.ts')) {
                    return Promise.resolve(mockFileStats as any);
                }
                return Promise.resolve(mockStats as any);
            });

            // Mock readdir to return only files, no subdirectories to prevent recursion
            mockedFs.readdir.mockResolvedValue([
                { name: 'file1.ts', isDirectory: () => false, isFile: () => true },
            ] as any);

            // Mock the calculateFileHash method
            MockedFileUtils.FileUtils.calculateFileHash = jest.fn().mockResolvedValue('testhash123');

            // Mock isBinaryFile to return false
            MockedFileUtils.FileUtils.isBinaryFile = jest.fn().mockResolvedValue(false);

            // Execute the method
            const result: TraversalResult = await fileSystemTraversal.traverseDirectory(testPath);

            // Verify the result
            expect(result).toBeDefined();
            expect(result.files).toHaveLength(1);
            expect(result.directories).toHaveLength(0); // No subdirectories now
            expect(result.errors).toHaveLength(0);
            expect(result.totalSize).toBe(1024);
            expect(result.processingTime).toBeGreaterThan(0);
        });

        it('should handle errors when directory does not exist', async () => {
            const testPath = '/nonexistent/path';
            mockedFs.stat.mockRejectedValue(new Error('ENOENT: no such file or directory'));

            const result: TraversalResult = await fileSystemTraversal.traverseDirectory(testPath);

            expect(result.errors).toHaveLength(1);
            expect(result.errors[0]).toContain('Failed to traverse directory');
        });

        it('should respect gitignore patterns when respectGitignore is true', async () => {
            const testPath = '/test/path';
            const mockStats = {
                isDirectory: () => true,
                isFile: () => false,
                size: 1024,
                mtime: new Date(),
            };

            mockedFs.stat.mockResolvedValue(mockStats as any);
            mockedFs.readdir.mockResolvedValue([] as any);
            mockedGitignoreParser.getAllGitignorePatterns.mockResolvedValue(['**/*.test.ts']);

            const optionsWithGitignore = { ...mockOptions, respectGitignore: true };
            const traversalWithGitignore = new FileSystemTraversal(mockLogger, optionsWithGitignore as any);

            await traversalWithGitignore.traverseDirectory(testPath);

            expect(mockIgnoreRulesManager.refreshIgnoreRules).toHaveBeenCalledTimes(1);
        });
    });

    describe('getFileContent', () => {
        it('should read file content successfully', async () => {
            const testPath = '/test/file.ts';
            const mockContent = 'export const test = true;';
            
            MockedFileUtils.FileUtils.getFileContent = jest.fn().mockResolvedValue(mockContent);

            const content = await fileSystemTraversal.getFileContent(testPath);

            expect(content).toBe(mockContent);
            expect(MockedFileUtils.FileUtils.getFileContent).toHaveBeenCalledWith(testPath);
        });

        it('should throw error when file cannot be read', async () => {
            const testPath = '/test/nonexistent.ts';
            const error = new Error(`Failed to read file ${testPath}: ENOENT: no such file or directory`);
            
            MockedFileUtils.FileUtils.getFileContent = jest.fn().mockRejectedValue(error);

            await expect(fileSystemTraversal.getFileContent(testPath)).rejects.toThrow(
                `Failed to read file ${testPath}`
            );
        });
    });

    describe('getDirectoryStats', () => {
        it('should return correct directory statistics', async () => {
            const testPath = '/test/path';
            const mockStats = {
                isDirectory: () => true,
                isFile: () => false,
                size: 1024,
                mtime: new Date(),
            };

            const mockFileStats = {
                isDirectory: () => false,
                isFile: () => true,
                size: 1024,
                mtime: new Date(),
            };

            (mockedFs.stat as jest.Mock).mockImplementation((filePath: string) => {
                if (filePath === path.join(testPath, 'file1.ts') || filePath === path.join(testPath, 'file2.js')) {
                    return Promise.resolve(mockFileStats as any);
                }
                return Promise.resolve(mockStats as any);
            });

            mockedFs.readdir.mockResolvedValue([
                { name: 'file1.ts', isDirectory: () => false, isFile: () => true },
                { name: 'file2.js', isDirectory: () => false, isFile: () => true },
            ] as any);

            mockedGitignoreParser.parseGitignore.mockResolvedValue([]);

            // Mock the calculateFileHash method
            MockedFileUtils.FileUtils.calculateFileHash = jest.fn().mockResolvedValue('testhash123');

            // Mock isBinaryFile to return false
            MockedFileUtils.FileUtils.isBinaryFile = jest.fn().mockResolvedValue(false);

            // Mock detectLanguage to return different languages
            MockedFileUtils.FileUtils.detectLanguage = jest.fn()
                .mockReturnValueOnce('typescript')
                .mockReturnValueOnce('javascript');

            const stats = await fileSystemTraversal.getDirectoryStats(testPath);

            expect(stats.totalFiles).toBe(2);
            expect(stats.totalSize).toBe(2048);
            expect(stats.filesByLanguage).toEqual({
                typescript: 1,
                javascript: 1,
            });
            expect(stats.largestFiles).toHaveLength(2);
        });
    });

    describe('Enhanced Ignore Rule Integration', () => {
        it('should integrate default, gitignore, indexignore, and custom ignore patterns', async () => {
            const testPath = '/test/path';

            // Mock GitignoreParser methods
            mockedGitignoreParser.getAllGitignorePatterns.mockResolvedValue([
                '**/node_modules/**',
                '**/dist/**',
                'src/**/build/**'  // Subdirectory gitignore pattern
            ]);

            mockedGitignoreParser.parseIndexignore.mockResolvedValue([
                '**/*.tmp',
                '**/temp/**'
            ]);

            // Mock directory structure - simpler to avoid recursion
            const mockStats = {
                isDirectory: () => true,
                isFile: () => false,
                size: 1024,
                mtime: new Date(),
            };

            const mockFileStats = {
                isDirectory: () => false,
                isFile: () => true,
                size: 1024,
                mtime: new Date(),
            };

            // Mock fs.realpathSync to prevent infinite recursion
            fsSync.realpathSync.mockImplementation((path: any) => String(path));

            // Mock stat to return different results based on path
            (mockedFs.stat as jest.Mock).mockImplementation((filePath: string) => {
                // Only process the root directory and one file
                if (filePath === testPath) {
                    return Promise.resolve(mockStats as any);
                }
                if (filePath === path.join(testPath, 'file.ts')) {
                    return Promise.resolve(mockFileStats as any);
                }
                // Everything else should be treated as non-existent to avoid recursion
                const error = new Error('File not found') as any;
                error.code = 'ENOENT';
                return Promise.reject(error);
            });

            // Mock readdir to return only simple files, no directories to prevent recursion
            mockedFs.readdir.mockResolvedValue([
                { name: 'file.ts', isDirectory: () => false, isFile: () => true },      // Should be included
                { name: 'file.tmp', isDirectory: () => false, isFile: () => true },     // Should be excluded by .indexignore
                { name: 'node_modules', isDirectory: () => true, isFile: () => false }, // Should be excluded by default patterns
            ] as any);

            // Mock helper methods
            MockedFileUtils.FileUtils.calculateFileHash = jest.fn().mockResolvedValue('testhash123');
            MockedFileUtils.FileUtils.isBinaryFile = jest.fn().mockResolvedValue(false);
            MockedFileUtils.FileUtils.getFileName = jest.fn().mockReturnValue('file'); // Return 'file' instead of 'file1'

            // Execute with custom exclude patterns
            const result = await fileSystemTraversal.traverseDirectory(testPath, {
                excludePatterns: ['**/custom-exclude/**']  // Custom pattern
            });

            // Verify that refreshIgnoreRules was called
            expect(mockIgnoreRulesManager.refreshIgnoreRules).toHaveBeenCalledWith(testPath, {
                excludePatterns: ['**/custom-exclude/**']
            });

            // Should only include the .ts file, excluding .tmp and node_modules
            expect(result.files).toHaveLength(1);
            expect(result.files[0].name).toBe('file'); // getFileName returns without extension

            // Verify debug logging was called with pattern counts
            expect(mockLogger.debug).toHaveBeenCalledWith(
                expect.stringContaining('Traversal options for'), // The actual log message
                expect.objectContaining({
                    includePatterns: expect.any(Array),
                    excludePatterns: expect.any(Array),
                    supportedExtensions: expect.any(Array),
                    maxFileSize: expect.any(Number),
                    ignoreHiddenFiles: expect.any(Boolean),
                    ignoreDirectories: expect.any(Array),
                    respectGitignore: expect.any(Boolean)
                })
            );
        });

        it('should handle respectGitignore=false correctly', async () => {
            const testPath = '/test/path';

            // Mock only indexignore (should still be processed even when respectGitignore=false)
            mockedGitignoreParser.parseIndexignore.mockResolvedValue(['**/*.tmp']);

            // Mock directory structure
            const mockStats = { isDirectory: () => true, isFile: () => false, size: 1024, mtime: new Date() };
            const mockFileStats = { isDirectory: () => false, isFile: () => true, size: 1024, mtime: new Date() };

            (mockedFs.stat as jest.Mock).mockImplementation((filePath: string) => {
                if (filePath.includes('file.ts')) {
                    return Promise.resolve(mockFileStats as any);
                }
                return Promise.resolve(mockStats as any);
            });

            mockedFs.readdir.mockResolvedValue([
                { name: 'file.ts', isDirectory: () => false, isFile: () => true }
            ] as any);

            // Mock helper methods
            MockedFileUtils.FileUtils.calculateFileHash = jest.fn().mockResolvedValue('testhash123');
            MockedFileUtils.FileUtils.isBinaryFile = jest.fn().mockResolvedValue(false);

            // Execute with respectGitignore=false
            const result = await fileSystemTraversal.traverseDirectory(testPath, {
                respectGitignore: false
            });

            // Verify refreshIgnoreRules was called with respectGitignore=false
            expect(mockIgnoreRulesManager.refreshIgnoreRules).toHaveBeenCalledWith(testPath, {
                respectGitignore: false
            });

            // Should still have default patterns applied
            expect(result.files).toHaveLength(1);
        });
    });
});

describe('findChangedFiles', () => {
    // Create mock logger for static method tests
    const mockLogger = {
        debug: jest.fn(),
        info: jest.fn(),
        warn: jest.fn(),
        error: jest.fn(),
        getLogFilePath: jest.fn(),
        markAsNormalExit: jest.fn(),
    } as unknown as LoggerService;

    let mockIgnoreRulesManager: any;

    beforeEach(() => {
        // Mock IgnoreRulesManager
        mockIgnoreRulesManager = {
            refreshIgnoreRules: jest.fn().mockResolvedValue(undefined),
            getIgnorePatternsForPath: jest.fn().mockReturnValue([]),
        };
        MockedIgnoreRulesManager.IgnoreRulesManager = jest.fn().mockImplementation(() => mockIgnoreRulesManager);

        // Mock FileUtils methods
        MockedFileUtils.FileUtils = {
            calculateFileHash: jest.fn().mockResolvedValue('testhash123'),
            isBinaryFile: jest.fn().mockResolvedValue(false),
            getFileContent: jest.fn().mockResolvedValue('export const test = true;'),
            getFileExtension: jest.fn().mockReturnValue('.ts'),
            getFileName: jest.fn().mockReturnValue('file1'),
            detectLanguage: jest.fn().mockReturnValue('typescript'),
        };

        // Mock PatternMatcher methods
        MockedPatternMatcher.PatternMatcher = {
            shouldIgnoreFile: jest.fn().mockReturnValue(false),
            shouldIgnoreDirectory: jest.fn().mockReturnValue(false),
            matchesPattern: jest.fn().mockReturnValue(false),
        };
    });

    it('should find files that have changed since last traversal', async () => {
        const testPath = '/test/path';
        const mockStats = {
            isDirectory: () => true,
            isFile: () => false,
            size: 1024,
            mtime: new Date(),
        };

        const mockFileStats = {
            isDirectory: () => false,
            isFile: () => true,
            size: 1024,
            mtime: new Date(),
        };

        (mockedFs.stat as jest.Mock).mockImplementation((filePath: string) => {
            if (filePath === path.join(testPath, 'file1.ts')) {
                return Promise.resolve(mockFileStats as any);
            }
            return Promise.resolve(mockStats as any);
        });

        mockedFs.readdir.mockResolvedValue([
            { name: 'file1.ts', isDirectory: () => false, isFile: () => true },
        ] as any);

        // Create a traversal instance and mock its methods
        const traversal = new FileSystemTraversal(mockLogger, {
            includePatterns: ['**/*.ts', '**/*.js'],
            excludePatterns: ['**/node_modules/**', '**/.git/**'],
            maxFileSize: 512000,
            supportedExtensions: ['.ts', '.js', '.tsx', '.jsx'],
            followSymlinks: false,
            ignoreHiddenFiles: true,
            ignoreDirectories: ['node_modules', '.git', 'dist', 'build'],
            respectGitignore: true,
        } as any);
        
        // Mock the calculateFileHash method
        MockedFileUtils.FileUtils.calculateFileHash = jest.fn()
            .mockResolvedValueOnce('newhash123') // For the current file
            .mockResolvedValueOnce('newhash123'); // For the same file during comparison

        // Mock isBinaryFile to return false
        MockedFileUtils.FileUtils.isBinaryFile = jest.fn().mockResolvedValue(false);

        // Create a previous hashes map with a different hash for the same file
        const previousHashes = new Map<string, string>();
        previousHashes.set('file1.ts', 'oldhash456');

        const changedFiles = await traversal.findChangedFiles(testPath, previousHashes);

        expect(changedFiles).toHaveLength(1);
        expect(changedFiles[0].relativePath).toBe('file1.ts');
    });

    it('should return empty array if no files have changed', async () => {
        const testPath = '/test/path';
        const mockStats = {
            isDirectory: () => true,
            isFile: () => false,
            size: 1024,
            mtime: new Date(),
        };

        const mockFileStats = {
            isDirectory: () => false,
            isFile: () => true,
            size: 1024,
            mtime: new Date(),
        };

        (mockedFs.stat as jest.Mock).mockImplementation((filePath: string) => {
            if (filePath === path.join(testPath, 'file1.ts')) {
                return Promise.resolve(mockFileStats as any);
            }
            return Promise.resolve(mockStats as any);
        });

        mockedFs.readdir.mockResolvedValue([
            { name: 'file1.ts', isDirectory: () => false, isFile: () => true },
        ] as any);

        // Create a traversal instance and mock its methods
        const traversal = new FileSystemTraversal(mockLogger, {
            includePatterns: ['**/*.ts', '**/*.js'],
            excludePatterns: ['**/node_modules/**', '**/.git/**'],
            maxFileSize: 512000,
            supportedExtensions: ['.ts', '.js', '.tsx', '.jsx'],
            followSymlinks: false,
            ignoreHiddenFiles: true,
            ignoreDirectories: ['node_modules', '.git', 'dist', 'build'],
            respectGitignore: true,
        } as any);
        
        // Mock the calculateFileHash method
        MockedFileUtils.FileUtils.calculateFileHash = jest.fn()
            .mockResolvedValueOnce('samehash789') // For the current file
            .mockResolvedValueOnce('samehash789'); // For the same file during comparison

        // Mock isBinaryFile to return false
        MockedFileUtils.FileUtils.isBinaryFile = jest.fn().mockResolvedValue(false);

        // Create a previous hashes map with the same hash for the file
        const previousHashes = new Map<string, string>();
        previousHashes.set('file1.ts', 'samehash789');

        const changedFiles = await traversal.findChangedFiles(testPath, previousHashes);

        expect(changedFiles).toHaveLength(0);
    });

    it('should return new files that were not previously tracked', async () => {
        const testPath = '/test/path';
        const mockStats = {
            isDirectory: () => true,
            isFile: () => false,
            size: 1024,
            mtime: new Date(),
        };

        const mockFileStats = {
            isDirectory: () => false,
            isFile: () => true,
            size: 1024,
            mtime: new Date(),
        };

        (mockedFs.stat as jest.Mock).mockImplementation((filePath: string) => {
            if (filePath === path.join(testPath, 'newfile.ts')) {
                return Promise.resolve(mockFileStats as any);
            }
            return Promise.resolve(mockStats as any);
        });

        mockedFs.readdir.mockResolvedValue([
            { name: 'newfile.ts', isDirectory: () => false, isFile: () => true },
        ] as any);

        // Create a traversal instance and mock its methods
        const traversal = new FileSystemTraversal(mockLogger, {
            includePatterns: ['**/*.ts', '**/*.js'],
            excludePatterns: ['**/node_modules/**', '**/.git/**'],
            maxFileSize: 512000,
            supportedExtensions: ['.ts', '.js', '.tsx', '.jsx'],
            followSymlinks: false,
            ignoreHiddenFiles: true,
            ignoreDirectories: ['node_modules', '.git', 'dist', 'build'],
            respectGitignore: true,
        } as any);
        
        // Mock the calculateFileHash method
        MockedFileUtils.FileUtils.calculateFileHash = jest.fn().mockResolvedValue('newhash123');

        // Mock isBinaryFile to return false
        MockedFileUtils.FileUtils.isBinaryFile = jest.fn().mockResolvedValue(false);

        // Create a previous hashes map without the new file
        const previousHashes = new Map<string, string>();

        const changedFiles = await traversal.findChangedFiles(testPath, previousHashes);

        expect(changedFiles).toHaveLength(1);
        expect(changedFiles[0].relativePath).toBe('newfile.ts');
    });
});

describe('getSupportedExtensions', () => {
    let mockLogger: LoggerService;
    let mockOptions: TraversalOptions;

    beforeEach(() => {
        mockLogger = {
            info: jest.fn(),
            error: jest.fn(),
            warn: jest.fn(),
            debug: jest.fn(),
            getLogFilePath: jest.fn(),
            markAsNormalExit: jest.fn(),
        } as any;

        mockOptions = {
            includePatterns: ['**/*.ts', '**/*.js'],
            excludePatterns: ['**/node_modules/**', '**/.git/**'],
            maxFileSize: 512000,
            supportedExtensions: ['.ts', '.js', '.tsx', '.jsx'],
            followSymlinks: false,
            ignoreHiddenFiles: true,
            ignoreDirectories: ['node_modules', '.git', 'dist', 'build'],
            respectGitignore: true,
        };
    });

    it('should return the list of supported file extensions', () => {
        const traversal = new FileSystemTraversal(mockLogger, mockOptions as any);
        const extensions = traversal.getSupportedExtensions();
        expect(Array.isArray(extensions)).toBe(true);
        expect(extensions.length).toBeGreaterThan(0);
        expect(extensions).toContain('.ts');
        expect(extensions).toContain('.js');
    });

    it('should return a copy of the extensions array', () => {
        const traversal = new FileSystemTraversal(mockLogger, mockOptions as any);
        const extensions1 = traversal.getSupportedExtensions();
        const extensions2 = traversal.getSupportedExtensions();

        // Verify they are separate arrays
        expect(extensions1).not.toBe(extensions2);

        // Verify they have the same content
        expect(extensions1).toEqual(extensions2);
    });
});