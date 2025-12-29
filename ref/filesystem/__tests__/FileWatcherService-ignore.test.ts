import { FileWatcherService } from '../FileWatcherService';
import { FileSystemTraversal } from '../FileSystemTraversal';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import * as path from 'path';
import * as fs from 'fs/promises';
import { TYPES } from '../../../types';

describe('FileWatcherService Ignore Rules', () => {
  let fileWatcherService: FileWatcherService;
  let fileSystemTraversal: FileSystemTraversal;
  let loggerService: LoggerService;
  let errorHandlerService: ErrorHandlerService;
  let testDir: string;

  beforeEach(async () => {
    loggerService = new LoggerService();
    errorHandlerService = new ErrorHandlerService(loggerService);
    fileSystemTraversal = new FileSystemTraversal(loggerService);

    // Create a temporary test directory
    testDir = path.join(__dirname, 'test-temp');
    await fs.mkdir(testDir, { recursive: true });

    fileWatcherService = new FileWatcherService(
      loggerService,
      errorHandlerService,
      {} as any, // HotReloadRecoveryService
      fileSystemTraversal
    );
  });

  afterEach(async () => {
    // Clean up test directory
    try {
      await fs.rm(testDir, { recursive: true, force: true });
    } catch (error) {
      // Ignore cleanup errors
    }
  });

  test('should properly initialize with default ignore patterns', async () => {
    // Initialize the service
    await fileWatcherService['initializeIgnorePatterns']();

    // Check that default patterns are loaded
    const allIgnorePatterns = fileWatcherService['allIgnorePatterns'];
    expect(allIgnorePatterns).toContain('**/node_modules/**');
    expect(allIgnorePatterns).toContain('.git/**');
    expect(allIgnorePatterns).toContain('dist/**');
    expect(allIgnorePatterns).toContain('build/**');
  });

  test('should ignore files based on patterns', async () => {
    // Initialize the service
    await fileWatcherService['initializeIgnorePatterns']();

    // Test node_modules path
    const shouldIgnoreNodeModules = fileWatcherService['shouldIgnoreFile']('src/node_modules/some-file.js');
    expect(shouldIgnoreNodeModules).toBe(true);

    // Test .git path
    const shouldIgnoreGit = fileWatcherService['shouldIgnoreFile']('.git/config');
    expect(shouldIgnoreGit).toBe(true);

    // Test dist path
    const shouldIgnoreDist = fileWatcherService['shouldIgnoreFile']('dist/bundle.js');
    expect(shouldIgnoreDist).toBe(true);

    // Test normal file (should not be ignored)
    const shouldNotIgnoreNormal = fileWatcherService['shouldIgnoreFile']('src/index.ts');
    expect(shouldNotIgnoreNormal).toBe(false);
  });

  test('should respect hidden file settings', async () => {
    // Initialize the service
    await fileWatcherService['initializeIgnorePatterns']();

    // Test hidden file (should be ignored by default)
    const shouldIgnoreHidden = fileWatcherService['shouldIgnoreFile']('.env');
    expect(shouldIgnoreHidden).toBe(true);

    // Test that normal files are not ignored
    const shouldNotIgnoreNormal = fileWatcherService['shouldIgnoreFile']('src/index.ts');
    expect(shouldNotIgnoreNormal).toBe(false);
  });

  test('should support language detection', async () => {
    // Test TypeScript file
    const tsLanguage = fileWatcherService['detectLanguage']('.ts');
    expect(tsLanguage).toBe('typescript');

    // Test JavaScript file
    const jsLanguage = fileWatcherService['detectLanguage']('.js');
    expect(jsLanguage).toBe('javascript');

    // Test Python file
    const pyLanguage = fileWatcherService['detectLanguage']('.py');
    expect(pyLanguage).toBe('python');

    // Test unsupported extension
    const unsupportedLanguage = fileWatcherService['detectLanguage']('.xyz');
    expect(unsupportedLanguage).toBeNull();
  });

  test('should refresh ignore rules when paths change', async () => {
    // Create a temporary directory to watch
    const watchDir = path.join(testDir, 'watch-test');
    await fs.mkdir(watchDir, { recursive: true });

    // Create a .gitignore file in the watched directory
    const gitignorePath = path.join(watchDir, '.gitignore');
    await fs.writeFile(gitignorePath, 'test-file.txt\n*.log\nnode_modules/');
    
    // Refresh ignore rules for this path
    await fileWatcherService['refreshIgnoreRules'](watchDir);

    // Check that the gitignore patterns are loaded
    const allIgnorePatterns = fileWatcherService['allIgnorePatterns'];
    // Note: gitignore patterns are converted with **/ prefix by GitignoreParser
    expect(allIgnorePatterns).toContain('**/test-file.txt');

    // Test that files matching gitignore patterns are ignored
    const shouldIgnoreTestFile = fileWatcherService['shouldIgnoreFile']('test-file.txt');
    expect(shouldIgnoreTestFile).toBe(true);

    const shouldIgnoreLogFile = fileWatcherService['shouldIgnoreFile']('app.log');
    expect(shouldIgnoreLogFile).toBe(true);
  });

  test('should not duplicate ignore patterns when refreshing multiple times', async () => {
    // Create a temporary directory to watch
    const watchDir = path.join(testDir, 'watch-test-duplicate');
    await fs.mkdir(watchDir, { recursive: true });

    // Create a .gitignore file in the watched directory
    const gitignorePath = path.join(watchDir, '.gitignore');
    await fs.writeFile(gitignorePath, 'test-file.txt\n*.log');

    // Get initial count of patterns
    const initialPatternCount = fileWatcherService['allIgnorePatterns'].length;

    // Refresh ignore rules multiple times
    await fileWatcherService['refreshIgnoreRules'](watchDir);
    await fileWatcherService['refreshIgnoreRules'](watchDir);
    await fileWatcherService['refreshIgnoreRules'](watchDir);

    // Check that the pattern count hasn't grown excessively
    const finalPatternCount = fileWatcherService['allIgnorePatterns'].length;
    
    // Should not have tripled the patterns
    expect(finalPatternCount).toBeLessThan(initialPatternCount * 2);
    
    // Should still contain the expected patterns
    expect(fileWatcherService['allIgnorePatterns']).toContain('**/node_modules/**');
    expect(fileWatcherService['allIgnorePatterns']).toContain('**/test-file.txt');
  });
});