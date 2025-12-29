import { injectable } from 'inversify';
import { DatabaseType, DatabaseConfig } from './DatabaseServiceFactory';
import { DatabaseError, DatabaseErrorType } from './DatabaseError';
import { DatabaseLoggerService } from './DatabaseLoggerService';
import { DatabaseEventType, DatabaseEvent } from './DatabaseEventTypes';

/**
 * 环境类型枚举
 */
export enum EnvironmentType {
  DEVELOPMENT = 'development',
  TEST = 'test',
  PRODUCTION = 'production'
}

/**
 * 配置管理器接口
 */
export interface IConfigManager {
  getDatabaseConfig(type: DatabaseType): DatabaseConfig | null;
  setDatabaseConfig(type: DatabaseType, config: DatabaseConfig): void;
  removeDatabaseConfig(type: DatabaseType): void;
  getAllConfigs(): Map<DatabaseType, DatabaseConfig>;
  validateConfig(config: DatabaseConfig): boolean;
  getCurrentEnvironment(): EnvironmentType;
  setCurrentEnvironment(env: EnvironmentType): void;
}

/**
 * 数据库配置管理器
 * 管理不同数据库类型的配置信息
 */
@injectable()
export class DatabaseConfigManager implements IConfigManager {
  private configs: Map<DatabaseType, DatabaseConfig> = new Map();
  private currentEnvironment: EnvironmentType = EnvironmentType.DEVELOPMENT;
  private databaseLogger: DatabaseLoggerService;

  constructor(databaseLogger?: DatabaseLoggerService) {
    if (databaseLogger) {
      this.databaseLogger = databaseLogger;
    } else {
      // 创建一个基本的logger实现
      let databaseLogLevel = 'info';
      this.databaseLogger = {
        loggerService: null as any,
        configService: null as any,
        databaseLogLevel: databaseLogLevel,

        async logDatabaseEvent(event: any): Promise<void> {
          console.log('Database event:', event);
        },

        async logQueryPerformance(query: string, duration: number, resultCount?: number): Promise<void> {
          console.log(`Query performance: ${query} took ${duration}ms, returned ${resultCount} results`);
        },

        async logConnectionEvent(operation: string, status: 'success' | 'failed', details: any): Promise<void> {
          console.log(`Connection event: ${operation} ${status}`, details);
        },

        async logBatchOperation(operation: string, batchSize: number, performance: any): Promise<void> {
          console.log(`Batch operation: ${operation} with ${batchSize} items`, performance);
        },

        async logCollectionOperation(operation: string, projectPath: string, status: 'success' | 'failed', details: any): Promise<void> {
          console.log(`Collection operation: ${operation} for ${projectPath} ${status}`, details);
        },

        async logVectorOperation(operation: string, projectPath: string, status: 'success' | 'failed', details: any): Promise<void> {
          console.log(`Vector operation: ${operation} for ${projectPath} ${status}`, details);
        },

        async logQueryOperation(operation: string, projectPath: string, status: 'success' | 'failed', details: any): Promise<void> {
          console.log(`Query operation: ${operation} for ${projectPath} ${status}`, details);
        },

        async logProjectOperation(operation: string, projectPath: string, status: 'success' | 'failed', details: any): Promise<void> {
          console.log(`Project operation: ${operation} for ${projectPath} ${status}`, details);
        },

        updateLogLevel(level: string): void {
          databaseLogLevel = level;
          // 更新属性值以反映当前状态
          (this as any).databaseLogLevel = level;
        },

        getLogLevelForEvent(eventType: string): string {
          const defaultMapping: Record<string, string> = {
            'connection_opened': 'info',
            'connection_closed': 'info',
            'connection_failed': 'error',
            'connection_error': 'error',
            'space_created': 'info',
            'space_deleted': 'info',
            'space_error': 'error',
            'data_inserted': 'debug',
            'data_updated': 'debug',
            'data_deleted': 'debug',
            'data_queried': 'debug',
            'data_error': 'error',
            'service_initialized': 'info',
            'service_error': 'error',
            'performance_metric': 'info',
            'query_executed': 'debug',
            'batch_operation_completed': 'info',
            'error_occurred': 'error'
          };

          return defaultMapping[eventType] || databaseLogLevel;
        },

        getPerformanceThreshold(): number {
          return 1000;
        }
      } as unknown as DatabaseLoggerService;
    }

    this.loadDefaultConfigs();
  }

  /**
   * 加载默认配置
   */
  private loadDefaultConfigs(): void {
    // 根据环境加载默认配置
    if (this.currentEnvironment === EnvironmentType.DEVELOPMENT) {
      // 开发环境默认配置
      this.configs.set(DatabaseType.QDRANT, {
        type: DatabaseType.QDRANT,
        connection: {
          host: process.env.QDRANT_HOST || 'localhost',
          port: parseInt(process.env.QDRANT_PORT || '6333'),
          apiKey: process.env.QDRANT_API_KEY
        },
        features: {
          enableCaching: false,
          enableCompression: false,
          enableEncryption: false,
          enableMonitoring: true
        }
      });

      this.configs.set(DatabaseType.NEBULA, {
        type: DatabaseType.NEBULA,
        connection: {
          host: process.env.NEBULA_HOST || 'localhost',
          port: parseInt(process.env.NEBULA_PORT || '9669'),
          username: process.env.NEBULA_USER || 'root',
          password: process.env.NEBULA_PASSWORD || 'nebula',
          timeout: 30000,
          retryAttempts: 3,
          retryDelay: 1000
        },
        features: {
          enableCaching: false,
          enableCompression: false,
          enableEncryption: false,
          enableMonitoring: true
        }
      });
    } else {
      // 生产环境配置从环境变量加载
      this.loadConfigsFromEnv();
    }
  }

  /**
    * 从环境变量加载配置
    */
  private loadConfigsFromEnv(): void {
    // 从环境变量加载Qdrant配置
    const qdrantHost = process.env.QDRANT_HOST;
    const qdrantPort = process.env.QDRANT_PORT;
    if (qdrantHost && qdrantPort) {
      this.configs.set(DatabaseType.QDRANT, {
        type: DatabaseType.QDRANT,
        connection: {
          host: qdrantHost,
          port: parseInt(qdrantPort),
          apiKey: process.env.QDRANT_API_KEY
        },
        features: {
          enableCaching: true,
          enableCompression: true,
          enableEncryption: true,
          enableMonitoring: true
        }
      });
    }

    // 从环境变量加载Nebula配置
    const nebulaHost = process.env.NEBULA_HOST;
    const nebulaPort = process.env.NEBULA_PORT;
    const nebulaUser = process.env.NEBULA_USER;
    const nebulaPassword = process.env.NEBULA_PASSWORD;
    if (nebulaHost && nebulaPort && nebulaUser && nebulaPassword) {
      this.configs.set(DatabaseType.NEBULA, {
        type: DatabaseType.NEBULA,
        connection: {
          host: nebulaHost,
          port: parseInt(nebulaPort),
          username: nebulaUser,
          password: nebulaPassword,
          timeout: parseInt(process.env.NEBULA_TIMEOUT || '3000'),
          retryAttempts: parseInt(process.env.NEBULA_RETRY_ATTEMPTS || '3'),
          retryDelay: parseInt(process.env.NEBULA_RETRY_DELAY || '1000')
        },
        features: {
          enableCaching: true,
          enableCompression: true,
          enableEncryption: true,
          enableMonitoring: true
        }
      });
    }
  }

  /**
   * 获取数据库配置
   */
  getDatabaseConfig(type: DatabaseType): DatabaseConfig | null {
    const config = this.configs.get(type);

    // 记录配置访问事件
    this.databaseLogger.logDatabaseEvent({
      type: DatabaseEventType.CONFIG_ACCESSED,
      source: 'common',
      timestamp: new Date(),
      data: {
        message: `Configuration accessed for database type: ${type}`,
        databaseType: type,
        hasConfig: !!config
      }
    }).catch(error => {
      console.error('Failed to log config access event:', error);
    });

    return config || null;
  }

  /**
   * 设置数据库配置
   */
  setDatabaseConfig(type: DatabaseType, config: DatabaseConfig): void {
    // 验证配置
    if (!this.validateConfig(config)) {
      throw DatabaseError.validationError(
        `Invalid configuration for database type: ${type}`,
        {
          component: 'DatabaseConfigManager',
          operation: 'setDatabaseConfig',
          details: { type, config }
        }
      );
    }

    // 检查类型是否匹配
    if (config.type !== type) {
      throw DatabaseError.validationError(
        `Configuration type mismatch: expected ${type}, got ${config.type}`,
        {
          component: 'DatabaseConfigManager',
          operation: 'setDatabaseConfig',
          details: { type, configType: config.type }
        }
      );
    }

    this.configs.set(type, config);

    // 记录配置设置事件
    this.databaseLogger.logDatabaseEvent({
      type: DatabaseEventType.CONFIG_SET,
      source: 'common',
      timestamp: new Date(),
      data: {
        message: `Configuration set for database type: ${type}`,
        databaseType: type
      }
    }).catch(error => {
      console.error('Failed to log config set event:', error);
    });
  }

  /**
   * 删除数据库配置
   */
  removeDatabaseConfig(type: DatabaseType): void {
    const existed = this.configs.delete(type);

    // 记录配置删除事件
    this.databaseLogger.logDatabaseEvent({
      type: DatabaseEventType.CONFIG_REMOVED,
      source: 'common',
      timestamp: new Date(),
      data: {
        message: `Configuration removed for database type: ${type}`,
        databaseType: type,
        wasPresent: existed
      }
    }).catch(error => {
      console.error('Failed to log config removal event:', error);
    });
  }

  /**
   * 获取所有配置
   */
  getAllConfigs(): Map<DatabaseType, DatabaseConfig> {
    // 返回配置的副本，防止外部修改
    const configsCopy = new Map<DatabaseType, DatabaseConfig>();
    this.configs.forEach((config, type) => {
      configsCopy.set(type, { ...config });
    });

    return configsCopy;
  }

  /**
   * 验证配置
   */
  validateConfig(config: DatabaseConfig): boolean {
    if (!config || typeof config !== 'object') {
      return false;
    }

    if (!config.type || !Object.values(DatabaseType).includes(config.type)) {
      return false;
    }

    if (!config.connection || typeof config.connection !== 'object') {
      return false;
    }

    if (!config.connection.host || typeof config.connection.host !== 'string') {
      return false;
    }

    if (typeof config.connection.port !== 'number' || config.connection.port <= 0) {
      return false;
    }

    if (config.type === DatabaseType.NEBULA) {
      if (!config.connection.username || typeof config.connection.username !== 'string') {
        return false;
      }
      if (!config.connection.password || typeof config.connection.password !== 'string') {
        return false;
      }
    }

    if (config.features && typeof config.features !== 'object') {
      return false;
    }

    return true;
  }

  /**
   * 获取当前环境
   */
  getCurrentEnvironment(): EnvironmentType {
    return this.currentEnvironment;
  }

  /**
   * 设置当前环境
   */
  setCurrentEnvironment(env: EnvironmentType): void {
    const oldEnv = this.currentEnvironment;
    this.currentEnvironment = env;

    // 当环境改变时，重新加载配置
    this.loadDefaultConfigs();

    // 记录环境变更事件
    this.databaseLogger.logDatabaseEvent({
      type: DatabaseEventType.ENVIRONMENT_CHANGED,
      source: 'common',
      timestamp: new Date(),
      data: {
        message: `Environment changed from ${oldEnv} to ${env}`,
        oldEnvironment: oldEnv,
        newEnvironment: env
      }
    }).catch(error => {
      console.error('Failed to log environment change event:', error);
    });
  }

  /**
   * 重新加载配置（例如从文件或环境变量）
   */
  reloadConfigs(): void {
    // 清除现有配置
    this.configs.clear();

    // 重新加载配置
    this.loadDefaultConfigs();

    // 记录重载事件
    this.databaseLogger.logDatabaseEvent({
      type: DatabaseEventType.CONFIG_RELOADED,
      source: 'common',
      timestamp: new Date(),
      data: {
        message: 'Configuration reloaded'
      }
    }).catch(error => {
      console.error('Failed to log config reload event:', error);
    });
  }

  /**
   * 获取特定环境的配置
   */
  getConfigForEnvironment(type: DatabaseType, env: EnvironmentType): DatabaseConfig | null {
    // 为特定环境获取配置（可以扩展以支持不同环境的特定配置）
    if (env === this.getCurrentEnvironment()) {
      return this.getDatabaseConfig(type);
    }

    // 这里可以实现特定环境的配置获取逻辑
    // 例如，从不同的配置源加载特定环境的配置
    return this.getDatabaseConfig(type);
  }
}