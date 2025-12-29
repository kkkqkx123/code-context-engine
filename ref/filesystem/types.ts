import { FileInfo, TraversalOptions, TraversalResult } from './FileSystemTraversal';
import { FileWatcherOptions, FileWatcherCallbacks } from './FileWatcherService';
import { ChangeDetectionOptions, ChangeDetectionCallbacks, FileChangeEvent } from './ChangeDetectionService';

export interface IFileSystemService {
  // 文件遍历相关方法
  traverseDirectory(rootPath: string, options?: TraversalOptions): Promise<TraversalResult>;
  getFileContent(filePath: string): Promise<string>;
  getDirectoryStats(rootPath: string, options?: TraversalOptions): Promise<{
    totalFiles: number;
    totalSize: number;
    filesByLanguage: Record<string, number>;
    largestFiles: FileInfo[];
  }>;

  // 文件监控相关方法
  startWatching(options: FileWatcherOptions): Promise<void>;
  stopWatching(): Promise<void>;
  setCallbacks(callbacks: FileWatcherCallbacks): void;
  isWatchingPath(watchPath: string): boolean;
  getWatchedPaths(): string[];

  // 变更检测相关方法
  initialize(rootPaths: string[], watcherOptions?: FileWatcherOptions): Promise<void>;
  stop(): Promise<void>;
  setCallbacks(callbacks: ChangeDetectionCallbacks): void;
  getFileHash(relativePath: string): string | undefined;
  getFileHistory(relativePath: string): any[];
  getAllFileHashes(): Map<string, string>;
  isFileTracked(relativePath: string): boolean;
  getTrackedFilesCount(): number;
  isServiceRunning(): boolean;
}