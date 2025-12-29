import { GitignoreParser } from '../GitignoreParser';
import * as fs from 'fs/promises';
import * as path from 'path';

// Mock fs module
jest.mock('fs/promises');
const mockedFs = fs as jest.Mocked<typeof fs>;

describe('GitignoreParser', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  describe('parseGitignore', () => {
    test('should parse gitignore file and return patterns', async () => {
      const mockContent = `
# Comment
node_modules/
dist/
*.log
.DS_Store
`;
      mockedFs.readFile.mockResolvedValue(mockContent);

      const patterns = await GitignoreParser.parseGitignore('/test/.gitignore');

      expect(patterns).toEqual([
        '**/node_modules/**',
        '**/dist/**',
        '**/*.log',
        '**/.DS_Store'
      ]);
      expect(mockedFs.readFile).toHaveBeenCalledWith('/test/.gitignore', 'utf-8');
    });

    test('should handle non-existent gitignore file', async () => {
      mockedFs.readFile.mockRejectedValue({ code: 'ENOENT' });

      const patterns = await GitignoreParser.parseGitignore('/non-existent/.gitignore');

      expect(patterns).toEqual([]);
    });

    test('should throw error for other read errors', async () => {
      mockedFs.readFile.mockRejectedValue(new Error('Permission denied'));

      await expect(GitignoreParser.parseGitignore('/test/.gitignore')).rejects.toThrow('Permission denied');
    });

    test('should handle empty gitignore file', async () => {
      mockedFs.readFile.mockResolvedValue('');

      const patterns = await GitignoreParser.parseGitignore('/test/.gitignore');

      expect(patterns).toEqual([]);
    });
  });

  describe('parseContent', () => {
    test('should parse gitignore content and return patterns', () => {
      const content = `
# Comment
node_modules/
dist/
*.log
.DS_Store
!important.txt
`;

      const patterns = GitignoreParser.parseContent(content);

      expect(patterns).toEqual([
        '**/node_modules/**',
        '**/dist/**',
        '**/*.log',
        '**/.DS_Store'
        // Negation pattern (!important.txt) should be skipped
      ]);
    });

    test('should handle empty content', () => {
      const patterns = GitignoreParser.parseContent('');
      expect(patterns).toEqual([]);
    });

    test('should handle undefined content', () => {
      const patterns = GitignoreParser.parseContent(undefined as any);
      expect(patterns).toEqual([]);
    });

    test('should handle null content', () => {
      const patterns = GitignoreParser.parseContent(null as any);
      expect(patterns).toEqual([]);
    });

    test('should skip comments and empty lines', () => {
      const content = `
# This is a comment
node_modules/

# Another comment
dist/

*.log

.DS_Store
`;

      const patterns = GitignoreParser.parseContent(content);

      expect(patterns).toEqual([
        '**/node_modules/**',
        '**/dist/**',
        '**/*.log',
        '**/.DS_Store'
      ]);
    });

    test('should skip negation patterns', () => {
      const content = `
node_modules/
!important.txt
dist/
!another.txt
`;

      const patterns = GitignoreParser.parseContent(content);

      expect(patterns).toEqual([
        '**/node_modules/**',
        '**/dist/**'
      ]);
    });
  });

  describe('convertGitignorePattern', () => {
    test('should convert absolute patterns', () => {
      expect(GitignoreParser['convertGitignorePattern']('/src/')).toBe('src/**');
      expect(GitignoreParser['convertGitignorePattern']('/src/file.ts')).toBe('**/src/file.ts');
      expect(GitignoreParser['convertGitignorePattern']('/file.txt')).toBe('file.txt');
    });

    test('should convert simple file patterns', () => {
      expect(GitignoreParser['convertGitignorePattern']('*.log')).toBe('**/*.log');
      expect(GitignoreParser['convertGitignorePattern']('node_modules')).toBe('**/node_modules');
    });

    test('should convert directory patterns', () => {
      expect(GitignoreParser['convertGitignorePattern']('dist/')).toBe('**/dist/**');
      expect(GitignoreParser['convertGitignorePattern']('src/')).toBe('**/src/**');
    });

    test('should convert path patterns', () => {
      expect(GitignoreParser['convertGitignorePattern']('src/utils')).toBe('**/src/utils');
      expect(GitignoreParser['convertGitignorePattern']('src/utils/')).toBe('**/src/utils/**');
    });

    test('should handle negation patterns', () => {
      expect(GitignoreParser['convertGitignorePattern']('!important.txt')).toBe('');
      expect(GitignoreParser['convertGitignorePattern']('!src/important')).toBe('');
    });

    test('should normalize path separators', () => {
      expect(GitignoreParser['convertGitignorePattern']('src\\utils\\file.ts')).toBe('**/src/utils/file.ts');
      expect(GitignoreParser['convertGitignorePattern']('dist\\')).toBe('**/dist/**');
    });
  });

  describe('getGitignorePatternsForFile', () => {
    beforeEach(() => {
      // Mock the static parseGitignore method
      jest.spyOn(GitignoreParser, 'parseGitignore');
    });

    afterEach(() => {
      jest.restoreAllMocks();
    });

    test('should get patterns from root gitignore', async () => {
      // Mock the root gitignore to return patterns
      (GitignoreParser.parseGitignore as jest.Mock)
        .mockResolvedValueOnce(['**/node_modules/**', '**/dist/**'])
        .mockResolvedValue([]);

      const patterns = await GitignoreParser.getGitignorePatternsForFile(
        '/project',
        'src/utils/file.ts'
      );

      expect(patterns).toEqual(['**/node_modules/**', '**/dist/**']);
      expect(GitignoreParser.parseGitignore).toHaveBeenCalledWith(path.join('/project', '.gitignore'));
    });

    test('should get patterns from nested gitignore files', async () => {
      // Create a custom mock implementation
      const mockParseGitignore = jest.fn()
        .mockResolvedValueOnce(['**/node_modules/**'])  // root .gitignore
        .mockResolvedValueOnce(['**/build/**'])         // src/.gitignore
        .mockResolvedValueOnce(['**/temp/**']);         // src/utils/.gitignore

      // Replace the original method with our mock
      const originalParseGitignore = GitignoreParser.parseGitignore;
      GitignoreParser.parseGitignore = mockParseGitignore;

      const patterns = await GitignoreParser.getGitignorePatternsForFile(
        '/project',
        'src/utils/file.ts'
      );

      expect(patterns).toEqual(['**/node_modules/**', '**/build/**', '**/temp/**']);
      expect(mockParseGitignore).toHaveBeenCalledWith(path.join('/project', '.gitignore'));
      expect(mockParseGitignore).toHaveBeenCalledWith(path.join('/project', 'src', '.gitignore'));
      expect(mockParseGitignore).toHaveBeenCalledWith(path.join('/project', 'src', 'utils', '.gitignore'));

      // Restore the original method
      GitignoreParser.parseGitignore = originalParseGitignore;
    });

    test('should handle file at root', async () => {
      // Mock the root gitignore to return patterns
      (GitignoreParser.parseGitignore as jest.Mock)
        .mockResolvedValueOnce(['**/node_modules/**'])
        .mockResolvedValue([]);

      const patterns = await GitignoreParser.getGitignorePatternsForFile(
        '/project',
        'file.ts'
      );

      expect(patterns).toEqual(['**/node_modules/**']);
      expect(GitignoreParser.parseGitignore).toHaveBeenCalledWith(path.join('/project', '.gitignore'));
    });

    test('should filter out empty patterns', async () => {
      // Mock the gitignore files to return patterns including empty ones
      (GitignoreParser.parseGitignore as jest.Mock)
        .mockResolvedValueOnce(['**/node_modules/**', ''])  // root .gitignore with empty pattern
        .mockResolvedValue(['']);                          // src/.gitignore with only empty pattern

      const patterns = await GitignoreParser.getGitignorePatternsForFile(
        '/project',
        'src/utils/file.ts'
      );

      expect(patterns).toEqual(['**/node_modules/**']);
    });
  });

  describe('getAllGitignorePatterns', () => {
    beforeEach(() => {
      jest.spyOn(GitignoreParser, 'parseGitignore');
    });

    afterEach(() => {
      jest.restoreAllMocks();
    });

    test('should get patterns from root and first-level subdirectories', async () => {
      // Mock fs.readdir
      const mockEntries = [
        { name: 'src', isDirectory: () => true },
        { name: 'tests', isDirectory: () => true },
        { name: 'package.json', isDirectory: () => false }
      ];
      mockedFs.readdir.mockResolvedValue(mockEntries as any);

      // Mock parseGitignore calls
      (GitignoreParser.parseGitignore as jest.Mock)
        .mockResolvedValueOnce(['**/node_modules/**', '**/dist/**'])  // root .gitignore
        .mockResolvedValueOnce(['**/build/**'])                     // src/.gitignore
        .mockResolvedValueOnce(['**/coverage/**']);                 // tests/.gitignore

      const patterns = await GitignoreParser.getAllGitignorePatterns('/project');

      expect(patterns).toEqual([
        '**/node_modules/**',
        '**/dist/**',
        'src/**/build/**',
        'tests/**/coverage/**'
      ]);
    });

    test('should handle project with no subdirectories', async () => {
      // Mock fs.readdir with no directories
      const mockEntries = [
        { name: 'package.json', isDirectory: () => false },
        { name: 'README.md', isDirectory: () => false }
      ];
      mockedFs.readdir.mockResolvedValue(mockEntries as any);

      // Mock root gitignore
      (GitignoreParser.parseGitignore as jest.Mock)
        .mockResolvedValueOnce(['**/node_modules/**']);

      const patterns = await GitignoreParser.getAllGitignorePatterns('/project');

      expect(patterns).toEqual(['**/node_modules/**']);
      expect(GitignoreParser.parseGitignore).toHaveBeenCalledTimes(1);
    });

    test('should handle readdir errors gracefully', async () => {
      // Mock readdir to throw error
      mockedFs.readdir.mockRejectedValue(new Error('Permission denied'));

      // Mock root gitignore
      (GitignoreParser.parseGitignore as jest.Mock)
        .mockResolvedValueOnce(['**/node_modules/**']);

      // Mock console.warn to avoid test output pollution
      const consoleWarnSpy = jest.spyOn(console, 'warn').mockImplementation();

      const patterns = await GitignoreParser.getAllGitignorePatterns('/project');

      expect(patterns).toEqual(['**/node_modules/**']);
      expect(consoleWarnSpy).toHaveBeenCalledWith('Failed to read subdirectories: Error: Permission denied');

      consoleWarnSpy.mockRestore();
    });

    test('should filter out empty patterns', async () => {
      // Mock fs.readdir
      const mockEntries = [
        { name: 'src', isDirectory: () => true }
      ];
      mockedFs.readdir.mockResolvedValue(mockEntries as any);

      // Mock parseGitignore with empty patterns
      (GitignoreParser.parseGitignore as jest.Mock)
        .mockResolvedValueOnce(['**/node_modules/**', ''])  // root with empty pattern
        .mockResolvedValueOnce(['']);                      // src with empty pattern

      const patterns = await GitignoreParser.getAllGitignorePatterns('/project');

      expect(patterns).toEqual(['**/node_modules/**']);
    });
  });

  describe('parseIndexignore', () => {
    test('should parse .indexignore file', async () => {
      const mockContent = `
# Index ignore patterns
*.tmp
temp/
logs/
!important.log
`;
      mockedFs.readFile.mockResolvedValue(mockContent);

      const patterns = await GitignoreParser.parseIndexignore('/project');

      expect(patterns).toEqual([
        '**/*.tmp',
        '**/temp/**',
        '**/logs/**'
        // Negation pattern (!important.log) should be skipped
      ]);
      expect(mockedFs.readFile).toHaveBeenCalledWith(path.join('/project', '.indexignore'), 'utf-8');
    });

    test('should handle non-existent .indexignore file', async () => {
      mockedFs.readFile.mockRejectedValue({ code: 'ENOENT' });

      const patterns = await GitignoreParser.parseIndexignore('/project');

      expect(patterns).toEqual([]);
    });

    test('should throw error for other read errors', async () => {
      mockedFs.readFile.mockRejectedValue(new Error('Permission denied'));

      await expect(GitignoreParser.parseIndexignore('/project')).rejects.toThrow('Permission denied');
    });

    test('should handle empty .indexignore file', async () => {
      mockedFs.readFile.mockResolvedValue('');

      const patterns = await GitignoreParser.parseIndexignore('/project');

      expect(patterns).toEqual([]);
    });
  });
});