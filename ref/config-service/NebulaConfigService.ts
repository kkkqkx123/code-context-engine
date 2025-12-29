import { injectable, inject } from 'inversify';
import * as Joi from 'joi';
import { BaseConfigService } from './BaseConfigService';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { TYPES } from '../../types';
import { HashUtils } from '../../utils/cache/HashUtils';
import { ProjectMappingService } from '../../database/ProjectMappingService';

export interface NebulaCacheConfig {
  defaultTTL: number;
  maxEntries: number;
  cleanupInterval: number;
  enableStats: boolean;
}

export interface NebulaPerformanceConfig {
  monitoringInterval: number;
  metricsRetentionPeriod: number;
  enableDetailedLogging: boolean;
  performanceThresholds: {
    queryExecutionTime: number;
    memoryUsage: number;
    responseTime: number;
  };
}

export interface NebulaBatchConfig {
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

export interface NebulaConfig {
  host: string;
  port: number;
  username: string;
  password: string;
  
  // 基础连接配置
  timeout?: number;
  maxConnections?: number;
  retryAttempts?: number;
  retryDelay?: number;
  space?: string;
  bufferSize?: number;
  pingInterval?: number;
  vidTypeLength?: number;
  
  // 连接池配置
  connectionPool?: {
    minConnections?: number;
    maxConnections?: number;
    acquireTimeout?: number;
    idleTimeout?: number;
    healthCheckInterval?: number;
    healthCheckTimeout?: number;
    maxFailures?: number;
  };
  
  // 缓存配置（扩展）
  cache?: NebulaCacheConfig;
  
  // 性能配置（扩展）
  performance?: NebulaPerformanceConfig;
  
  // 批处理配置（新增）
  batch?: NebulaBatchConfig;
  
  // 容错配置
  faultTolerance?: {
    maxRetries?: number;
    retryDelay?: number;
    exponentialBackoff?: boolean;
    circuitBreakerEnabled?: boolean;
    circuitBreakerFailureThreshold?: number;
    circuitBreakerTimeout?: number;
    fallbackStrategy?: 'cache' | 'default' | 'error';
  };
}

@injectable()
export class NebulaConfigService extends BaseConfigService<NebulaConfig> {
  constructor(
    @inject(TYPES.LoggerService) private logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) private errorHandler: ErrorHandlerService,
    @inject(TYPES.UnifiedMappingService) private unifiedMappingService?: ProjectMappingService
  ) {
    super();
  }

  loadConfig(): NebulaConfig {
    try {
      const rawConfig = {
        host: process.env.NEBULA_HOST || 'localhost',
        port: parseInt(process.env.NEBULA_PORT || '9669'),
        username: process.env.NEBULA_USERNAME || 'root',
        password: process.env.NEBULA_PASSWORD || 'nebula',
        
        // 基础连接配置
        timeout: parseInt(process.env.NEBULA_TIMEOUT || '30000'),
        maxConnections: parseInt(process.env.NEBULA_MAX_CONNECTIONS || '10'),
        retryAttempts: parseInt(process.env.NEBULA_RETRY_ATTEMPTS || '3'),
        retryDelay: parseInt(process.env.NEBULA_RETRY_DELAY || '1000'),
        bufferSize: parseInt(process.env.NEBULA_BUFFER_SIZE || '2000'),
        pingInterval: parseInt(process.env.NEBULA_PING_INTERVAL || '3000'),
        vidTypeLength: parseInt(process.env.NEBULA_VID_TYPE_LENGTH || '128'),
        
        // 连接池配置
        connectionPool: {
          minConnections: parseInt(process.env.NEBULA_POOL_MIN_CONNECTIONS || '2'),
          maxConnections: parseInt(process.env.NEBULA_MAX_CONNECTIONS || '10'),
          acquireTimeout: parseInt(process.env.NEBULA_POOL_ACQUIRE_TIMEOUT || '30000'),
          idleTimeout: parseInt(process.env.NEBULA_POOL_IDLE_TIMEOUT || '300000'),
          healthCheckInterval: parseInt(process.env.NEBULA_POOL_HEALTH_CHECK_INTERVAL || '30000'),
          healthCheckTimeout: parseInt(process.env.NEBULA_POOL_HEALTH_CHECK_TIMEOUT || '5000'),
          maxFailures: parseInt(process.env.NEBULA_POOL_MAX_FAILURES || '3'),
        },
        
        // 缓存配置
        cache: {
          defaultTTL: parseInt(process.env.NEBULA_CACHE_TTL || '30000'),
          maxEntries: parseInt(process.env.NEBULA_CACHE_MAX_ENTRIES || '10000'),
          cleanupInterval: parseInt(process.env.NEBULA_CACHE_CLEANUP_INTERVAL || '60000'),
          enableStats: process.env.NEBULA_CACHE_STATS_ENABLED !== 'false',
        },
        
        // 性能配置
        performance: {
          monitoringInterval: parseInt(process.env.NEBULA_PERFORMANCE_INTERVAL || '1000'),
          metricsRetentionPeriod: parseInt(process.env.NEBULA_PERFORMANCE_RETENTION || '8640000'),
          enableDetailedLogging: process.env.NEBULA_PERFORMANCE_LOGGING_ENABLED !== 'false',
          performanceThresholds: {
            queryExecutionTime: parseInt(process.env.NEBULA_PERFORMANCE_QUERY_TIMEOUT || '1000'),
            memoryUsage: parseFloat(process.env.NEBULA_PERFORMANCE_MEMORY_THRESHOLD || '0.80'),
            responseTime: parseInt(process.env.NEBULA_PERFORMANCE_RESPONSE_THRESHOLD || '2000'),
          },
        },
        
        // 批处理配置
        batch: {
          maxConcurrentOperations: parseInt(process.env.NEBULA_BATCH_CONCURRENCY || '5'),
          defaultBatchSize: parseInt(process.env.NEBULA_BATCH_SIZE_DEFAULT || '50'),
          maxBatchSize: parseInt(process.env.NEBULA_BATCH_SIZE_MAX || '500'),
          minBatchSize: parseInt(process.env.NEBULA_BATCH_SIZE_MIN || '10'),
          memoryThreshold: parseFloat(process.env.NEBULA_BATCH_MEMORY_THRESHOLD || '0.80'),
          processingTimeout: parseInt(process.env.NEBULA_BATCH_PROCESSING_TIMEOUT || '300000'),
          retryAttempts: parseInt(process.env.NEBULA_BATCH_RETRY_ATTEMPTS || '3'),
          retryDelay: parseInt(process.env.NEBULA_BATCH_RETRY_DELAY || '1000'),
          adaptiveBatchingEnabled: process.env.NEBULA_BATCH_ADAPTIVE_ENABLED !== 'false',
          performanceThreshold: parseInt(process.env.NEBULA_BATCH_PERFORMANCE_THRESHOLD || '1000'),
          adjustmentFactor: parseFloat(process.env.NEBULA_BATCH_ADJUSTMENT_FACTOR || '0.1'),
        },
        
        // 容错配置
        faultTolerance: {
          maxRetries: parseInt(process.env.NEBULA_FAULT_TOLERANCE_MAX_RETRIES || '3'),
          retryDelay: parseInt(process.env.NEBULA_FAULT_TOLERANCE_RETRY_DELAY || '1000'),
          exponentialBackoff: process.env.NEBULA_FAULT_TOLERANCE_EXPONENTIAL_BACKOFF !== 'false',
          circuitBreakerEnabled: process.env.NEBULA_FAULT_TOLERANCE_CIRCUIT_BREAKER_ENABLED !== 'false',
          circuitBreakerFailureThreshold: parseInt(process.env.NEBULA_FAULT_TOLERANCE_CIRCUIT_BREAKER_THRESHOLD || '5'),
          circuitBreakerTimeout: parseInt(process.env.NEBULA_FAULT_TOLERANCE_CIRCUIT_BREAKER_TIMEOUT || '3000'),
          fallbackStrategy: (process.env.NEBULA_FAULT_TOLERANCE_FALLBACK_STRATEGY as any) || 'cache',
        },
        
        space: this.getSpaceName(), // 使用增强的配置逻辑
      };

      return this.validateConfig(rawConfig);
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Unknown error in NebulaConfigService'),
        { component: 'NebulaConfigService', operation: 'loadConfig' }
      );
      throw error;
    }
  }

  /**
   * 获取空间名称，实现配置优先级逻辑
   * 1. 检查显式环境配置
   * 2. 否则使用默认值（项目隔离的动态命名在使用时确定）
   */
  private getSpaceName(): string | undefined {
    try {
      // 检查显式环境配置
      const explicitName = process.env.NEBULA_SPACE;
      // 检查是否显式设置了一个有效的空间名称，排除无效值
      if (explicitName && explicitName !== 'code_graphs' && explicitName !== 'undefined' && explicitName !== '') {
        // 显式设置的配置，记录警告日志
        this.logger.warn('Using explicit NEBULA_SPACE configuration, this will override project isolation');
        // 验证显式配置的命名是否符合规范
        if (!this.validateNamingConvention(explicitName)) {
          this.logger.warn(`Explicit space name "${explicitName}" does not follow naming conventions, this may cause issues.`);
        }
        return explicitName;
      }

      // 返回默认的 test_space 作为初始连接空间
      // 后续会根据项目需要动态切换到项目特定的 space
      return 'test_space';
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Unknown error in getSpaceName'),
        { component: 'NebulaConfigService', operation: 'getSpaceName' }
      );
      // 返回默认值以确保服务可用
      return 'test_space';
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
        { component: 'NebulaConfigService', operation: 'validateNamingConvention' }
      );
      return false;
    }
  }

  /**
   * 获取空间名称，支持显式配置或动态生成（供外部使用）
   *
   * @param projectId 项目ID，用于生成动态空间名称
   * @returns 对应的空间名称，优先使用显式环境配置，否则使用项目隔离的动态命名
   * @throws 如果生成的名称不符合命名约定，则抛出错误
   */
  getSpaceNameForProject(projectId: string): string {
    try {
      // 1. 检查显式环境配置
      const explicitName = process.env.NEBULA_SPACE;
      if (explicitName && explicitName !== 'code_graphs') {
        this.logger.warn('Using explicit NEBULA_SPACE configuration, which may override project isolation');
        // 验证显式配置的命名是否符合规范
        if (!this.validateNamingConvention(explicitName)) {
          this.logger.error(`Explicit NEBULA_SPACE name "${explicitName}" does not follow naming conventions, this may cause issues.`);
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
        this.logger.error(`Generated space name "${dynamicName}" does not follow naming conventions.`);
        throw new Error(`Generated space name "${dynamicName}" is invalid`);
      }

      return dynamicName;
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Unknown error in getSpaceNameForProject'),
        { component: 'NebulaConfigService', operation: 'getSpaceNameForProject', projectId }
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
    const projectSpecificName = `project_${projectId}`;
    return explicitName !== projectSpecificName;
  }

  validateConfig(config: any): NebulaConfig {
    try {
      const schema = Joi.object({
        host: Joi.string().hostname().default('localhost'),
        port: Joi.number().port().default(9669),
        username: Joi.string().default('root'),
        password: Joi.string().default('nebula'),
        timeout: Joi.number().default(30000),
        maxConnections: Joi.number().default(10),
        retryAttempts: Joi.number().default(3),
        retryDelay: Joi.number().default(1000),
        space: Joi.string().optional(),
        bufferSize: Joi.number().default(2000),
        pingInterval: Joi.number().default(3000),
        vidTypeLength: Joi.number().min(8).max(256).default(128),
        
        // 连接池配置验证
        connectionPool: Joi.object({
          minConnections: Joi.number().min(1).default(2),
          maxConnections: Joi.number().min(1).default(10),
          acquireTimeout: Joi.number().min(1000).default(30000),
          idleTimeout: Joi.number().min(10000).default(300000),
          healthCheckInterval: Joi.number().min(5000).default(30000),
          healthCheckTimeout: Joi.number().min(1000).default(5000),
          maxFailures: Joi.number().min(1).default(3),
        }).optional(),
        
        // 缓存配置验证
        cache: Joi.object({
          defaultTTL: Joi.number().min(1000).default(30000),
          maxEntries: Joi.number().min(100).default(10000),
          cleanupInterval: Joi.number().min(10000).default(60000),
          enableStats: Joi.boolean().default(true),
        }).optional(),
        
        // 性能配置验证
        performance: Joi.object({
          monitoringInterval: Joi.number().min(100).default(1000),
          metricsRetentionPeriod: Joi.number().min(1000).default(8640000),
          enableDetailedLogging: Joi.boolean().default(true),
          performanceThresholds: Joi.object({
            queryExecutionTime: Joi.number().min(100).default(1000),
            memoryUsage: Joi.number().min(0).max(1).default(0.80),
            responseTime: Joi.number().min(100).default(2000),
          }).default(),
        }).optional(),
        
        // 批处理配置验证
        batch: Joi.object({
          maxConcurrentOperations: Joi.number().min(1).default(5),
          defaultBatchSize: Joi.number().min(1).default(50),
          maxBatchSize: Joi.number().min(1).default(500),
          minBatchSize: Joi.number().min(1).default(10),
          memoryThreshold: Joi.number().min(0).max(1).default(0.80),
          processingTimeout: Joi.number().min(1000).default(300000),
          retryAttempts: Joi.number().min(0).default(3),
          retryDelay: Joi.number().min(100).default(1000),
          adaptiveBatchingEnabled: Joi.boolean().default(true),
          performanceThreshold: Joi.number().min(100).default(1000),
          adjustmentFactor: Joi.number().min(0).max(1).default(0.1),
        }).optional(),
        
        // 容错配置验证
        faultTolerance: Joi.object({
          maxRetries: Joi.number().min(0).default(3),
          retryDelay: Joi.number().min(100).default(1000),
          exponentialBackoff: Joi.boolean().default(true),
          circuitBreakerEnabled: Joi.boolean().default(true),
          circuitBreakerFailureThreshold: Joi.number().min(1).default(5),
          circuitBreakerTimeout: Joi.number().min(1000).default(3000),
          fallbackStrategy: Joi.string().valid('cache', 'default', 'error').default('cache'),
        }).optional(),
      });

      const { error, value } = schema.validate(config);
      if (error) {
        this.logger.error(`Nebula config validation error: ${error.message}`);
        throw new Error(`Nebula config validation error: ${error.message}`);
      }

      // 验证空间名称格式（如果提供了空间名称）
      if (value.space && !this.validateNamingConvention(value.space)) {
        this.logger.error(`Invalid space name format: ${value.space}`);
        throw new Error(`Invalid space name format: ${value.space}`);
      }

      // 连接池配置现在使用统一的 maxConnections 配置

      return value;
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Unknown error in validateConfig'),
        { component: 'NebulaConfigService', operation: 'validateConfig' }
      );
      throw error;
    }
  }

  getDefaultConfig(): NebulaConfig {
    return {
      host: 'localhost',
      port: 9669,
      username: 'root',
      password: 'nebula',
      timeout: 30000,
      maxConnections: 10,
      retryAttempts: 3,
      retryDelay: 1000,
      space: 'test_space',
      bufferSize: 2000,
      pingInterval: 3000,
      vidTypeLength: 128,
      
      connectionPool: {
        minConnections: 2,
        maxConnections: 10,
        acquireTimeout: 30000,
        idleTimeout: 300000,
        healthCheckInterval: 30000,
        healthCheckTimeout: 5000,
        maxFailures: 3,
      },
      
      cache: {
        defaultTTL: 30000,
        maxEntries: 10000,
        cleanupInterval: 60000,
        enableStats: true,
      },
      
      performance: {
        monitoringInterval: 1000,
        metricsRetentionPeriod: 8640000,
        enableDetailedLogging: true,
        performanceThresholds: {
          queryExecutionTime: 1000,
          memoryUsage: 0.80,
          responseTime: 2000,
        },
      },
      
      batch: {
        maxConcurrentOperations: 5,
        defaultBatchSize: 50,
        maxBatchSize: 500,
        minBatchSize: 10,
        memoryThreshold: 0.80,
        processingTimeout: 300000,
        retryAttempts: 3,
        retryDelay: 1000,
        adaptiveBatchingEnabled: true,
        performanceThreshold: 1000,
        adjustmentFactor: 0.1,
      },
      
      faultTolerance: {
        maxRetries: 3,
        retryDelay: 1000,
        exponentialBackoff: true,
        circuitBreakerEnabled: true,
        circuitBreakerFailureThreshold: 5,
        circuitBreakerTimeout: 3000,
        fallbackStrategy: 'cache',
      },
    };
  }
}