import { injectable, inject } from 'inversify';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { TYPES } from '../../types';
import { ProjectHotReloadConfig } from './ProjectHotReloadService';
import { HotReloadConfig as GlobalHotReloadConfig } from '../../config/ConfigTypes';
import { HotReloadConfigFactory } from '../../config/factories/HotReloadConfigFactory';

export interface HotReloadGlobalConfig {
  enabled: boolean;
  defaultDebounceInterval: number;
  defaultWatchPatterns: string[];
  defaultIgnorePatterns: string[];
  defaultMaxFileSize: number;
  defaultErrorHandling: {
    maxRetries: number;
    alertThreshold: number;
    autoRecovery: boolean;
  };
  enableDetailedLogging: boolean;
  maxConcurrentProjects: number;
}

@injectable()
export class HotReloadConfigService {
  private globalConfig: HotReloadGlobalConfig;
  private projectConfigs: Map<string, Partial<ProjectHotReloadConfig>> = new Map();
  private readonly configFilePath: string;
  
  constructor(
    @inject(TYPES.LoggerService) private logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) private errorHandler: ErrorHandlerService
  ) {
    this.configFilePath = './hotreload.config.json';
    this.globalConfig = this.getDefaultGlobalConfig();
  }
  
  private getDefaultGlobalConfig(): HotReloadGlobalConfig {
    // 使用配置工厂获取默认配置，避免硬编码和循环依赖
    return HotReloadConfigFactory.createDefaultGlobalConfig();
  }
  
  /**
   * 获取全局热更新配置
   */
  getGlobalConfig(): HotReloadGlobalConfig {
    return { ...this.globalConfig };
  }
  
  /**
   * 更新全局热更新配置
   */
  updateGlobalConfig(config: Partial<HotReloadGlobalConfig>): void {
    this.globalConfig = {
      ...this.globalConfig,
      ...config,
      defaultErrorHandling: {
        ...this.globalConfig.defaultErrorHandling,
        ...(config.defaultErrorHandling || {})
      }
    };
    
    this.logger.info('Global hot reload configuration updated', { config: this.globalConfig });
  }
  
  /**
   * 获取项目特定的热更新配置
   */
 getProjectConfig(projectPath: string): ProjectHotReloadConfig {
    const projectSpecificConfig = this.projectConfigs.get(projectPath) || {};
    
    // 合并全局默认配置和项目特定配置
    return {
      enabled: projectSpecificConfig.enabled ?? this.globalConfig.enabled,
      debounceInterval: projectSpecificConfig.debounceInterval ?? this.globalConfig.defaultDebounceInterval,
      watchPatterns: projectSpecificConfig.watchPatterns ?? this.globalConfig.defaultWatchPatterns,
      ignorePatterns: projectSpecificConfig.ignorePatterns ?? this.globalConfig.defaultIgnorePatterns,
      maxFileSize: projectSpecificConfig.maxFileSize ?? this.globalConfig.defaultMaxFileSize,
      errorHandling: {
        ...this.globalConfig.defaultErrorHandling,
        ...(projectSpecificConfig.errorHandling || {})
      }
    };
  }

  /**
   * 从项目状态获取热更新配置
   */
  getProjectConfigFromState(projectState: any): ProjectHotReloadConfig {
    if (!projectState?.hotReload) {
      return this.getProjectConfig(projectState?.projectPath || '');
    }

    const hotReload = projectState.hotReload;
    return {
      enabled: hotReload.enabled ?? this.globalConfig.enabled,
      debounceInterval: hotReload.config?.debounceInterval ?? this.globalConfig.defaultDebounceInterval,
      watchPatterns: hotReload.config?.watchPatterns ?? this.globalConfig.defaultWatchPatterns,
      ignorePatterns: hotReload.config?.ignorePatterns ?? this.globalConfig.defaultIgnorePatterns,
      maxFileSize: hotReload.config?.maxFileSize ?? this.globalConfig.defaultMaxFileSize,
      errorHandling: {
        ...this.globalConfig.defaultErrorHandling,
        ...(hotReload.config?.errorHandling || {})
      }
    };
  }

  /**
   * 更新项目状态中的热更新配置
   */
  updateProjectStateConfig(projectState: any, config: Partial<ProjectHotReloadConfig>): void {
    if (!projectState) return;

    if (!projectState.hotReload) {
      projectState.hotReload = {
        enabled: false,
        config: {},
        changesDetected: 0,
        errorsCount: 0
      };
    }

    // 更新配置
    if (config.enabled !== undefined) {
      const oldEnabled = projectState.hotReload.enabled;
      projectState.hotReload.enabled = config.enabled;
      
      // 记录启用/禁用时间
      if (oldEnabled !== config.enabled) {
        if (config.enabled === true) {
          projectState.hotReload.lastEnabled = new Date();
        } else if (config.enabled === false) {
          projectState.hotReload.lastDisabled = new Date();
        }
      }
    }

    if (config.debounceInterval !== undefined) {
      projectState.hotReload.config.debounceInterval = config.debounceInterval;
    }

    if (config.watchPatterns !== undefined) {
      projectState.hotReload.config.watchPatterns = config.watchPatterns;
    }

    if (config.ignorePatterns !== undefined) {
      projectState.hotReload.config.ignorePatterns = config.ignorePatterns;
    }

    if (config.maxFileSize !== undefined) {
      projectState.hotReload.config.maxFileSize = config.maxFileSize;
    }

    if (config.errorHandling !== undefined) {
      projectState.hotReload.config.errorHandling = {
        ...projectState.hotReload.config.errorHandling,
        ...config.errorHandling
      };
    }
  }
  
  /**
   * 设置项目特定的热更新配置
   */
  setProjectConfig(projectPath: string, config: Partial<ProjectHotReloadConfig>): void {
    const existingConfig = this.projectConfigs.get(projectPath) || {};
    const newConfig = { ...existingConfig, ...config };
    
    this.projectConfigs.set(projectPath, newConfig);
    
    this.logger.info(`Project hot reload configuration updated for ${projectPath}`, { config: newConfig });
  }
  
  /**
   * 删除项目配置（恢复为全局默认值）
   */
  removeProjectConfig(projectPath: string): void {
    this.projectConfigs.delete(projectPath);
    this.logger.info(`Project hot reload configuration removed for ${projectPath}`);
  }
  
  /**
   * 获取所有项目的配置
   */
  getAllProjectConfigs(): Map<string, Partial<ProjectHotReloadConfig>> {
    return new Map(this.projectConfigs);
  }
  
  /**
   * 检查热更新是否全局启用
   */
  isGloballyEnabled(): boolean {
    return this.globalConfig.enabled;
  }
  
  /**
   * 检查项目是否启用了热更新
   */
  isProjectEnabled(projectPath: string): boolean {
    return this.getProjectConfig(projectPath).enabled;
  }
  
  /**
   * 验证配置是否有效
   */
  validateConfig(config: Partial<ProjectHotReloadConfig>): { isValid: boolean; errors: string[] } {
    const errors: string[] = [];
    
    if (config.debounceInterval !== undefined && config.debounceInterval < 50) {
      errors.push('Debounce interval must be at least 50ms');
    }
    
    if (config.maxFileSize !== undefined && (config.maxFileSize <= 0 || config.maxFileSize > 100 * 1024 * 1024)) {
      errors.push('Max file size must be between 1 byte and 100MB');
    }
    
    if (config.errorHandling?.maxRetries !== undefined && config.errorHandling.maxRetries < 0) {
      errors.push('Max retries must be non-negative');
    }
    
    return {
      isValid: errors.length === 0,
      errors
    };
  }

  /**
   * 验证项目状态热更新配置
   */
  validateProjectStateConfig(config: any): { isValid: boolean; errors: string[] } {
    const errors: string[] = [];
    
    if (config.enabled !== undefined && typeof config.enabled !== 'boolean') {
      errors.push('Enabled must be a boolean');
    }
    
    if (config.config?.debounceInterval !== undefined && (config.config.debounceInterval < 50 || config.config.debounceInterval > 10000)) {
      errors.push('Debounce interval must be between 50ms and 10000ms');
    }
    
    if (config.config?.maxFileSize !== undefined && (config.config.maxFileSize <= 0 || config.config.maxFileSize > 100 * 1024 * 1024)) {
      errors.push('Max file size must be between 1 byte and 100MB');
    }
    
    if (config.config?.errorHandling?.maxRetries !== undefined && config.config.errorHandling.maxRetries < 0) {
      errors.push('Max retries must be non-negative');
    }
    
    if (config.config?.watchPatterns !== undefined && !Array.isArray(config.config.watchPatterns)) {
      errors.push('Watch patterns must be an array');
    }
    
    if (config.config?.ignorePatterns !== undefined && !Array.isArray(config.config.ignorePatterns)) {
      errors.push('Ignore patterns must be an array');
    }
    
    return {
      isValid: errors.length === 0,
      errors
    };
  }

  /**
   * 获取项目热更新状态信息
   */
  getProjectHotReloadStatus(projectState: any): {
    enabled: boolean;
    lastEnabled?: Date;
    lastDisabled?: Date;
    changesDetected: number;
    errorsCount: number;
  } {
    if (!projectState?.hotReload) {
      return {
        enabled: false,
        changesDetected: 0,
        errorsCount: 0
      };
    }

    return {
      enabled: projectState.hotReload.enabled,
      lastEnabled: projectState.hotReload.lastEnabled,
      lastDisabled: projectState.hotReload.lastDisabled,
      changesDetected: projectState.hotReload.changesDetected || 0,
      errorsCount: projectState.hotReload.errorsCount || 0
    };
  }

  /**
   * 重置项目热更新统计信息
   */
  resetProjectStats(projectState: any): void {
    if (projectState?.hotReload) {
      projectState.hotReload.changesDetected = 0;
      projectState.hotReload.errorsCount = 0;
    }
  }
  
  /**
   * 重置为默认配置
   */
  resetToDefaults(): void {
    this.globalConfig = this.getDefaultGlobalConfig();
    this.projectConfigs.clear();
    this.logger.info('Hot reload configuration reset to defaults');
  }
  
  /**
   * 从JSON文件加载配置
   */
  async loadFromFile(filePath?: string): Promise<void> {
    const path = filePath || this.configFilePath;
    try {
      const fs = await import('fs/promises');
      const configContent = await fs.readFile(path, 'utf-8');
      const config = JSON.parse(configContent);
      
      if (config.global) {
        this.updateGlobalConfig(config.global);
      }
      
      if (config.projects) {
        for (const [projectPath, projectConfig] of Object.entries(config.projects)) {
          this.setProjectConfig(projectPath, projectConfig as Partial<ProjectHotReloadConfig>);
        }
      }
      
      this.logger.info(`Hot reload configuration loaded from ${path}`);
    } catch (error) {
      if ((error as any).code === 'ENOENT') {
        this.logger.info(`Configuration file not found at ${path}, using defaults`);
      } else {
        this.errorHandler.handleError(
          new Error(`Failed to load hot reload configuration: ${error instanceof Error ? error.message : String(error)}`),
          { component: 'HotReloadConfigService', operation: 'loadFromFile', filePath: path }
        );
      }
    }
  }
  
  /**
   * 保存配置到JSON文件
   */
  async saveToFile(filePath?: string): Promise<void> {
    const path = filePath || this.configFilePath;
    try {
      const fs = await import('fs/promises');
      const configToSave = {
        global: this.globalConfig,
        projects: Object.fromEntries(this.projectConfigs)
      };
      
      await fs.writeFile(path, JSON.stringify(configToSave, null, 2), 'utf-8');
      this.logger.info(`Hot reload configuration saved to ${path}`);
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to save hot reload configuration: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'HotReloadConfigService', operation: 'saveToFile', filePath: path }
      );
    }
  }
}