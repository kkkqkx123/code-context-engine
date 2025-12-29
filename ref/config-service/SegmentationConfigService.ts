import { injectable } from 'inversify';
import { BaseConfigService } from './BaseConfigService';
import { SegmentationConfig, SegmentationMode, EnvironmentUtils } from '../ConfigTypes';
import { SegmentationPresetFactory } from '../presets/SegmentationPresetFactory';
import { segmentationSchema } from '../validation/ConfigValidation';
import { ValidationUtils } from '../utils/ValidationUtils';
import { UniversalChunkingOptions } from '../../service/parser/processing/strategies/types/SegmentationTypes';

/**
 * 分段配置服务
 * 负责管理和提供代码分段相关的配置
 */
@injectable()
export class SegmentationConfigService extends BaseConfigService<SegmentationConfig> {
  
  /**
  * 加载配置
  * 从环境变量读取配置模式，创建对应的配置对象
  * @returns 分段配置对象
  */
  public loadConfig(): SegmentationConfig {
    const mode = EnvironmentUtils.parseString(
      'SEGMENTATION_MODE', 
      SegmentationMode.DEFAULT
    ) as SegmentationMode;

    // 1. 从工厂创建基础配置
    let config = SegmentationPresetFactory.create(mode);

    // 2. 应用环境变量覆盖
    config = this.applyEnvironmentOverrides(config);

    // 3. 验证最终配置
    return this.validateConfig(config);
  }

  /**
  * 验证配置
  * 使用 Joi 验证模式验证配置对象
  * @param config 待验证的配置对象
  * @returns 验证后的配置对象
  */
  public validateConfig(config: SegmentationConfig): SegmentationConfig {
    return ValidationUtils.validateConfig(config, segmentationSchema);
  }

  /**
   * 获取默认配置
   * @returns 默认的分段配置对象
   */
  public getDefaultConfig(): SegmentationConfig {
    return SegmentationPresetFactory.create(SegmentationMode.DEFAULT);
  }

  /**
   * 应用环境变量覆盖
   * 允许通过环境变量微调配置
   * @param config 基础配置对象
   * @returns 应用覆盖后的配置对象
   */
  private applyEnvironmentOverrides(config: SegmentationConfig): SegmentationConfig {
    // 全局设置覆盖
    if (process.env.SEGMENTATION_MIN_CHUNK_SIZE) {
      config.global.minChunkSize = parseInt(process.env.SEGMENTATION_MIN_CHUNK_SIZE, 10);
    }
    
    if (process.env.SEGMENTATION_MAX_CHUNK_SIZE) {
      config.global.maxChunkSize = parseInt(process.env.SEGMENTATION_MAX_CHUNK_SIZE, 10);
    }
    
    if (process.env.SEGMENTATION_CHUNK_OVERLAP) {
      config.global.chunkOverlap = parseInt(process.env.SEGMENTATION_CHUNK_OVERLAP, 10);
    }

    // 性能设置覆盖
    if (process.env.SEGMENTATION_MAX_FILE_SIZE) {
      config.performance.maxFileSize = parseInt(process.env.SEGMENTATION_MAX_FILE_SIZE, 10);
    }
    
    if (process.env.SEGMENTATION_MAX_PARSE_TIME) {
      config.performance.maxParseTime = parseInt(process.env.SEGMENTATION_MAX_PARSE_TIME, 10);
    }
    
    if (process.env.SEGMENTATION_ENABLE_CACHING) {
      config.performance.enableCaching = process.env.SEGMENTATION_ENABLE_CACHING === 'true';
    }
    
    if (process.env.SEGMENTATION_MAX_CACHE_SIZE) {
      config.performance.maxCacheSize = parseInt(process.env.SEGMENTATION_MAX_CACHE_SIZE, 10);
    }
    
    if (process.env.SEGMENTATION_ENABLE_PARALLEL) {
      config.performance.enableParallel = process.env.SEGMENTATION_ENABLE_PARALLEL === 'true';
    }
    
    if (process.env.SEGMENTATION_PARALLEL_THREADS) {
      config.performance.parallelThreads = parseInt(process.env.SEGMENTATION_PARALLEL_THREADS, 10);
    }

    // 嵌套设置覆盖
    if (process.env.SEGMENTATION_ENABLE_NESTED_EXTRACTION) {
      config.nesting.enableNestedExtraction = process.env.SEGMENTATION_ENABLE_NESTED_EXTRACTION === 'true';
    }
    
    if (process.env.SEGMENTATION_MAX_NESTING_LEVEL) {
      config.nesting.maxNestingLevel = parseInt(process.env.SEGMENTATION_MAX_NESTING_LEVEL, 10);
    }

    // 降级策略覆盖
    if (process.env.SEGMENTATION_ENABLE_FALLBACK) {
      config.fallback.enableFallback = process.env.SEGMENTATION_ENABLE_FALLBACK === 'true';
    }
    
    if (process.env.SEGMENTATION_FALLBACK_THRESHOLD) {
      config.fallback.fallbackThreshold = parseFloat(process.env.SEGMENTATION_FALLBACK_THRESHOLD);
    }
    
    if (process.env.SEGMENTATION_FALLBACK_STRATEGIES) {
      config.fallback.strategies = process.env.SEGMENTATION_FALLBACK_STRATEGIES.split(',').map(s => s.trim());
    }

    return config;
  }

  /**
   * 获取语言特定配置
   * @param language 语言名称
   * @returns 语言特定配置，如果不存在则返回默认配置
   */
  public getLanguageSpecificConfig(language: string): SegmentationConfig['languageSpecific'][string] {
    const config = this.getConfig();
    return config.languageSpecific[language] || this.getDefaultLanguageConfig();
  }

  /**
   * 获取默认语言配置
   * @returns 默认的语言配置
   */
  private getDefaultLanguageConfig(): SegmentationConfig['languageSpecific'][string] {
    return {
      maxChunkSize: 2000,
      minChunkSize: 200,
      maxNestingLevel: 10,
      preserveComments: true,
      preserveEmptyLines: false,
    };
  }

  /**
   * 更新语言特定配置
   * @param language 语言名称
   * @param updates 配置更新
   */
  public updateLanguageSpecificConfig(language: string, updates: Partial<SegmentationConfig['languageSpecific'][string]>): void {
    const config = this.getConfig();
    const currentLangConfig = config.languageSpecific[language] || this.getDefaultLanguageConfig();
    config.languageSpecific[language] = { ...currentLangConfig, ...updates };
    this.updateConfig(config);
  }

  /**
   * 获取当前配置模式
   * @returns 当前配置模式
   */
  public getCurrentMode(): SegmentationMode {
    return EnvironmentUtils.parseString(
      'SEGMENTATION_MODE', 
      SegmentationMode.DEFAULT
    ) as SegmentationMode;
  }

  /**
   * 重新加载配置
   * 强制重新从环境变量加载配置
   */
  public reloadConfig(): void {
    // 重置初始化标志，强制重新加载
    (this as any)._isInitialized = false;
    (this as any)._config = null;
  }

  /**
   * 获取默认选项
   * @returns 默认的通用分块选项
   */
  public getDefaultOptions(): UniversalChunkingOptions {
    const config = this.getDefaultConfig();
    return this.convertToUniversalChunkingOptions(config);
  }

  /**
   * 合并选项
   * @param baseOptions 基础选项
   * @param overrides 覆盖选项
   * @returns 合并后的选项
   */
  public mergeOptions(baseOptions: UniversalChunkingOptions, overrides: Partial<UniversalChunkingOptions>): UniversalChunkingOptions {
    return {
      ...baseOptions,
      ...overrides,
      advanced: {
        ...baseOptions.advanced,
        ...overrides.advanced,
      },
      customParams: {
        ...baseOptions.customParams,
        ...overrides.customParams,
      },
      languageConfig: {
        ...baseOptions.languageConfig,
        ...overrides.languageConfig,
      },
    };
  }

  /**
   * 将 SegmentationConfig 转换为 UniversalChunkingOptions
   * @param config SegmentationConfig 对象
   * @returns UniversalChunkingOptions 对象
   */
  private convertToUniversalChunkingOptions(config: SegmentationConfig): UniversalChunkingOptions {
    return {
      maxChunkSize: config.global.maxChunkSize,
      minChunkSize: config.global.minChunkSize,
      overlapSize: config.global.chunkOverlap,
      maxLinesPerChunk: config.global.maxLinesPerChunk,
      minLinesPerChunk: config.global.minLinesPerChunk,
      enableIntelligentChunking: true,
      memoryLimitMB: config.performance.maxFileSize / (1024 * 1024),
      errorThreshold: 0.1,
      advanced: {
        semanticWeight: 0.4,
        syntacticWeight: 0.4,
        structuralWeight: 0.2,
        enableChunkDeduplication: true,
        deduplicationThreshold: 0.8,
        astNodeTracking: config.nesting.enableNestedExtraction,
      },
      strategy: 'universal',
      languageConfig: Object.fromEntries(
        Object.entries(config.languageSpecific).map(([language, langConfig]) => [
          language,
          {
            maxChunkSize: langConfig.maxChunkSize,
            minChunkSize: langConfig.minChunkSize,
            overlapSize: config.global.chunkOverlap,
            maxLinesPerChunk: config.global.maxLinesPerChunk,
            minLinesPerChunk: config.global.minLinesPerChunk,
            enableIntelligentChunking: true,
            preserveComments: langConfig.preserveComments,
            preserveEmptyLines: langConfig.preserveEmptyLines,
          }
        ])
      ),
    };
  }
}