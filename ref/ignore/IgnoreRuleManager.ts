import { EventEmitter } from 'events';
import * as fs from 'fs/promises';
import * as fsSync from 'fs';
import * as path from 'path';
import { GitignoreParser } from './GitignoreParser';
import { LoggerService } from '../../utils/LoggerService';
import { DEFAULT_IGNORE_PATTERNS } from './defaultIgnorePatterns';

/**
 * 忽略规则管理器
 * 负责管理.gitignore和.indexignore规则，支持热更新
 */
export class IgnoreRuleManager extends EventEmitter {
  private rules: Map<string, string[]> = new Map(); // projectPath -> patterns
  private fileWatchers: Map<string, { gitignoreWatcher: fsSync.FSWatcher | undefined, indexignoreWatcher: fsSync.FSWatcher | undefined, subDirWatchers: Map<string, fsSync.FSWatcher> }> = new Map(); // projectPath -> watchers
  private checkIntervals: Map<string, NodeJS.Timeout> = new Map(); // projectPath -> intervalId

  constructor(private logger: LoggerService) {
    super();
  }

  /**
   * 获取项目的所有忽略规则
   */
  async getIgnorePatterns(projectPath: string): Promise<string[]> {
    if (this.rules.has(projectPath)) {
      return this.rules.get(projectPath)!;
    }

    const patterns = await this.loadIgnorePatterns(projectPath);
    this.rules.set(projectPath, patterns);

    // 启动文件监听（热更新准备）
    await this.startWatching(projectPath);

    return patterns;
  }

  /**
   * 加载所有忽略规则
   */
  private async loadIgnorePatterns(projectPath: string): Promise<string[]> {
    const patterns: string[] = [];

    // 1. 默认规则
    patterns.push(...this.getDefaultIgnorePatterns());

    // 2. .gitignore规则
    const gitignorePatterns = await GitignoreParser.getAllGitignorePatterns(projectPath);
    patterns.push(...gitignorePatterns);

    // 3. .indexignore规则
    const indexignorePatterns = await GitignoreParser.parseIndexignore(projectPath);
    patterns.push(...indexignorePatterns);

    return [...new Set(patterns)]; // 去重
  }

  /**
   * 启动文件监听（热更新准备）
   */
  private async startWatching(projectPath: string): Promise<void> {
    // 如果已经在监听，则先停止之前的监听
    if (this.fileWatchers.has(projectPath)) {
      await this.stopWatching(projectPath);
    }

    const watchers: {
      gitignoreWatcher: fsSync.FSWatcher | undefined,
      indexignoreWatcher: fsSync.FSWatcher | undefined,
      subDirWatchers: Map<string, fsSync.FSWatcher>
    } = {
      gitignoreWatcher: undefined,
      indexignoreWatcher: undefined,
      subDirWatchers: new Map<string, fsSync.FSWatcher>()
    };

    try {
      // 监听根目录的 .gitignore 文件
      const gitignorePath = path.join(projectPath, '.gitignore');
      try {
        await fs.access(gitignorePath);
        const gitignoreWatcher = fsSync.watch(gitignorePath, (eventType) => {
          if (eventType === 'change') {
            this.handleRuleChange(projectPath, '.gitignore');
          }
        });
        watchers.gitignoreWatcher = gitignoreWatcher;
      } catch (error) {
        // 文件不存在，创建一个定时器来定期检查文件是否被创建
        const checkInterval = setInterval(async () => {
          try {
            await fs.access(gitignorePath);
            // 文件被创建了，清除定时器并启动监听
            clearInterval(checkInterval);
            this.checkIntervals.delete(projectPath);
            // 检查是否仍然需要启动监听器（可能在定时器运行期间已经调用了stopWatching）
            if (this.fileWatchers.has(projectPath)) {
              const gitignoreWatcher = fsSync.watch(gitignorePath, (eventType) => {
                if (eventType === 'change') {
                  this.handleRuleChange(projectPath, '.gitignore');
                }
              });
              watchers.gitignoreWatcher = gitignoreWatcher;
              this.logger.info(`[IgnoreRuleManager] Started watching .gitignore for ${projectPath}`);
            }
          } catch (e) {
            // 文件还不存在，继续等待
            // 检查定时器是否仍然存在（可能在等待期间已经调用了stopWatching）
            if (!this.checkIntervals.has(projectPath)) {
              clearInterval(checkInterval);
            }
          }
        }, 5000); // 每5秒检查一次
        this.checkIntervals.set(projectPath, checkInterval);
      }

      // 监听根目录的 .indexignore 文件
      const indexignorePath = path.join(projectPath, '.indexignore');
      try {
        await fs.access(indexignorePath);
        const indexignoreWatcher = fsSync.watch(indexignorePath, (eventType) => {
          if (eventType === 'change') {
            this.handleRuleChange(projectPath, '.indexignore');
          }
        });
        watchers.indexignoreWatcher = indexignoreWatcher;
      } catch (error) {
        // 文件不存在，创建一个定时器来定期检查文件是否被创建
        const checkInterval = setInterval(async () => {
          try {
            await fs.access(indexignorePath);
            // 文件被创建了，清除定时器并启动监听
            clearInterval(checkInterval);
            this.checkIntervals.delete(projectPath);
            // 检查是否仍然需要启动监听器（可能在定时器运行期间已经调用了stopWatching）
            if (this.fileWatchers.has(projectPath)) {
              const indexignoreWatcher = fsSync.watch(indexignorePath, (eventType) => {
                if (eventType === 'change') {
                  this.handleRuleChange(projectPath, '.indexignore');
                }
              });
              watchers.indexignoreWatcher = indexignoreWatcher;
              this.logger.info(`[IgnoreRuleManager] Started watching .indexignore for ${projectPath}`);
            }
          } catch (e) {
            // 文件还不存在，继续等待
            // 检查定时器是否仍然存在（可能在等待期间已经调用了stopWatching）
            if (!this.checkIntervals.has(projectPath)) {
              clearInterval(checkInterval);
            }
          }
        }, 5000); // 每5秒检查一次
        this.checkIntervals.set(projectPath, checkInterval);
      }

      // 监听一级子目录中的 .gitignore 文件变化
      try {
        const entries = await fs.readdir(projectPath, { withFileTypes: true });
        for (const entry of entries) {
          if (entry.isDirectory()) {
            const subDirPath = path.join(projectPath, entry.name);
            const subGitignorePath = path.join(subDirPath, '.gitignore');

            try {
              await fs.access(subGitignorePath);
              const subWatcher = fsSync.watch(subGitignorePath, (eventType) => {
                if (eventType === 'change') {
                  this.handleRuleChange(projectPath, `${entry.name}/.gitignore`);
                }
              });

              // 存储监听器
              watchers.subDirWatchers.set(entry.name, subWatcher);
            } catch (error) {
              // 子目录中的 .gitignore 文件不存在，无需处理
            }
          }
        }
      } catch (error) {
        // 无法读取项目目录，记录错误但不停止执行
        this.logger.error(`[IgnoreRuleManager] Failed to read project directory: ${projectPath}`, error);
      }

      // 存储所有监听器
      this.fileWatchers.set(projectPath, watchers);
      this.logger.info(`[IgnoreRuleManager] Started watching ignore files for ${projectPath}`);
    } catch (error) {
      this.logger.error(`[IgnoreRuleManager] Failed to start watching ignore files for ${projectPath}`, error);
    }
  }

  /**
   * 停止文件监听
   */
  async stopWatching(projectPath: string): Promise<void> {
    const watchers = this.fileWatchers.get(projectPath);
    if (watchers) {
      // 关闭根目录 .gitignore 监听器
      if (watchers.gitignoreWatcher) {
        watchers.gitignoreWatcher.close();
      }

      // 关闭根目录 .indexignore 监听器
      if (watchers.indexignoreWatcher) {
        watchers.indexignoreWatcher.close();
      }

      // 关闭子目录监听器
      for (const [_, watcher] of watchers.subDirWatchers) {
        watcher.close();
      }
    }

    // 清除所有与此项目相关的检查定时器
    const intervalId = this.checkIntervals.get(projectPath);
    if (intervalId) {
      clearInterval(intervalId);
      this.checkIntervals.delete(projectPath);
    }

    // 从映射中移除监听器
    this.fileWatchers.delete(projectPath);
    this.logger.info(`[IgnoreRuleManager] Stopped watching ignore files for ${projectPath}`);
  }

  /**
   * 强制清理所有资源
   */
  async forceCleanup(): Promise<void> {
    // 清理所有监听器
    for (const [projectPath] of this.fileWatchers) {
      await this.stopWatching(projectPath);
    }

    // 清理所有定时器
    for (const [projectPath, intervalId] of this.checkIntervals) {
      clearInterval(intervalId);
    }
    this.checkIntervals.clear();

    // 移除所有事件监听器
    this.removeAllListeners();
  }

  /**
   * 处理规则变化
   */
  private async handleRuleChange(projectPath: string, changedFile: string): Promise<void> {
    this.logger.info(`[IgnoreRuleManager] Detected change in ignore file: ${changedFile} for project: ${projectPath}`);

    // 重新加载规则
    const newPatterns = await this.loadIgnorePatterns(projectPath);

    // 更新缓存
    this.rules.set(projectPath, newPatterns);

    // 发出规则变化事件
    this.emit('rulesChanged', projectPath, newPatterns, changedFile);
    this.logger.info(`[IgnoreRuleManager] Rules updated for project: ${projectPath}, patterns count: ${newPatterns.length}`);
  }

  /**
   * 获取默认忽略规则
   */
  private getDefaultIgnorePatterns(): string[] {
    // 返回统一的默认忽略规则列表
    return DEFAULT_IGNORE_PATTERNS;
  }

  /**
   * 获取项目当前的忽略规则
   */
  getCurrentPatterns(projectPath: string): string[] | undefined {
    return this.rules.get(projectPath);
  }
}