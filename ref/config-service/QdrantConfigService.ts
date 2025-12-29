import { injectable, inject } from 'inversify';
import * as Joi from 'joi';
import { BaseConfigService } from './BaseConfigService';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { TYPES } from '../../types';
import { EnvironmentUtils } from '../utils/EnvironmentUtils';
import { ValidationUtils } from '../utils/ValidationUtils';
import { HashUtils } from '../../utils/cache/HashUtils';
import { ProjectMappingService } from '../../database/ProjectMappingService';

export interface QdrantCacheConfig {
  defaultTTL: number;
  maxEntries: number;
  cleanupInterval: number;
  enableStats: boolean;
}

export interface QdrantPerformanceConfig {
  monitoringInterval: number;
  metricsRetentionPeriod: number;
  enableDetailedLogging: boolean;
  performanceThresholds: {
    queryExecutionTime: number;
    memoryUsage: number;
    responseTime: number;
  };
}

export interface QdrantBatchConfig {
  maxConcurrentOperations: number;
  defaultBatchSize: number;
  maxBatchSize: number;
  minBatchSize: number;
  memoryThreshold: number;
  processingTimeout: number;
  retryAttempts: number;
  retryDelay: number;
  adaptiveBatchingEnabled: boolean;
  performanceThreshold: number;
  adjustmentFactor: number;
}

export interface QdrantConfig {
  host: string;
  port: number;
  collection: string;
  apiKey?: string;
  useHttps: boolean;
  timeout: number;
  
  // 新增配置块
  cache?: QdrantCacheConfig;
  performance?: QdrantPerformanceConfig;
  batch?: QdrantBatchConfig;
}

@injectable()
export class QdrantConfigService extends BaseConfigService<QdrantConfig> {
  constructor(
    @inject(TYPES.LoggerService) private logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) private errorHandler: ErrorHandlerService,
    @inject(TYPES.UnifiedMappingService) private unifiedMappingService?: ProjectMappingService
  ) {
    super();
  }

  loadConfig(): QdrantConfig {
    try {
      const rawConfig = {
        host: EnvironmentUtils.parseString('QDRANT_HOST', 'localhost'),
        port: EnvironmentUtils.parsePort('QDRANT_PORT', 6333),
        collection: this.getCollectionName(), // 使用增强的配置逻辑
        apiKey: EnvironmentUtils.parseOptionalString('QDRANT_API_KEY'),
        useHttps: EnvironmentUtils.parseBoolean('QDRANT_USE_HTTPS', false),
        timeout: EnvironmentUtils.parseNumber('QDRANT_TIMEOUT', 30000),
        
        // 新增: 缓存配置
        cache: {
          defaultTTL: EnvironmentUtils.parseNumber('QDRANT_CACHE_TTL', 30000),
          maxEntries: EnvironmentUtils.parseNumber('QDRANT_CACHE_MAX_ENTRIES', 10000),
          cleanupInterval: EnvironmentUtils.parseNumber('QDRANT_CACHE_CLEANUP_INTERVAL', 60000),
          enableStats: EnvironmentUtils.parseBoolean('QDRANT_CACHE_STATS_ENABLED', true),
        },
        
        // 新增: 性能配置
        performance: {
          monitoringInterval: EnvironmentUtils.parseNumber('QDRANT_PERFORMANCE_INTERVAL', 30000),
          metricsRetentionPeriod: EnvironmentUtils.parseNumber('QDRANT_PERFORMANCE_RETENTION', 86400000),
          enableDetailedLogging: EnvironmentUtils.parseBoolean('QDRANT_PERFORMANCE_LOGGING_ENABLED', true),
          performanceThresholds: {
            queryExecutionTime: EnvironmentUtils.parseNumber('QDRANT_PERFORMANCE_QUERY_TIMEOUT', 5000),
            memoryUsage: EnvironmentUtils.parseNumber('QDRANT_PERFORMANCE_MEMORY_THRESHOLD', 80),
            responseTime: EnvironmentUtils.parseNumber('QDRANT_PERFORMANCE_RESPONSE_THRESHOLD', 500),
          },
        },
        
        // 新增: 批处理配置
        batch: {
          maxConcurrentOperations: EnvironmentUtils.parseNumber('QDRANT_BATCH_CONCURRENCY', 5),
          defaultBatchSize: EnvironmentUtils.parseNumber('QDRANT_BATCH_SIZE_DEFAULT', 50),
          maxBatchSize: EnvironmentUtils.parseNumber('QDRANT_BATCH_SIZE_MAX', 500),
          minBatchSize: EnvironmentUtils.parseNumber('QDRANT_BATCH_SIZE_MIN', 10),
          memoryThreshold: EnvironmentUtils.parseFloat('QDRANT_BATCH_MEMORY_THRESHOLD', 0.80),
          processingTimeout: EnvironmentUtils.parseNumber('QDRANT_BATCH_PROCESSING_TIMEOUT', 3000),
          retryAttempts: EnvironmentUtils.parseNumber('QDRANT_BATCH_RETRY_ATTEMPTS', 3),
          retryDelay: EnvironmentUtils.parseNumber('QDRANT_BATCH_RETRY_DELAY', 1000),
          adaptiveBatchingEnabled: EnvironmentUtils.parseBoolean('QDRANT_BATCH_ADAPTIVE_ENABLED', true),
          performanceThreshold: EnvironmentUtils.parseNumber('QDRANT_BATCH_PERFORMANCE_THRESHOLD', 1000),
          adjustmentFactor: EnvironmentUtils.parseFloat('QDRANT_BATCH_ADJUSTMENT_FACTOR', 0.1),
        },
      };

      return this.validateConfig(rawConfig);
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Unknown error in QdrantConfigService'),
        { component: 'QdrantConfigService', operation: 'loadConfig' }
      );
      throw error;
    }
  }

  /**
   * 获取集合名称，实现配置优先级逻辑
   * 1. 检查显式环境配置
   * 2. 否则使用默认值（项目隔离的动态命名在使用时确定）
   */
  private getCollectionName(): string {
    try {
      // 检查显式环境配置
      const explicitName = process.env.QDRANT_COLLECTION;
      if (explicitName && explicitName !== 'code-snippets') {
        // 显式设置的配置，记录警告日志
        this.logger.warn('Using explicit QDRANT_COLLECTION configuration, this will override project isolation');
        // 验证显式配置的命名是否符合规范
        if (!this.validateNamingConvention(explicitName)) {
          this.logger.warn(`Explicit collection name "${explicitName}" does not follow naming conventions, this may cause issues.`);
        }
        return explicitName;
      }

      // 默认使用项目隔离的动态命名（在实际使用时确定）
      return 'code-snippets'; // 占位符，实际使用时会被替换
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Unknown error in getCollectionName'),
        { component: 'QdrantConfigService', operation: 'getCollectionName' }
      );
      // 返回默认值以确保服务可用
      return 'code-snippets';
    }
  }

  /**
   * 验证命名约定是否符合数据库要求
   *
   * @param name 要验证的名称
   * @returns 如果名称符合约定则返回true，否则返回false
   */
  validateNamingConvention(name: string): boolean {
    try {
      // 验证命名符合数据库约束
      const pattern = /^[a-zA-Z0-9_-]{1,63}$/;
      const isValid = pattern.test(name) && !name.startsWith('_');

      if (!isValid) {
        this.logger.warn(`Invalid naming convention detected: "${name}" does not match pattern ${pattern}`);
      }

      return isValid;
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Unknown error in validateNamingConvention'),
        { component: 'QdrantConfigService', operation: 'validateNamingConvention' }
      );
      return false;
    }
  }

  /**
   * 获取集合名称，支持显式配置或动态生成（供外部使用）
   *
   * @param projectId 项目ID，用于生成动态集合名称
   * @returns 对应的集合名称，优先使用显式环境配置，否则使用项目隔离的动态命名
   * @throws 如果生成的名称不符合命名约定，则抛出错误
   */
  getCollectionNameForProject(projectId: string): string {
    try {
      // 1. 检查显式环境配置
      const explicitName = process.env.QDRANT_COLLECTION;
      if (explicitName && explicitName !== 'code-snippets') {
        this.logger.warn('Using explicit QDRANT_COLLECTION configuration, which may override project isolation');
        // 验证显式配置的命名是否符合规范
        if (!this.validateNamingConvention(explicitName)) {
          this.logger.error(`Explicit QDRANT_COLLECTION name "${explicitName}" does not follow naming conventions, this may cause issues.`);
        }
        return explicitName;
      }

      // 2. 使用项目隔离的动态命名，确保符合命名规范
      const dynamicName = HashUtils.generateSafeProjectName(
        projectId,
        'project',
        63,
        true,
        this.unifiedMappingService
      );

      // 验证动态生成的命名是否符合规范（应该总是通过，但作为双重检查）
      if (!this.validateNamingConvention(dynamicName)) {
        this.logger.error(`Generated collection name "${dynamicName}" does not follow naming conventions.`);
        throw new Error(`Generated collection name "${dynamicName}" is invalid`);
      }

      return dynamicName;
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Unknown error in getCollectionNameForProject'),
        { component: 'QdrantConfigService', operation: 'getCollectionNameForProject', projectId }
      );
      throw error;
    }
  }

  /**
   * 检查配置是否冲突
   * @param explicitName 显式配置的名称
   * @param projectId 项目ID
   * @returns 是否存在冲突
   */
  public static checkConfigurationConflict(explicitName: string | undefined, projectId: string): boolean {
    // 如果没有显式配置，则无冲突
    if (!explicitName) {
      return false;
    }

    // 检查显式配置是否与项目隔离命名冲突
    const projectSpecificName = `project-${projectId}`;
    return explicitName !== projectSpecificName;
  }

  validateConfig(config: any): QdrantConfig {
    try {
      const schema = Joi.object({
        host: Joi.string().hostname().default('localhost'),
        port: ValidationUtils.portSchema(6333),
        collection: Joi.string().default('code-snippets'),
        apiKey: Joi.string().optional(),
        useHttps: ValidationUtils.booleanSchema(false),
        timeout: ValidationUtils.positiveNumberSchema(30000),
        
        // 缓存配置验证
        cache: Joi.object({
          defaultTTL: Joi.number().min(1000).default(30000),
          maxEntries: Joi.number().min(100).default(10000),
          cleanupInterval: Joi.number().min(10000).default(60000),
          enableStats: Joi.boolean().default(true),
        }).optional(),
        
        // 性能配置验证
        performance: Joi.object({
          monitoringInterval: Joi.number().min(100).default(30000),
          metricsRetentionPeriod: Joi.number().min(1000).default(86400000),
          enableDetailedLogging: Joi.boolean().default(true),
          performanceThresholds: Joi.object({
            queryExecutionTime: Joi.number().min(100).default(5000),
            memoryUsage: Joi.number().min(1).max(100).default(80),
            responseTime: Joi.number().min(100).default(500),
          }).default(),
        }).optional(),
        
        // 批处理配置验证
        batch: Joi.object({
          maxConcurrentOperations: Joi.number().min(1).default(5),
          defaultBatchSize: Joi.number().min(1).default(50),
          maxBatchSize: Joi.number().min(1).default(500),
          minBatchSize: Joi.number().min(1).default(10),
          memoryThreshold: Joi.number().min(0).max(1).default(0.80),
          processingTimeout: Joi.number().min(1000).default(3000),
          retryAttempts: Joi.number().min(0).default(3),
          retryDelay: Joi.number().min(100).default(1000),
          adaptiveBatchingEnabled: Joi.boolean().default(true),
          performanceThreshold: Joi.number().min(100).default(1000),
          adjustmentFactor: Joi.number().min(0).max(1).default(0.1),
        }).optional(),
      });

      const value = ValidationUtils.validateConfig(config, schema);

      // 验证集合名称格式
      if (value.collection && !this.validateNamingConvention(value.collection)) {
        this.logger.error(`Invalid collection name format: ${value.collection}`);
        throw new Error(`Invalid collection name format: ${value.collection}`);
      }

      return value;
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Unknown error in validateConfig'),
        { component: 'QdrantConfigService', operation: 'validateConfig' }
      );
      throw error;
    }
  }

  getDefaultConfig(): QdrantConfig {
    return {
      host: 'localhost',
      port: 6333,
      collection: 'code-snippets',
      useHttps: false,
      timeout: 30000,
      
      cache: {
        defaultTTL: 30000,
        maxEntries: 10000,
        cleanupInterval: 60000,
        enableStats: true,
      },
      
      performance: {
        monitoringInterval: 30000,
        metricsRetentionPeriod: 86400000,
        enableDetailedLogging: true,
        performanceThresholds: {
          queryExecutionTime: 5000,
          memoryUsage: 80,
          responseTime: 500,
        },
      },
      
      batch: {
        maxConcurrentOperations: 5,
        defaultBatchSize: 50,
        maxBatchSize: 500,
        minBatchSize: 10,
        memoryThreshold: 0.80,
        processingTimeout: 3000,
        retryAttempts: 3,
        retryDelay: 1000,
        adaptiveBatchingEnabled: true,
        performanceThreshold: 1000,
        adjustmentFactor: 0.1,
      },
    };
  }
}