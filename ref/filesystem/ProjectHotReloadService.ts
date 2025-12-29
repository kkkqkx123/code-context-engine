import { injectable, inject } from 'inversify';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { TYPES } from '../../types';
import { ChangeDetectionService } from './ChangeDetectionService';
import { HotReloadError, HotReloadErrorCode } from './HotReloadError';
import { FileChangeEvent } from './ChangeDetectionService';
import { HotReloadMonitoringService } from './HotReloadMonitoringService';
import { HotReloadErrorPersistenceService } from './HotReloadErrorPersistenceService';
import { HotReloadConfigService } from './HotReloadConfigService';
import { HotReloadConfigFactory } from '../../config/factories/HotReloadConfigFactory';

export interface ProjectHotReloadConfig {
  enabled: boolean;
  debounceInterval: number;
  watchPatterns: string[];
  ignorePatterns: string[];
  maxFileSize: number;
  errorHandling: {
    maxRetries: number;
    alertThreshold: number;
    autoRecovery: boolean;
  };
}

export interface HotReloadStatus {
  enabled: boolean;
  isWatching: boolean;
  watchedPaths: string[];
  lastChange: Date | null;
  changesCount: number;
  errorsCount: number;
  lastError: Date | null;
}

export interface HotReloadMetrics {
  filesProcessed: number;
 changesDetected: number;
  averageProcessingTime: number;
 lastUpdated: Date;
  errorCount: number;
  errorBreakdown: Record<string, number>; // 按错误类型分类
  recoveryStats: {
    autoRecovered: number;
    manualIntervention: number;
    failedRecoveries: number;
  };
}

@injectable()
export class ProjectHotReloadService {
  private projectConfigs: Map<string, ProjectHotReloadConfig> = new Map();
  private activeWatchers: Map<string, boolean> = new Map();
 private projectStatus: Map<string, HotReloadStatus> = new Map();
  private projectMetrics: Map<string, HotReloadMetrics> = new Map();
  
  constructor(
    @inject(TYPES.ChangeDetectionService) private changeDetectionService: ChangeDetectionService,
    @inject(TYPES.HotReloadMonitoringService) private monitoringService: HotReloadMonitoringService,
    @inject(TYPES.HotReloadErrorPersistenceService) private errorPersistenceService: HotReloadErrorPersistenceService,
    @inject(TYPES.HotReloadConfigService) private configService: HotReloadConfigService,
    @inject(TYPES.LoggerService) private logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) private errorHandler: ErrorHandlerService
  ) {
    this.setupChangeDetectionCallbacks();
  }
  
  private setupChangeDetectionCallbacks(): void {
    this.changeDetectionService.setCallbacks({
      onFileCreated: async (event: FileChangeEvent) => {
        await this.handleFileChange(event);
      },
      onFileModified: async (event: FileChangeEvent) => {
        await this.handleFileChange(event);
      },
      onFileDeleted: async (event: FileChangeEvent) => {
        await this.handleFileChange(event);
      },
      onError: (error: Error) => {
        this.handleError(error);
      }
    });
  }
  
  private async handleFileChange(event: FileChangeEvent): Promise<void> {
    try {
      // 更新指标
      const projectPath = this.getProjectPathFromEvent(event);
      if (projectPath) {
        const metrics = this.getProjectMetrics(projectPath);
        metrics.changesDetected++;
        metrics.lastUpdated = new Date();
        
        // 更新状态
        const status = this.getProjectStatus(projectPath);
        status.lastChange = new Date();
        status.changesCount++;
      }
      
      this.logger.debug(`File change detected: ${event.type} - ${event.path}`);
      
      // 记录到监控服务
      if (projectPath) {
        this.monitoringService.recordChangeDetected(projectPath);
      }
    } catch (error) {
      this.handleError(error instanceof Error ? error : new Error(String(error)));
    }
  }
  
  private handleError(error: Error): void {
    // 更新错误计数
    // 这里可以实现更复杂的错误处理逻辑
    this.logger.error('Hot reload error occurred', error);
    
    // 如果是HotReloadError，记录到持久化服务
    if (error instanceof HotReloadError) {
      const errorReport: any = { // 使用any以匹配HotReloadErrorReport结构
        id: `hotreload_err_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
        timestamp: new Date(),
        projectId: 'unknown', // 实际实现中应从上下文获取项目ID
        errorCode: error.code,
        message: error.message,
        stack: error.stack,
        context: error.context || {},
        resolved: false
      };
      
      this.errorPersistenceService.queueError(errorReport);
    }
  }
  
  private getProjectPathFromEvent(event: FileChangeEvent): string | null {
    // 简化的项目路径提取逻辑
    // 实际实现可能需要更复杂的路径映射
    const possibleProjectPaths = Array.from(this.projectConfigs.keys());
    for (const projectPath of possibleProjectPaths) {
      if (event.path.startsWith(projectPath)) {
        return projectPath;
      }
    }
    return null;
  }
  
  async enableForProject(projectPath: string, config?: Partial<ProjectHotReloadConfig>): Promise<void> {
    try {
      // 使用配置工厂获取默认配置，避免硬编码
      const globalConfig = this.configService.getGlobalConfig();
      const defaultConfig = HotReloadConfigFactory.createDefaultProjectConfig(globalConfig);
      
      const finalConfig: ProjectHotReloadConfig = {
        ...defaultConfig,
        ...config,
        errorHandling: {
          ...defaultConfig.errorHandling,
          ...(config?.errorHandling || {})
        }
      };
      
      this.projectConfigs.set(projectPath, finalConfig);
      
      if (finalConfig.enabled) {
        // 初始化变更检测服务
        await this.changeDetectionService.initialize([projectPath], {
          watchPaths: [projectPath],
          debounceInterval: finalConfig.debounceInterval,
          enableHashComparison: true,
          trackFileHistory: true,
        });
        
        this.activeWatchers.set(projectPath, true);
        
        // 更新项目状态
        this.projectStatus.set(projectPath, {
          enabled: true,
          isWatching: true,
          watchedPaths: [projectPath],
          lastChange: null,
          changesCount: 0,
          errorsCount: 0,
          lastError: null
        });
        
        // 初始化项目指标
        const initialMetrics = this.monitoringService.getProjectMetrics(projectPath);
        this.projectMetrics.set(projectPath, initialMetrics);
        
        this.logger.info(`Hot reload enabled for project: ${projectPath}`);
      }
    } catch (error) {
      const hotReloadError = new HotReloadError(
        HotReloadErrorCode.CHANGE_DETECTION_FAILED,
        `Failed to enable hot reload for project ${projectPath}: ${error instanceof Error ? error.message : String(error)}`
      );
      
      this.errorHandler.handleError(
        hotReloadError,
        { component: 'ProjectHotReloadService', operation: 'enableForProject', projectPath }
      );
      
      // 记录错误到持久化服务
      const errorReport: any = {
        id: `hotreload_err_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
        timestamp: new Date(),
        projectId: projectPath,
        errorCode: hotReloadError.code,
        message: hotReloadError.message,
        stack: hotReloadError.stack,
        context: hotReloadError.context || {},
        resolved: false
      };
      
      await this.errorPersistenceService.persistError(errorReport);
      
      throw hotReloadError;
    }
  }
  
  async disableForProject(projectPath: string): Promise<void> {
    try {
      this.projectConfigs.set(projectPath, {
        ...this.getProjectConfig(projectPath),
        enabled: false
      });
      
      // 停止监视
      if (this.activeWatchers.get(projectPath)) {
        // 注意：这里不直接停止ChangeDetectionService，因为它可能还在监视其他项目
        // 实际实现中需要更复杂的逻辑来管理多个项目的共享服务
        this.activeWatchers.set(projectPath, false);
        
        const status = this.getProjectStatus(projectPath);
        status.enabled = false;
        status.isWatching = false;
      }
      
      this.logger.info(`Hot reload disabled for project: ${projectPath}`);
    } catch (error) {
      const hotReloadError = new HotReloadError(
        HotReloadErrorCode.CHANGE_DETECTION_FAILED,
        `Failed to disable hot reload for project ${projectPath}: ${error instanceof Error ? error.message : String(error)}`
      );
      
      this.errorHandler.handleError(
        hotReloadError,
        { component: 'ProjectHotReloadService', operation: 'disableForProject', projectPath }
      );
      
      throw hotReloadError;
    }
 }
  
  getProjectConfig(projectPath: string): ProjectHotReloadConfig {
    const config = this.projectConfigs.get(projectPath);
    if (!config) {
      // 使用配置工厂获取默认配置，避免硬编码
      const globalConfig = this.configService.getGlobalConfig();
      const defaultConfig = HotReloadConfigFactory.createDefaultProjectConfig(globalConfig);
      return {
        ...defaultConfig,
        enabled: false // 默认情况下项目配置是禁用的
      };
    }
    return config;
  }
  
 getProjectStatus(projectPath: string): HotReloadStatus {
    const status = this.projectStatus.get(projectPath);
    if (!status) {
      // 返回默认状态
      return {
        enabled: false,
        isWatching: false,
        watchedPaths: [],
        lastChange: null,
        changesCount: 0,
        errorsCount: 0,
        lastError: null
      };
    }
    return status;
  }
  
  getProjectMetrics(projectPath: string): HotReloadMetrics {
    const metrics = this.projectMetrics.get(projectPath);
    if (!metrics) {
      // 返回默认指标
      return {
        filesProcessed: 0,
        changesDetected: 0,
        averageProcessingTime: 0,
        lastUpdated: new Date(),
        errorCount: 0,
        errorBreakdown: {},
        recoveryStats: {
          autoRecovered: 0,
          manualIntervention: 0,
          failedRecoveries: 0
        }
      };
    }
    return metrics;
  }
  
  async updateConfig(projectPath: string, config: Partial<ProjectHotReloadConfig>): Promise<void> {
    const currentConfig = this.getProjectConfig(projectPath);
    const updatedConfig: ProjectHotReloadConfig = {
      ...currentConfig,
      ...config,
      errorHandling: {
        ...currentConfig.errorHandling,
        ...(config.errorHandling || {})
      }
    };
    
    this.projectConfigs.set(projectPath, updatedConfig);
    
    // 如果服务已经在运行，可能需要重启以应用新配置
    if (this.activeWatchers.get(projectPath)) {
      // 这里可以重新初始化变更检测服务以应用新配置
      // 但为了简单起见，我们记录这个需求
      this.logger.info(`Config updated for project ${projectPath}. Restart hot reload to apply changes.`);
    }
  }
  
  isProjectHotReloadEnabled(projectPath: string): boolean {
    return this.getProjectConfig(projectPath).enabled;
  }
  
  getAllProjectStatuses(): Map<string, HotReloadStatus> {
    return new Map(this.projectStatus);
  }
}