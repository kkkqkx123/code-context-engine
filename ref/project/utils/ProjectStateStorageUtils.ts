
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { ProjectState } from '../ProjectStateManager';
import * as fs from 'fs/promises';
import * as path from 'path';

/**
 * 项目状态存储工具类
 * 职责：文件读写、重试机制、原子操作
 */
export class ProjectStateStorageUtils {
  /**
   * 保存项目状态（带重试机制）
   */
  static async saveProjectStates(
    projectStates: Map<string, ProjectState>,
    storagePath: string,
    logger: LoggerService,
    errorHandler: ErrorHandlerService
  ): Promise<void> {
    const maxRetries = 5;
    let lastError: Error | null = null;

    for (let attempt = 1; attempt <= maxRetries; attempt++) {
      try {
        // 确保目录存在
        await ProjectStateStorageUtils.ensureStorageDirectory(storagePath);

        // 转换为数组并序列化
        const states = Array.from(projectStates.values());
        const jsonData = JSON.stringify(states, null, 2);

        // 使用临时文件+重命名的方式实现原子写入
        await ProjectStateStorageUtils.atomicWriteWithRetry(storagePath, jsonData, attempt, maxRetries, logger);

        logger.debug(`Saved ${states.length} project states (attempt ${attempt})`);
        return; // 成功保存，退出重试循环
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));
        logger.warn(`Failed to save project states (attempt ${attempt}/${maxRetries}): ${lastError.message}`);

        if (attempt < maxRetries) {
          // 等待一段时间后重试，使用指数退避
          await new Promise(resolve => setTimeout(resolve, Math.pow(2, attempt) * 100));
        }
      }
    }

    // 所有重试都失败，但不抛出错误，因为内存状态仍然是正确的
    logger.error(`Failed to save project states after ${maxRetries} attempts, but memory state is still valid: ${lastError?.message || 'Unknown error'}`);
    // 不抛出错误，因为内存中的状态仍然是正确的
  }

  /**
   * 加载项目状态
   */
  static async loadProjectStates(
    storagePath: string,
    logger: LoggerService,
    validateAndNormalizeProjectState: (rawState: any) => ProjectState
  ): Promise<Map<string, ProjectState>> {
    try {
      // 确保目录存在
      await ProjectStateStorageUtils.ensureStorageDirectory(storagePath);

      // 读取状态文件
      const data = await fs.readFile(storagePath, 'utf-8');
      const rawStates = JSON.parse(data);

      // 验证和恢复项目状态
      const projectStates = new Map<string, ProjectState>();
      let validStateCount = 0;
      let invalidStateCount = 0;
      let duplicatePathCount = 0;

      // 用于跟踪已处理的 projectPath，防止重复
      const { HashUtils } = await import('../../../utils/cache/HashUtils');
      const processedPaths = new Set<string>();

      for (const rawState of rawStates) {
        try {
          const validatedState = validateAndNormalizeProjectState(rawState);

          // 检查 projectPath 是否已存在
          const normalizedPath = HashUtils.normalizePath(validatedState.projectPath);
          if (processedPaths.has(normalizedPath)) {
            logger.warn(`Skipping duplicate project path: ${normalizedPath}`, { projectId: validatedState.projectId });
            duplicatePathCount++;
            continue;
          }

          // 检查 projectId 是否已存在（防止数据不一致）
          if (projectStates.has(validatedState.projectId)) {
            logger.warn(`Skipping duplicate project ID: ${validatedState.projectId}`, { projectPath: normalizedPath });
            invalidStateCount++;
            continue;
          }

          // 添加到处理过的路径集合
          processedPaths.add(normalizedPath);

          // 添加到项目状态映射
          projectStates.set(validatedState.projectId, validatedState);
          validStateCount++;
        } catch (validationError) {
          logger.warn(`Skipping invalid project state: ${validationError instanceof Error ? validationError.message : String(validationError)}`, { rawState });
          invalidStateCount++;
        }
      }

      if (duplicatePathCount > 0) {
        logger.warn(`Detected and skipped ${duplicatePathCount} duplicate project paths`);
      }



      logger.info(`Loaded ${validStateCount} valid project states, skipped ${invalidStateCount + duplicatePathCount} invalid states`);
      return projectStates;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
        // 文件不存在，初始化空状态
        logger.info('Project states file does not exist, initializing empty states');
        return new Map<string, ProjectState>();
      } else {
        throw error;
      }
    }
  }

  /**
   * 确保存储目录存在
   */
  static async ensureStorageDirectory(storagePath: string): Promise<void> {
    const dirPath = path.dirname(storagePath);
    await fs.mkdir(dirPath, { recursive: true });
  }

  /**
   * 原子写入重试机制
   */
  private static async atomicWriteWithRetry(
    storagePath: string,
    jsonData: string,
    attempt: number,
    maxRetries: number,
    logger: LoggerService
  ): Promise<void> {
    // 使用临时文件+重命名的方式实现原子写入
    const tempPath = `${storagePath}.tmp.${Date.now()}.${Math.random().toString(36).substring(2, 15)}`; // 使用唯一临时文件名

    try {
      // 写入临时文件
      await fs.writeFile(tempPath, jsonData);

      // 原子性重命名
      await fs.rename(tempPath, storagePath);
    } catch (writeError: any) {
      // 清理临时文件（如果存在）
      try {
        await fs.unlink(tempPath);
      } catch { }

      // 如果是权限错误，尝试直接写入目标文件作为后备方案
      if (writeError.code === 'EPERM' || writeError.code === 'EACCES') {
        logger.warn(`Permission error during atomic write, trying direct write as fallback`);
        try {
          await fs.writeFile(storagePath, jsonData);
          logger.debug(`Saved project states using direct write fallback`);
          return;
        } catch (directWriteError: any) {
          logger.warn(`Direct write also failed: ${directWriteError.message || directWriteError}`);
        }
      }

      throw writeError;
    }
  }

  /**
   * 格式化日期为ISO字符串
   */
  static formatDate(date: Date): string {
    return date.toISOString();
  }

  /**
   * 解析ISO字符串为Date对象
   */
  static parseDate(dateString: string): Date {
    return new Date(dateString);
  }
}