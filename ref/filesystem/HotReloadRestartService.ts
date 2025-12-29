import { injectable, inject } from 'inversify';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { TYPES } from '../../types';
import { ProjectHotReloadService } from './ProjectHotReloadService';
import { ChangeDetectionService } from './ChangeDetectionService';
import { HotReloadConfigService } from './HotReloadConfigService';
import { HotReloadRecoveryService } from './HotReloadRecoveryService';
import { ProjectStateManager } from '../project/ProjectStateManager';
import * as fs from 'fs/promises';
import * as path from 'path';

export interface RestartState {
  phase: 'idle' | 'preparing' | 'saving' | 'restoring' | 'completed' | 'failed';
  timestamp: Date;
  projects: string[];
  error?: any;
}

export interface RestartStateData {
  projects: Array<{
    projectPath: string;
    config: any;
    status: any;
    metrics: any;
  }>;
  timestamp: Date;
}

@injectable()
export class HotReloadRestartService {
  private restartState: RestartState = { phase: 'idle', timestamp: new Date(), projects: [] };
  private readonly RESTART_STATE_FILE = './hotreload-restart-state.json';
  
  constructor(
    @inject(TYPES.ProjectHotReloadService) private projectHotReloadService: ProjectHotReloadService,
    @inject(TYPES.ChangeDetectionService) private changeDetectionService: ChangeDetectionService,
    @inject(TYPES.HotReloadConfigService) private configService: HotReloadConfigService,
    @inject(TYPES.HotReloadRecoveryService) private recoveryService: HotReloadRecoveryService,
    @inject(TYPES.ProjectStateManager) private projectStateManager: ProjectStateManager,
    @inject(TYPES.LoggerService) private logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) private errorHandler: ErrorHandlerService
  ) {}
  
  /**
   * 在应用启动时处理重启逻辑
   */
async handleApplicationRestart(): Promise<void> {
    this.restartState = { phase: 'preparing', timestamp: new Date(), projects: [] };
    
    try {
      this.logger.info('Starting hot reload restart process...');
      
      // 阶段1：尝试恢复之前的状态
      await this.restoreStateAfterRestart();
      
      // 阶段2：使用ProjectHotReloadService恢复所有已索引项目的监听
      await this.restoreProjectWatchingThroughProjectHotReloadService();
      
      // 阶段3：验证功能完整性
      await this.validateFunctionality();
      
      this.restartState = {
        phase: 'completed',
        timestamp: new Date(),
        projects: Array.from(this.projectHotReloadService.getAllProjectStatuses().keys())
      };
      
      this.logger.info('Hot reload restart process completed successfully');
    } catch (error) {
      this.restartState = {
        phase: 'failed',
        timestamp: new Date(),
        projects: [],
        error
      };
      
      this.errorHandler.handleError(
        new Error(`Failed to handle application restart: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'HotReloadRestartService', operation: 'handleApplicationRestart' }
      );
      
      // 即使失败也尝试进行基本恢复
      await this.performBasicRecovery();
      
      // 如果基本恢复仍然失败，尝试高级恢复策略
      try {
        await this.performAdvancedRecovery({
          error: error instanceof Error ? error.message : String(error),
          phase: 'application_restart'
        });
      } catch (advancedRecoveryError) {
        this.logger.error('Advanced recovery also failed:', advancedRecoveryError);
      }
    }
  }
  
  /**
   * 保存当前状态（在应用关闭前调用）
   */
  async saveCurrentState(): Promise<void> {
    this.restartState = { ...this.restartState, phase: 'saving' };
    
    try {
      const stateData: RestartStateData = {
        projects: [],
        timestamp: new Date()
      };
      
      // 获取所有已启用热更新的项目状态
      const allStatuses = this.projectHotReloadService.getAllProjectStatuses();
      
      for (const [projectPath, status] of allStatuses) {
        if (status.enabled) {
          stateData.projects.push({
            projectPath,
            config: this.projectHotReloadService.getProjectConfig(projectPath),
            status,
            metrics: this.projectHotReloadService.getProjectMetrics(projectPath)
          });
        }
      }
      
      // 保存状态到文件
      await fs.writeFile(
        this.RESTART_STATE_FILE,
        JSON.stringify(stateData, this.replacer, 2)
      );
      
      this.logger.info(`Hot reload state saved for ${stateData.projects.length} projects`);
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to save restart state: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'HotReloadRestartService', operation: 'saveCurrentState' }
      );
      throw error;
    }
  }
  
 /**
   * 重启后恢复状态
   */
  private async restoreStateAfterRestart(): Promise<void> {
    try {
      // 尝试从文件加载状态
      const stateData = await this.loadRestartState();
      
      if (!stateData || stateData.projects.length === 0) {
        this.logger.info('No previous hot reload state found, starting fresh');
        return;
      }
      
      this.logger.info(`Found previous hot reload state for ${stateData.projects.length} projects`);
      
      // 恢复每个项目的热更新
      for (const projectData of stateData.projects) {
        try {
          // 检查项目路径是否存在
          const projectExists = await this.checkProjectExists(projectData.projectPath);
          if (!projectExists) {
            this.logger.warn(`Project no longer exists, skipping: ${projectData.projectPath}`);
            continue;
          }
          
          // 检查项目是否仍然需要索引
          const isProjectIndexed = await this.isProjectIndexed(projectData.projectPath);
          if (!isProjectIndexed) {
            this.logger.warn(`Project not indexed, skipping hot reload: ${projectData.projectPath}`);
            continue;
          }
          
          // 启用项目的热更新
          await this.projectHotReloadService.enableForProject(
            projectData.projectPath,
            projectData.config
          );
          
          this.logger.info(`Restored hot reload for project: ${projectData.projectPath}`);
        } catch (error) {
          this.logger.error(`Failed to restore hot reload for project ${projectData.projectPath}:`, error);
          // 继续处理其他项目
        }
      }
      
      // 更新项目列表
      this.restartState.projects = stateData.projects.map(p => p.projectPath);
    } catch (error) {
      this.logger.warn(`Failed to restore hot reload state: ${error instanceof Error ? error.message : String(error)}`);
      // 这是可选的恢复步骤，失败不应该阻止应用启动
    }
  }
  
  /**
   * 通过ProjectHotReloadService恢复项目监听
   */
  private async restoreProjectWatchingThroughProjectHotReloadService(): Promise<void> {
    try {
      this.logger.info('Restoring project watching through ProjectHotReloadService...');
      
      // 获取所有已索引的项目路径
      const indexedProjectPaths = await this.getIndexedProjects();
      
      if (indexedProjectPaths.length === 0) {
        this.logger.info('No indexed projects found, nothing to restore');
        return;
      }
      
      this.logger.info(`Found ${indexedProjectPaths.length} indexed projects to restore watching for`);
      
      // 为每个已索引的项目启动监听
      for (const projectPath of indexedProjectPaths) {
        try {
          // 检查项目路径是否存在
          const projectExists = await this.checkProjectExists(projectPath);
          if (!projectExists) {
            this.logger.warn(`Project path does not exist, skipping: ${projectPath}`);
            continue;
          }
          
          // 检查项目是否仍然需要索引
          const isProjectIndexed = await this.isProjectIndexed(projectPath);
          if (!isProjectIndexed) {
            this.logger.warn(`Project not indexed, skipping hot reload: ${projectPath}`);
            continue;
          }
          
          // 使用默认配置启用项目的热更新
          const defaultConfig = {
            debounceInterval: 500,
            enabled: true,
            watchPatterns: ['**/*.{js,ts,jsx,tsx,json,md,py,go,java}'],
            ignorePatterns: ['**/node_modules/**', '**/.git/**', '**/dist/**', '**/build/**', '**/target/**', '**/venv/**'],
            maxFileSize: 512000, // 500KB
            errorHandling: {
              maxRetries: 3,
              alertThreshold: 5,
              autoRecovery: true
            }
          };
          
          await this.projectHotReloadService.enableForProject(projectPath, defaultConfig);
          
          this.logger.info(`Restored hot reload for project: ${projectPath}`);
        } catch (error) {
          this.logger.error(`Failed to restore hot reload for project ${projectPath}:`, error);
          // 继续处理其他项目
        }
      }
      
      this.logger.info('Project watching restoration through ProjectHotReloadService completed');
    } catch (error) {
      this.logger.error('Failed to restore project watching through ProjectHotReloadService:', error);
      
      this.errorHandler.handleError(
        new Error(`Failed to restore project watching through ProjectHotReloadService: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'HotReloadRestartService', operation: 'restoreProjectWatchingThroughProjectHotReloadService' }
      );
      
      // 抛出错误，以便调用者知道此操作失败了
      throw error;
    }
  }
  
  /**
   * 验证热更新功能是否正常
   */
  private async validateFunctionality(): Promise<void> {
    try {
      this.logger.info('Validating hot reload functionality...');
      
      // 检查变更检测服务是否运行
      if (!this.changeDetectionService.isServiceRunning()) {
        this.logger.warn('Change detection service is not running, attempting to restart...');
        
        // 获取当前所有已启用热更新的项目
        const allStatuses = this.projectHotReloadService.getAllProjectStatuses();
        const activeProjects = Array.from(allStatuses.entries())
          .filter(([_, status]) => status.enabled && status.isWatching)
          .map(([projectPath, _]) => projectPath);
        
        if (activeProjects.length > 0) {
          // 重新初始化变更检测服务
          await this.changeDetectionService.initialize(activeProjects);
          this.logger.info(`Reinitialized change detection for ${activeProjects.length} projects`);
        }
      }
      
      // 额外的验证：检查索引服务中的项目监听状态
      try {
        const indexedProjectPaths = await this.projectStateManager.getAllProjects();
        if (indexedProjectPaths.length > 0) {
          this.logger.info(`Validating project watching for ${indexedProjectPaths.length} indexed projects...`);
          
          let validatedCount = 0;
          for (const projectPath of indexedProjectPaths) {
            try {
              // 尝试为每个项目触发一次简单的检查
              const projectState = this.projectStateManager.getProjectStateByPath(projectPath);
              if (projectState) {
                validatedCount++;
                this.logger.debug(`Project validation passed for: ${projectPath}`);
              } else {
                this.logger.warn(`Project validation failed for: ${projectPath} (not found in state manager)`);
              }
            } catch (validationError) {
              this.logger.warn(`Project validation error for ${projectPath}:`, validationError);
            }
          }
          
          this.logger.info(`Project validation completed: ${validatedCount}/${indexedProjectPaths.length} projects validated`);
        }
      } catch (validationError) {
        this.logger.warn('Project validation failed during functionality check:', validationError);
      }
      
      this.logger.info('Hot reload functionality validation completed');
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to validate hot reload functionality: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'HotReloadRestartService', operation: 'validateFunctionality' }
      );
    }
  }
  
  /**
   * 执行基本恢复（当完整恢复失败时）
   */
  private async performBasicRecovery(): Promise<void> {
    try {
      this.logger.info('Performing basic hot reload recovery...');
      
      // 调用ProjectHotReloadService中的方法来恢复所有已索引项目的监听
      await this.restoreProjectWatchingThroughProjectHotReloadService();
      
      this.logger.info('Basic hot reload recovery completed through ProjectHotReloadService');
    } catch (error) {
      this.logger.error('Basic recovery through IndexService failed:', error);
      
      // 尝试检查当前已索引的项目并启用热更新
      try {
        const indexedProjects = await this.getIndexedProjects();
        
        for (const projectPath of indexedProjects) {
          try {
            const config = this.configService.getProjectConfig(projectPath);
            if (config.enabled) {
              await this.projectHotReloadService.enableForProject(projectPath, config);
              this.logger.info(`Basic recovery: Enabled hot reload for project: ${projectPath}`);
            }
          } catch (projectError) {
            this.logger.error(`Failed to enable hot reload for project ${projectPath}:`, projectError);
          }
        }
      } catch (fallbackError) {
        this.logger.error('Fallback basic recovery also failed:', fallbackError);
        
        // 最后的恢复尝试：使用ChangeDetectionService直接初始化
        try {
          const indexedProjects = await this.getIndexedProjects();
          if (indexedProjects.length > 0) {
            await this.changeDetectionService.initialize(indexedProjects);
            this.logger.info(`Last resort recovery: Initialized change detection for ${indexedProjects.length} projects`);
          }
        } catch (lastResortError) {
          this.logger.error('Last resort recovery also failed:', lastResortError);
          
          // 如果所有恢复方法都失败了，至少尝试启动变更检测服务
          try {
            await this.changeDetectionService.initialize([process.cwd()]);
            this.logger.info('Recovery: Initialized change detection for current working directory as fallback');
          } catch (finalFallbackError) {
            this.logger.error('All recovery attempts failed, change detection service could not be initialized:', finalFallbackError);
          }
        }
      }
    }
  }

  /**
   * 高级恢复策略 - 用于处理复杂错误场景
   */
  async performAdvancedRecovery(recoveryContext?: any): Promise<void> {
    try {
      this.logger.info('Starting advanced recovery process...', { recoveryContext });

      // 步骤1: 检查并重置所有相关服务
      await this.resetServices();

      // 步骤2: 重新加载项目状态
      await this.reinitializeProjectStates();

      // 步骤3: 重新初始化热更新功能
      await this.reinitializeHotReloadFunctionality();

      // 步骤4: 验证恢复结果
      await this.validateRecoveryResult();

      this.logger.info('Advanced recovery completed successfully');
    } catch (error) {
      this.logger.error('Advanced recovery failed:', error);
      
      // 记录错误到错误处理服务
      this.errorHandler.handleError(
        new Error(`Advanced recovery failed: ${error instanceof Error ? error.message : String(error)}`),
        {
          component: 'HotReloadRestartService',
          operation: 'performAdvancedRecovery',
          recoveryContext
        }
      );
    }
  }

  /**
   * 重置所有相关服务
   */
  private async resetServices(): Promise<void> {
    try {
      this.logger.info('Resetting hot reload related services...');

      // 停止变更检测服务
      if (this.changeDetectionService.isServiceRunning()) {
        await this.changeDetectionService.stop();
        this.logger.info('Change detection service stopped');
      }

      // 重置ProjectHotReloadService状态
      // 注意：这取决于ProjectHotReloadService是否提供了重置方法
      // 如果没有提供，我们可以跳过或记录日志
      this.logger.info('ProjectHotReloadService reset completed (if applicable)');

      this.logger.info('Service reset completed');
    } catch (error) {
      this.logger.warn('Service reset encountered issues:', error);
    }
  }

  /**
   * 重新初始化项目状态
   */
  private async reinitializeProjectStates(): Promise<void> {
    try {
      this.logger.info('Reinitializing project states...');

      // 从ProjectStateManager重新加载项目状态
      await this.projectStateManager.refreshAllProjectStates();
      this.logger.info('Project states reinitialized');

      // 检查有多少项目被成功重新加载
      const allProjects = await this.projectStateManager.getAllProjects();
      this.logger.info(`Reinitialized states for ${allProjects.length} projects`);

    } catch (error) {
      this.logger.error('Failed to reinitialize project states:', error);
      throw error;
    }
  }

  /**
   * 重新初始化热更新功能
   */
  private async reinitializeHotReloadFunctionality(): Promise<void> {
    try {
      this.logger.info('Reinitializing hot reload functionality...');

      // 获取所有已索引的项目
      const indexedProjectPaths = await this.getIndexedProjects();
      this.logger.info(`Found ${indexedProjectPaths.length} indexed projects to reinitialize`);

      if (indexedProjectPaths.length === 0) {
        this.logger.info('No indexed projects found, skipping hot reload reinitialization');
        return;
      }

      // 为每个项目重新启用热更新
      for (const projectPath of indexedProjectPaths) {
        try {
          // 检查项目路径是否存在
          const projectExists = await this.checkProjectExists(projectPath);
          if (!projectExists) {
            this.logger.warn(`Project path does not exist during reinitialization, skipping: ${projectPath}`);
            continue;
          }

          // 使用默认配置重新启用热更新
          const defaultConfig = {
            debounceInterval: 500,
            enabled: true,
            watchPatterns: ['**/*.{js,ts,jsx,tsx,json,md,py,go,java}'],
            ignorePatterns: ['**/node_modules/**', '**/.git/**', '**/dist/**', '**/build/**', '**/target/**', '**/venv/**'],
            maxFileSize: 512000, // 500KB
            errorHandling: {
              maxRetries: 3,
              alertThreshold: 5,
              autoRecovery: true
            }
          };

          await this.projectHotReloadService.enableForProject(projectPath, defaultConfig);
          this.logger.info(`Hot reload reinitialized for project: ${projectPath}`);

        } catch (projectError) {
          this.logger.error(`Failed to reinitialize hot reload for project ${projectPath}:`, projectError);
        }
      }

      this.logger.info('Hot reload functionality reinitialized');
    } catch (error) {
      this.logger.error('Failed to reinitialize hot reload functionality:', error);
      throw error;
    }
  }

  /**
   * 验证恢复结果
   */
  private async validateRecoveryResult(): Promise<void> {
    try {
      this.logger.info('Validating recovery result...');

      // 验证变更检测服务是否运行
      if (!this.changeDetectionService.isServiceRunning()) {
        throw new Error('Change detection service is not running after recovery');
      }

      // 验证是否有项目正在被监听
      const allStatuses = this.projectHotReloadService.getAllProjectStatuses();
      const activeProjects = Array.from(allStatuses.entries())
        .filter(([_, status]) => status.enabled && status.isWatching);

      if (activeProjects.length === 0) {
        this.logger.warn('No projects are actively being watched after recovery');
      } else {
        this.logger.info(`Recovery validation passed: ${activeProjects.length} projects are being watched`);
      }

      this.logger.info('Recovery validation completed');
    } catch (error) {
      this.logger.error('Recovery validation failed:', error);
      throw error;
    }
  }
  
  /**
   * 加载重启状态
   */
  private async loadRestartState(): Promise<RestartStateData | null> {
    try {
      const content = await fs.readFile(this.RESTART_STATE_FILE, 'utf-8');
      
      // 检查文件内容是否为空或只包含空白字符
      if (!content || content.trim() === '') {
        this.logger.info('Restart state file is empty, starting fresh');
        return null;
      }
      
      const data = JSON.parse(content) as RestartStateData;
      
      // 转换timestamp字符串为Date对象
      data.timestamp = new Date(data.timestamp);
      
      return data;
    } catch (error) {
      if ((error as any).code === 'ENOENT') {
        // 文件不存在是正常的
        return null;
      }
      
      // 处理JSON解析错误
      if (error instanceof SyntaxError && error.message.includes('JSON')) {
        this.logger.warn('Invalid JSON in restart state file, starting fresh');
        return null;
      }
      
      this.errorHandler.handleError(
        new Error(`Failed to load restart state: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'HotReloadRestartService', operation: 'loadRestartState' }
      );
      return null;
    }
  }
  
  /**
   * 检查项目路径是否存在
   */
  private async checkProjectExists(projectPath: string): Promise<boolean> {
    try {
      await fs.access(projectPath);
      return true;
    } catch {
      return false;
    }
  }
  
  /**
   * 检查项目是否已被索引
   */
  private async isProjectIndexed(projectPath: string): Promise<boolean> {
    try {
      // 检查项目是否在ProjectStateManager中存在，或通过其他方式验证索引状态
      const projectState = await this.projectStateManager.getProjectStateByPath(projectPath);
      if (projectState) {
        return true;
      }
      
      // 如果通过状态管理器没有找到，再检查是否有项目ID
      const projectId = this.projectStateManager.getProjectId(projectPath);
      return !!projectId;
    } catch {
      return false;
    }
  }
  
  /**
   * 获取所有已索引的项目
   */
  private async getIndexedProjects(): Promise<string[]> {
    try {
      // 从ProjectStateManager获取所有已知项目
      return await this.projectStateManager.getAllProjects();
    } catch (error) {
      this.logger.error('Failed to get indexed projects:', error);
      return [];
    }
  }
  
  /**
   * 清除重启状态文件
   */
  async clearRestartState(): Promise<void> {
    try {
      await fs.unlink(this.RESTART_STATE_FILE).catch(() => {});
      this.logger.info('Hot reload restart state cleared');
    } catch (error) {
      this.logger.warn(`Failed to clear restart state: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  
  /**
   * JSON序列化替换函数，用于处理Date对象
   */
  private replacer(key: string, value: any): any {
    if (value instanceof Date) {
      return { __type: 'Date', value: value.toISOString() };
    }
    return value;
  }
  
  /**
   * 获取当前重启状态
   */
  getRestartState(): RestartState {
    return { ...this.restartState };
  }
  
  /**
   * 手动触发恢复过程 - 公共接口，供其他服务在检测到问题时调用
   */
  async triggerRecovery(recoveryType: 'basic' | 'advanced' = 'basic', context?: any): Promise<void> {
    this.logger.info('Manual recovery triggered', { recoveryType, context });
    
    try {
      if (recoveryType === 'basic') {
        await this.performBasicRecovery();
      } else {
        await this.performAdvancedRecovery(context);
      }
      
      this.logger.info('Manual recovery completed successfully', { recoveryType });
    } catch (error) {
      this.logger.error(`Manual recovery failed for type ${recoveryType}:`, error);
      
      this.errorHandler.handleError(
        new Error(`Manual recovery failed: ${error instanceof Error ? error.message : String(error)}`),
        {
          component: 'HotReloadRestartService',
          operation: 'triggerRecovery',
          recoveryType,
          context
        }
      );
      
      // 如果手动恢复失败，尝试基本恢复作为后备
      if (recoveryType !== 'basic') {
        try {
          await this.performBasicRecovery();
          this.logger.info('Fallback basic recovery after manual recovery failure completed');
        } catch (fallbackError) {
          this.logger.error('Fallback basic recovery also failed:', fallbackError);
        }
      }
    }
  }
  
  /**
   * 检查热更新功能的健康状态
   */
  async checkHealth(): Promise<{
    isHealthy: boolean;
    details: {
      changeDetectionService: boolean;
      projectHotReloadService: boolean;
      indexedProjectsCount: number;
      watchedProjectsCount: number;
      lastError?: string;
    }
  }> {
    try {
      // 检查变更检测服务是否运行
      const isChangeDetectionRunning = this.changeDetectionService.isServiceRunning();
      
      // 检查项目热更新服务状态
      const allStatuses = this.projectHotReloadService.getAllProjectStatuses();
      const watchedProjectsCount = Array.from(allStatuses.entries())
        .filter(([_, status]) => status.enabled && status.isWatching)
        .length;
      
      // 获取已索引的项目数量
      const indexedProjectsCount = (await this.getIndexedProjects()).length;
      
      // 检查是否有最近的错误
      const hasRecentError = !!this.restartState.error;
      
      const isHealthy = isChangeDetectionRunning &&
                       watchedProjectsCount > 0 &&
                       indexedProjectsCount >= watchedProjectsCount &&
                       !hasRecentError;
      
      return {
        isHealthy,
        details: {
          changeDetectionService: isChangeDetectionRunning,
          projectHotReloadService: allStatuses.size > 0,
          indexedProjectsCount,
          watchedProjectsCount,
          lastError: this.restartState.error ? String(this.restartState.error) : undefined
        }
      };
    } catch (error) {
      this.logger.error('Health check failed:', error);
      
      return {
        isHealthy: false,
        details: {
          changeDetectionService: false,
          projectHotReloadService: false,
          indexedProjectsCount: 0,
          watchedProjectsCount: 0,
          lastError: error instanceof Error ? error.message : String(error)
        }
      };
    }
  }
  
  /**
   * 获取热更新统计信息
   */
  async getHotReloadStats(): Promise<{
    totalIndexedProjects: number;
    totalWatchedProjects: number;
    restartCount: number;
    lastRestartTime?: Date;
    recoveryAttempts: number;
  }> {
    try {
      const indexedProjects = await this.getIndexedProjects();
      const allStatuses = this.projectHotReloadService.getAllProjectStatuses();
      const watchedProjects = Array.from(allStatuses.entries())
        .filter(([_, status]) => status.enabled && status.isWatching);
      
      return {
        totalIndexedProjects: indexedProjects.length,
        totalWatchedProjects: watchedProjects.length,
        restartCount: 1, // 这是当前重启周期，实际计数需要持久化
        lastRestartTime: this.restartState.timestamp,
        recoveryAttempts: this.restartState.phase === 'failed' ? 1 : 0
      };
    } catch (error) {
      this.logger.error('Failed to get hot reload stats:', error);
      
      return {
        totalIndexedProjects: 0,
        totalWatchedProjects: 0,
        restartCount: 0,
        recoveryAttempts: 0
      };
    }
  }
}