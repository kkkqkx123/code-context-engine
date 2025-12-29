import { injectable, inject, optional } from 'inversify';
import * as fs from 'fs/promises';
import * as fsSync from 'fs';
import * as path from 'path';
import { LoggerService } from '../../utils/LoggerService';
import { TYPES } from '../../types';
import { DEFAULT_SUPPORTED_EXTENSIONS } from '../parser/constants/language-constants';
import { PatternMatcher } from '../../utils/filesystem/PatternMatcher';
import { FileUtils } from '../../utils/filesystem/FileUtils';
import { IgnoreRulesManager } from '../../utils/filesystem/IgnoreRulesManager';
import { FileHashManager } from './FileHashManager';

export interface FileInfo {
    path: string;
    relativePath: string;
    name: string;
    extension: string;
    size: number;
    hash: string;
    lastModified: Date;
    language: string;
    isBinary: boolean;
}

export interface TraversalOptions {
    includePatterns?: string[];
    excludePatterns?: string[];
    maxFileSize?: number;
    supportedExtensions?: string[];
    followSymlinks?: boolean;
    ignoreHiddenFiles?: boolean;
    ignoreDirectories?: string[];
    respectGitignore?: boolean;
}

export interface TraversalResult {
    files: FileInfo[];
    directories: string[];
    errors: string[];
    totalSize: number;
    processingTime: number;
}

@injectable()
export class FileSystemTraversal {
    private defaultOptions: Required<TraversalOptions>;
    private ignoreRulesManager: IgnoreRulesManager;
    private fileHashManager?: FileHashManager;
    private projectId?: string; // 用于 FileHashManager 操作

    constructor(
        @inject(TYPES.LoggerService) private logger: LoggerService,
        @inject('TraversalOptions') @optional() options?: TraversalOptions,
        @inject(TYPES.FileHashManager) @optional() fileHashManager?: FileHashManager
    ) {
        this.defaultOptions = {
            includePatterns: options?.includePatterns ?? [],
            excludePatterns: options?.excludePatterns ?? [
                '**/node_modules/**',
                '**/.git/**',
                '**/dist/**',
                '**/build/**',
            ],
            maxFileSize: options?.maxFileSize ?? 512000, // 500KB (512000 字节)
            // 使用 language-constants.ts 中的 DEFAULT_SUPPORTED_EXTENSIONS 以保持一致性
            supportedExtensions: options?.supportedExtensions ?? DEFAULT_SUPPORTED_EXTENSIONS,
            followSymlinks: options?.followSymlinks ?? false,
            ignoreHiddenFiles: options?.ignoreHiddenFiles ?? true,
            ignoreDirectories: options?.ignoreDirectories ?? [
                'node_modules',
                '.git',
                'dist',
                'build',
                'coverage',
                'logs',
            ],
            respectGitignore: options?.respectGitignore ?? true,
        };

        // 初始化忽略规则管理器
        this.ignoreRulesManager = new IgnoreRulesManager(this.logger);

        // 注入可选依赖
        this.fileHashManager = fileHashManager;
    }

    /**
     * 设置项目ID（用于 FileHashManager 操作）
     */
    setProjectId(projectId: string): void {
        this.projectId = projectId;
    }

    /**
     * 刷新特定根路径的忽略模式
     * 这允许对忽略规则进行热更新（例如，当 .gitignore 或 .indexignore 文件更改时）
     */
    async refreshIgnoreRules(rootPath: string, options?: TraversalOptions): Promise<void> {
        await this.ignoreRulesManager.refreshIgnoreRules(rootPath, options);
    }

    async traverseDirectory(rootPath: string, options?: TraversalOptions): Promise<TraversalResult> {
        const startTime = Date.now();

        // Refresh ignore rules for this path before traversal
        await this.refreshIgnoreRules(rootPath, options);

        const traversalOptions = { ...this.defaultOptions, ...options };

        // 获取此路径的忽略模式
        const allIgnorePatterns = this.ignoreRulesManager.getIgnorePatternsForPath(rootPath);

        // 使用最新的忽略模式更新遍历选项
        const updatedOptions = {
            ...traversalOptions,
            excludePatterns: allIgnorePatterns
        };

        // 调试：记录遍历选项
        this.logger.debug(`[DEBUG] Traversal options for ${rootPath}`, {
            includePatterns: updatedOptions.includePatterns,
            excludePatterns: updatedOptions.excludePatterns,
            supportedExtensions: updatedOptions.supportedExtensions,
            maxFileSize: updatedOptions.maxFileSize,
            ignoreHiddenFiles: updatedOptions.ignoreHiddenFiles,
            ignoreDirectories: updatedOptions.ignoreDirectories,
            respectGitignore: updatedOptions.respectGitignore
        });

        const result: TraversalResult = {
            files: [],
            directories: [],
            errors: [],
            totalSize: 0,
            processingTime: 0,
        };

        try {
            await this.traverseRecursive(rootPath, rootPath, result, updatedOptions);
            result.processingTime = Date.now() - startTime;

            // 调试：记录最终结果
            this.logger.debug(`[DEBUG] Traversal completed for ${rootPath}`, {
                filesFound: result.files.length,
                directoriesFound: result.directories.length,
                errors: result.errors,
                processingTime: result.processingTime,
                ignorePatternsApplied: updatedOptions.excludePatterns?.length || 0,
                files: result.files.map(f => ({ path: f.path, extension: f.extension, language: f.language }))
            });
        } catch (error) {
            result.errors.push(
                `Failed to traverse directory: ${error instanceof Error ? error.message : String(error)}`
            );
            result.processingTime = Date.now() - startTime;

            // 调试：记录错误
            this.logger.error(`[DEBUG] Traversal failed for ${rootPath}`, error);
        }

        return result;
    }

    private async traverseRecursive(
        currentPath: string,
        rootPath: string,
        result: TraversalResult,
        options: Required<TraversalOptions>,
        visitedPaths: Set<string> = new Set()
    ): Promise<void> {
        try {
            // 检查循环引用来防止无限递归
            let realPath: string;
            try {
                realPath = fsSync.realpathSync(currentPath);
            } catch (error) {
                result.errors.push(`Cannot resolve real path: ${currentPath}`);
                return;
            }

            if (visitedPaths.has(realPath)) {
                result.errors.push(`Circular reference detected: ${currentPath}`);
                return;
            }
            visitedPaths.add(realPath);

            // 限制递归深度以防止堆栈溢出
            if (visitedPaths.size > 1000) {
                result.errors.push(`Maximum recursion depth exceeded: ${currentPath}`);
                return;
            }

            const stats = await fs.stat(currentPath);
            const relativePath = path.relative(rootPath, currentPath);

            if (stats.isDirectory()) {
                await this.processDirectory(currentPath, relativePath, rootPath, result, options, visitedPaths);
            } else if (stats.isFile()) {
                await this.processFile(currentPath, relativePath, stats, result, options);
            }
        } catch (error) {
            // 如果这是根路径，则重新抛出异常，以便被外部 try-catch 捕获
            if (currentPath === rootPath) {
                throw error;
            }
            result.errors.push(
                `Error accessing ${currentPath}: ${error instanceof Error ? error.message : String(error)}`
            );
        }
    }

    private async processDirectory(
        dirPath: string,
        relativePath: string,
        rootPath: string,
        result: TraversalResult,
        options: Required<TraversalOptions>,
        visitedPaths: Set<string>
    ): Promise<void> {
        const dirName = path.basename(dirPath);

        if (PatternMatcher.shouldIgnoreDirectory(dirName, options)) {
            return;
        }

        if (relativePath !== '') {
            result.directories.push(relativePath);
        }

        try {
            const entries = await fs.readdir(dirPath, { withFileTypes: true });

            for (const entry of entries) {
                const fullPath = path.join(dirPath, entry.name);

                if (entry.isDirectory()) {
                    await this.traverseRecursive(fullPath, rootPath, result, options, visitedPaths);
                } else if (entry.isFile() || (entry.isSymbolicLink() && options.followSymlinks)) {
                    await this.traverseRecursive(fullPath, rootPath, result, options, visitedPaths);
                }
            }
        } catch (error) {
            result.errors.push(
                `Error reading directory ${dirPath}: ${error instanceof Error ? error.message : String(error)}`
            );
        }
    }

    private async processFile(
        filePath: string,
        relativePath: string,
        stats: fsSync.Stats,
        result: TraversalResult,
        options: Required<TraversalOptions>
    ): Promise<void> {
        // 调试：记录文件处理尝试
        this.logger.debug(`[DEBUG] Processing file`, { filePath, relativePath });

        if (PatternMatcher.shouldIgnoreFile(relativePath, options)) {
            this.logger.debug(`[DEBUG] File ignored by pattern`, { relativePath });
            return;
        }

        if (stats.size > options.maxFileSize) {
            result.errors.push(`File too large: ${relativePath} (${stats.size} bytes)`);
            this.logger.debug(`[DEBUG] File too large`, { relativePath, size: stats.size });
            return;
        }

        const extension = FileUtils.getFileExtension(filePath);
        const language = FileUtils.detectLanguage(extension, options.supportedExtensions);

        this.logger.debug(`[DEBUG] File detected`, { extension, language, supported: options.supportedExtensions.includes(extension) });

        if (!language) {
            this.logger.debug(`[DEBUG] File skipped - unsupported extension`, { extension });
            return;
        }

        try {
            this.logger.debug(`[DEBUG] Starting isBinaryFile check for ${relativePath}`);
            const isBinary = await FileUtils.isBinaryFile(filePath, this.logger);
            this.logger.debug(`[DEBUG] File binary check completed`, { isBinary });

            if (isBinary) {
                this.logger.debug(`[DEBUG] File skipped - binary file`, { relativePath });
                return;
            }

            // 使用 FileHashManager 计算哈希（如果可用），否则回退到 FileUtils
            let hash: string;
            if (this.fileHashManager && this.projectId) {
                this.logger.debug(`[DEBUG] Starting hash calculation via FileHashManager for ${relativePath}`);
                // 先检查数据库缓存
                const cachedHash = await this.fileHashManager.getFileHash(this.projectId, filePath);
                if (cachedHash) {
                    hash = cachedHash;
                    this.logger.debug(`[DEBUG] Hash from FileHashManager cache`, { hash });
                } else {
                    // 计算新哈希
                    hash = await FileUtils.calculateFileHash(filePath, this.logger);
                    // 异步更新 FileHashManager
                    await this.fileHashManager.updateFileHash(this.projectId, filePath, hash, {
                        lastModified: stats.mtime,
                        fileSize: stats.size,
                        language
                    });
                    this.logger.debug(`[DEBUG] Hash updated in FileHashManager`, { hash });
                }
            } else {
                this.logger.debug(`[DEBUG] Starting hash calculation for ${relativePath}`);
                hash = await FileUtils.calculateFileHash(filePath, this.logger);
                this.logger.debug(`[DEBUG] Hash calculation completed`, { hash });
            }

            const fileInfo: FileInfo = {
                path: filePath,
                relativePath,
                name: FileUtils.getFileName(filePath),
                extension,
                size: stats.size,
                hash,
                lastModified: stats.mtime,
                language,
                isBinary,
            };

            result.files.push(fileInfo);
            result.totalSize += stats.size;
            this.logger.debug(`[DEBUG] File added to results`, { relativePath });
        } catch (error) {
            const errorMessage = error instanceof Error ? error.message : String(error);
            result.errors.push(
                `Error processing file ${relativePath}: ${errorMessage}`
            );
            this.logger.error(`[DEBUG] Error processing file ${relativePath}`, { error: errorMessage, stack: error instanceof Error ? error.stack : undefined });
        }
    }

    async findChangedFiles(
        rootPath: string,
        previousHashes: Map<string, string>,
        options?: TraversalOptions
    ): Promise<FileInfo[]> {
        const result = await this.traverseDirectory(rootPath, options);
        const changedFiles: FileInfo[] = [];

        for (const file of result.files) {
            const previousHash = previousHashes.get(file.relativePath);

            if (!previousHash || previousHash !== file.hash) {
                changedFiles.push(file);
            }
        }

        return changedFiles;
    }

    async getFileContent(filePath: string): Promise<string> {
        return await FileUtils.getFileContent(filePath);
    }

    async getDirectoryStats(
        rootPath: string,
        options?: TraversalOptions
    ): Promise<{
        totalFiles: number;
        totalSize: number;
        filesByLanguage: Record<string, number>;
        largestFiles: FileInfo[];
    }> {
        const result = await this.traverseDirectory(rootPath, options);

        const filesByLanguage: Record<string, number> = {};
        const largestFiles = [...result.files].sort((a, b) => b.size - a.size).slice(0, 10);

        for (const file of result.files) {
            filesByLanguage[file.language] = (filesByLanguage[file.language] || 0) + 1;
        }

        return {
            totalFiles: result.files.length,
            totalSize: result.totalSize,
            filesByLanguage,
            largestFiles,
        };
    }

    /**
     * 获取支持的文件扩展名列表
     * 为 FileTraversalService 提供统一的扩展名检查功能
     */
    getSupportedExtensions(): string[] {
        return [...this.defaultOptions.supportedExtensions];
    }

    /**
     * 清除特定路径的忽略规则缓存
     */
    clearIgnoreRulesCache(rootPath: string): void {
        this.ignoreRulesManager.clearCacheForPath(rootPath);
    }
    /**
     * 检查文件是否为二进制文件
     * 提供公共方法以便外部调用
     */
    async isBinaryFile(filePath: string): Promise<boolean> {
        return await FileUtils.isBinaryFile(filePath, this.logger);
    }

    /**
     * 清除所有忽略规则缓存
     */
    clearAllIgnoreRulesCache(): void {
        this.ignoreRulesManager.clearAllCache();
    }

    /**
     * 获取 FileHashManager 引用（用于外部操作）
     */
    getFileHashManager(): FileHashManager | undefined {
        return this.fileHashManager;
    }
}