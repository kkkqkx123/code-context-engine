import { injectable, inject } from 'inversify';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { DatabaseLoggerService } from '../common/DatabaseLoggerService';
import { DatabaseEventType } from '../common/DatabaseEventTypes';
import { NebulaConfig, NebulaConnectionStatus, NebulaQueryResult } from './NebulaTypes';
import { TYPES } from '../../types';
import { IConnectionManager } from '../common/IDatabaseService';
import { NebulaConfigService } from '../../config/service/NebulaConfigService';

import { EventListener } from '../../types';
import { IQueryRunner } from './query/QueryRunner';
import { SpaceValidator, SpaceValidationResult, SpaceValidationErrorType } from './validation/SpaceValidator';
import { EventEmitter } from 'events';

export interface INebulaConnectionManager extends IConnectionManager {
  getConnectionStatus(): NebulaConnectionStatus;
  executeQuery(nGQL: string, parameters?: Record<string, any>): Promise<NebulaQueryResult>;
  executeTransaction(queries: Array<{ query: string; params?: Record<string, any> }>): Promise<NebulaQueryResult[]>;
  // 配置管理
  getConfig(): NebulaConfig;
  // Space 验证
  validateSpace(spaceName: string, forceValidate?: boolean): Promise<SpaceValidationResult>;
  clearSpaceCache(spaceName?: string): void;
  updateConfig(config: Partial<NebulaConfig>): void;
}

@injectable()
export class NebulaConnectionManager implements INebulaConnectionManager {
  private databaseLogger: DatabaseLoggerService;
  private errorHandler: ErrorHandlerService;
  private nebulaConfigService: NebulaConfigService;
  private connectionStatus: NebulaConnectionStatus;
  private config!: NebulaConfig;
  private eventEmitter: EventEmitter;
  private spaceValidator: SpaceValidator;
  private queryRunner: IQueryRunner;

  constructor(
    @inject(TYPES.DatabaseLoggerService) databaseLogger: DatabaseLoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.NebulaConfigService) nebulaConfigService: NebulaConfigService,
    @inject(TYPES.SpaceValidator) spaceValidator: SpaceValidator,
    @inject(TYPES.IQueryRunner) queryRunner: IQueryRunner
  ) {
    this.databaseLogger = databaseLogger;
    this.errorHandler = errorHandler;
    this.nebulaConfigService = nebulaConfigService;
    this.eventEmitter = new EventEmitter();
    this.spaceValidator = spaceValidator;
    this.queryRunner = queryRunner;
    this.connectionStatus = {
      connected: false,
      host: '',
      port: 0,
      username: '',
    };

    // 使用NebulaConfigService加载配置
    this.loadConfiguration();
  }

  /**
   * 从配置更新连接状态
   */
  private updateConnectionStatusFromConfig(): void {
    this.connectionStatus.host = this.config.host;
    this.connectionStatus.port = this.config.port;
    this.connectionStatus.username = this.config.username;
    this.connectionStatus.space = this.getValidSpace(this.config.space);
  }

  /**
   * 获取有效的space名称
   */
  private getValidSpace(space?: string): string | undefined {
    return (space && space !== 'undefined' && space !== '') ? space : undefined;
  }

  async connect(): Promise<boolean> {
    try {
      // 重新加载配置
      const loadedConfig = this.nebulaConfigService.loadConfig();
      // 从NebulaConfigService获取容错配置
      const faultToleranceOptions = loadedConfig.faultTolerance || {
        maxRetries: loadedConfig.retryAttempts || 3,
        retryDelay: loadedConfig.retryDelay || 1000,
        exponentialBackoff: true,
        circuitBreakerEnabled: true,
        circuitBreakerFailureThreshold: 5,
        circuitBreakerTimeout: 3000,
        fallbackStrategy: 'cache' as const
      };

      // 合并配置
      this.config = {
        ...loadedConfig, ...{
          retryAttempts: faultToleranceOptions.maxRetries,
          retryDelay: faultToleranceOptions.retryDelay
        }
      };
      this.updateConnectionStatusFromConfig();

      // 使用 DatabaseLoggerService 记录连接信息
      await this.logDatabaseEvent({
        type: DatabaseEventType.CONNECTION_OPENED,
        source: 'nebula',
        timestamp: new Date(),
        data: {
          message: 'Connecting to Nebula Graph',
          host: this.config.host,
          port: this.config.port,
          username: this.config.username
        }
      });

      // 简单的连接测试
      // 验证配置的 space（如果存在）
      if (this.config.space) {
        const spaceValidation = await this.spaceValidator.validateSpace(this.config.space);
        if (!spaceValidation.isValid) {
          const errorMessage = spaceValidation.error?.message || 'Space validation failed';
          this.errorHandler.handleError(
            new Error(`Space validation failed for '${this.config.space}': ${errorMessage}`),
            {
              component: 'NebulaConnectionManager',
              operation: 'connect',
              spaceName: this.config.space,
              validationError: spaceValidation.error
            }
          );

          // 记录验证失败事件
          await this.logDatabaseEvent({
            type: DatabaseEventType.CONNECTION_ERROR,
            source: 'nebula',
            timestamp: new Date(),
            data: {
              message: 'Space validation failed',
              spaceName: this.config.space,
              error: errorMessage,
              suggestions: spaceValidation.error?.suggestions
            }
          });

          return false;
        }

        // 记录验证成功事件
        await this.logDatabaseEvent({
          type: DatabaseEventType.SERVICE_INITIALIZED,
          source: 'nebula',
          timestamp: new Date(),
          data: {
            message: 'Space validation successful',
            spaceName: this.config.space,
            spaceInfo: spaceValidation.spaceInfo
          }
        });
      }
      await this.queryRunner.execute('YIELD 1 AS connection_test;');

      // 设置连接状态
      this.connectionStatus.connected = true;
      this.connectionStatus.lastConnected = new Date();

      await this.logDatabaseEvent({
        type: DatabaseEventType.CONNECTION_OPENED,
        source: 'nebula',
        timestamp: new Date(),
        data: { message: 'Successfully connected to Nebula Graph' }
      });

      return true;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);

      this.connectionStatus.connected = false;
      this.connectionStatus.error = errorMessage;

      this.errorHandler.handleError(
        new Error(`Failed to connect to Nebula Graph: ${errorMessage}`),
        { component: 'NebulaConnectionManager', operation: 'connect' }
      );

      return false;
    }
  }

  /**
   * 记录数据库事件（错误处理版本）
   */
  private async logDatabaseEvent(event: any): Promise<void> {
    try {
      await this.databaseLogger.logDatabaseEvent(event);
    } catch (error) {
      // 如果日志记录失败，我们不希望影响主流程
      console.error('Failed to log database event:', error);
    }
  }

  async disconnect(): Promise<void> {
    if (!this.connectionStatus.connected) {
      await this.logDatabaseEvent({
        type: DatabaseEventType.CONNECTION_CLOSED,
        source: 'nebula',
        timestamp: new Date(),
        data: { message: 'Already disconnected from Nebula Graph' }
      });
      return;
    }

    try {
      await this.logDatabaseEvent({
        type: DatabaseEventType.CONNECTION_CLOSED,
        source: 'nebula',
        timestamp: new Date(),
        data: { message: 'Disconnecting from Nebula Graph' }
      });

      // 更新连接状态
      this.updateConnectionStatusAfterDisconnect();

      await this.logDatabaseEvent({
        type: DatabaseEventType.CONNECTION_CLOSED,
        source: 'nebula',
        timestamp: new Date(),
        data: { message: 'Successfully disconnected from Nebula Graph' }
      });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.errorHandler.handleError(
        new Error(`Failed to disconnect from Nebula Graph: ${errorMessage}`),
        { component: 'NebulaConnectionManager', operation: 'disconnect' }
      );
    }
  }

  /**
   * 更新断开连接后的状态
   */
  private updateConnectionStatusAfterDisconnect(): void {
    this.connectionStatus.connected = false;
    this.connectionStatus.space = undefined;
  }

  isConnected(): boolean {
    return this.connectionStatus.connected;
  }

  getConnectionStatus(): NebulaConnectionStatus {
    const status = { ...this.connectionStatus };
    if (!status.space || status.space === 'undefined' || status.space === '') {
      status.space = undefined;
    }
    return status;
  }

  async executeQuery(nGQL: string, parameters?: Record<string, any>): Promise<NebulaQueryResult> {
    // 委托给新的QueryRunner
    return await this.queryRunner.execute(nGQL, parameters);
  }

  async executeTransaction(queries: Array<{ query: string; params?: Record<string, any> }>): Promise<NebulaQueryResult[]> {
    // 转换查询格式以匹配QueryRunner的接口
    const queryBatches = queries.map(q => ({
      query: q.query,
      params: q.params,
      options: undefined
    }));

    // 委托给新的QueryRunner
    return await this.queryRunner.executeBatch(queryBatches);
  }

  /**
   * 获取当前配置
   */
  getConfig(): NebulaConfig {
    return { ...this.config };
  }

  /**
   * 更新配置
   */
  updateConfig(config: Partial<NebulaConfig>): void {
    this.config = { ...this.config, ...config };
    this.updateConnectionStatusFromConfig();

    this.databaseLogger.logDatabaseEvent({
      type: DatabaseEventType.SERVICE_INITIALIZED,
      source: 'nebula',
      timestamp: new Date(),
      data: { message: 'Configuration updated', config: this.config }
    });
  }

  /**
   * 加载配置
   */
  private loadConfiguration(): void {
    try {
      // 从NebulaConfigService加载基础配置
      const loadedConfig = this.nebulaConfigService.loadConfig();

      // 从GraphConfigService获取容错配置
      // 从NebulaConfigService获取容错配置
      const faultToleranceOptions = loadedConfig.faultTolerance || {
        maxRetries: loadedConfig.retryAttempts || 3,
        retryDelay: loadedConfig.retryDelay || 1000,
        exponentialBackoff: true,
        circuitBreakerEnabled: true,
        circuitBreakerFailureThreshold: 5,
        circuitBreakerTimeout: 3000,
        fallbackStrategy: 'cache' as const
      };

      // 将容错选项转换为NebulaConfig格式
      const faultTolerantConfig: Partial<NebulaConfig> = {
        retryAttempts: faultToleranceOptions.maxRetries,
        retryDelay: faultToleranceOptions.retryDelay
      };

      // 合并配置，容错配置优先
      this.config = { ...loadedConfig, ...faultTolerantConfig };
      this.updateConnectionStatusFromConfig();

      this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SERVICE_INITIALIZED,
        source: 'nebula',
        timestamp: new Date(),
        data: { message: 'Configuration loaded', config: this.config }
      });
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Failed to load configuration'),
        { component: 'NebulaConnectionManager', operation: 'loadConfiguration' }
      );
      throw error;
    }
  }

  /**
   * 订阅事件（推荐的新API）
   */
  subscribe(eventType: string, listener: EventListener) {
    this.eventEmitter.on(eventType, listener);
    return {
      id: `sub_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
      eventType,
      handler: listener,
      unsubscribe: () => this.eventEmitter.off(eventType, listener)
    };

  }


  /**
   * 验证 space
   * @param spaceName space 名称
   * @param forceValidate 是否强制验证（忽略缓存）
   * @returns 验证结果
   */
  async validateSpace(spaceName: string, forceValidate: boolean = false): Promise<SpaceValidationResult> {
    try {
      return await this.spaceValidator.validateSpace(spaceName, forceValidate);
    } catch (error: any) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.errorHandler.handleError(
        new Error(`Space validation failed: ${errorMessage}`),
        { component: 'NebulaConnectionManager', operation: 'validateSpace', spaceName }
      );

      return {
        isValid: false,
        exists: false,
        isAccessible: false,
        error: {
          type: SpaceValidationErrorType.UNKNOWN_ERROR,
          message: `Space validation failed: ${errorMessage}`,
          details: error
        }
      };
    }
  }

  /**
   * 清除 space 验证缓存
   * @param spaceName 可选，指定要清除的 space 名称，不指定则清除所有缓存
   */
  clearSpaceCache(spaceName?: string): void {
    try {
      this.spaceValidator.clearCache(spaceName);
    } catch (error: any) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Failed to clear space cache'),
        { component: 'NebulaConnectionManager', operation: 'clearSpaceCache', spaceName }
      );
    }
  }
}