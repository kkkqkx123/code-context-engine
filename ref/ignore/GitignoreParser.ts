import * as fs from 'fs/promises';
import * as path from 'path';

export class GitignoreParser {
  /**
   * 解析 .gitignore 文件并返回忽略模式数组
   * @param gitignorePath .gitignore 文件的路径
   * @returns 忽略模式数组
   */
  static async parseGitignore(gitignorePath: string): Promise<string[]> {
    try {
      const content = await fs.readFile(gitignorePath, 'utf-8');
      return this.parseContent(content);
    } catch (error) {
      // 如果文件不存在，返回空数组
      if ((error as any).code === 'ENOENT') {
        return [];
      }
      throw error;
    }
  }

  /**
   * 解析 .gitignore 文件的内容并返回忽略模式数组
   * @param content .gitignore 文件的内容
   * @returns 忽略模式数组
   */
  static parseContent(content: string): string[] {
    // 处理内容可能为 undefined 或 null 的情况
    if (!content) {
      return [];
    }

    const patterns: string[] = [];

    const lines = content.split('\n');

    for (const line of lines) {
      // 跳过空行和注释
      const trimmedLine = line.trim();
      if (trimmedLine === '' || trimmedLine.startsWith('#')) {
        continue;
      }

      // 将 gitignore 模式转换为 glob 模式
      const pattern = this.convertGitignorePattern(trimmedLine);
      // 只添加非空模式（跳过否定模式）
      if (pattern !== '') {
        patterns.push(pattern);
      }
    }

    return patterns;
  }

  /**
   * 将 gitignore 模式转换为可用于匹配逻辑的 glob 模式
   * @param pattern Gitignore 模式
   * @returns 转换后的 glob 模式
   */
  private static convertGitignorePattern(pattern: string): string {
    // 处理否定模式（我们暂时跳过这些，因为系统不支持它们）
    if (pattern.startsWith('!')) {
      // 暂时跳过否定模式
      return '';
    }

    // 标准化路径分隔符
    pattern = pattern.replace(/\\/g, '/');

    // 处理以 / 开头的模式
    if (pattern.startsWith('/')) {
      // 从根目录开始的绝对模式 - 移除开头的 /
      pattern = pattern.substring(1);
      // 如果是简单模式（没有更多斜杠）且不以 / 结尾，则是根文件模式
      if (!pattern.includes('/') && !pattern.endsWith('/')) {
        return pattern;
      }
      // 如果是以 / 结尾的目录模式，转换为 glob 模式
      if (pattern.endsWith('/')) {
        return pattern.slice(0, -1) + '/**';
      }
      // 对于其他带路径的绝对模式，添加 **/ 前缀
      return '**/' + pattern;
    } else if (!pattern.includes('/')) {
      // 简单文件名模式 - 匹配任何位置的同名文件
      // 对于像 *.js 这样的简单模式，我们想要匹配：
      // 1. 直接文件：file.js
      // 2. 目录中的文件：path/to/file.js
      // 我们将创建一个更灵活的模式来匹配两者
      return '**/' + pattern;
    } else if (!pattern.endsWith('/')) {
      // 带路径但不以 / 结尾的模式 - 同时匹配文件和目录
      pattern = '**/' + pattern;
    } else if (pattern.endsWith('/')) {
      // 目录模式 - 在任何目录中匹配
      pattern = '**/' + pattern.slice(0, -1) + '/**';
    }

    // 处理以 / 结尾的目录模式（这些是绝对的）
    if (pattern.endsWith('/')) {
      pattern = pattern.slice(0, -1) + '/**';
    }

    return pattern;
  }

  /**
   * 通过从根目录到文件遍历目录，获取项目的所有 gitignore 模式
   * @param projectRoot 项目的根目录
   * @param filePath 相对于项目根目录的文件路径
   * @returns 所有适用的 gitignore 模式数组
   */
  static async getGitignorePatternsForFile(projectRoot: string, filePath: string): Promise<string[]> {
    const patterns: string[] = [];

    // 标准化文件路径以确保一致的路径分隔符
    const normalizedFilePath = path.normalize(filePath);
    const fileDir = path.dirname(normalizedFilePath);

    // 将目录路径拆分为各个目录
    const dirs = fileDir.split(path.sep).filter(dir => dir !== '');

    // 从根目录的 .gitignore 开始
    const rootGitignorePath = path.join(projectRoot, '.gitignore');
    const rootPatterns = await this.parseGitignore(rootGitignorePath);
    patterns.push(...rootPatterns);

    // 遍历每个目录并收集 .gitignore 模式
    let currentPath = projectRoot;
    for (const dir of dirs) {
      if (dir === '' || dir === '.') continue;

      currentPath = path.join(currentPath, dir);
      const gitignorePath = path.join(currentPath, '.gitignore');
      const dirPatterns = await this.parseGitignore(gitignorePath);
      patterns.push(...dirPatterns);
    }

    // 过滤掉空模式并返回
    const filteredPatterns = patterns.filter(pattern => pattern !== '');
    return filteredPatterns;
  }

  /**
   * 从根目录和一级子目录获取所有适用的 .gitignore 规则
   * @param projectRoot 项目根目录
   * @returns 所有适用的忽略规则数组
   */
  static async getAllGitignorePatterns(projectRoot: string): Promise<string[]> {
    const patterns: string[] = [];

    // 1. 读取根目录的 .gitignore
    const rootGitignorePath = path.join(projectRoot, '.gitignore');
    const rootPatterns = await this.parseGitignore(rootGitignorePath);
    patterns.push(...rootPatterns);

    // 2. 读取一级子目录中的 .gitignore 文件
    try {
      const entries = await fs.readdir(projectRoot, { withFileTypes: true });
      for (const entry of entries) {
        if (entry.isDirectory()) {
          const subDirPath = path.join(projectRoot, entry.name);
          const subGitignorePath = path.join(subDirPath, '.gitignore');
          const subPatterns = await this.parseGitignore(subGitignorePath);

          // 为子目录规则添加前缀以确保它们只应用于相应目录
          // 只添加非空模式
          const prefixedPatterns = subPatterns
            .filter(pattern => pattern !== '')
            .map(pattern => path.join(entry.name, pattern).replace(/\\/g, '/'));
          patterns.push(...prefixedPatterns);
        }
      }
    } catch (error) {
      // 忽略读取错误，继续执行
      console.warn(`Failed to read subdirectories: ${error}`);
    }

    return patterns.filter(pattern => pattern !== '');
  }

  /**
   * 解析 .indexignore 文件
   * @param projectRoot 项目根目录
   * @returns 来自 .indexignore 的规则数组
   */
  static async parseIndexignore(projectRoot: string): Promise<string[]> {
    const indexignorePath = path.join(projectRoot, '.indexignore');
    try {
      const content = await fs.readFile(indexignorePath, 'utf-8');
      return this.parseContent(content);
    } catch (error) {
      if ((error as any).code === 'ENOENT') {
        return [];
      }
      throw error;
    }
  }
}
