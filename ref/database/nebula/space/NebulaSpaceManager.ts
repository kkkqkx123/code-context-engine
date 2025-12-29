import { injectable, inject } from 'inversify';
import { TYPES } from '../../../types';
import { DatabaseLoggerService } from '../../common/DatabaseLoggerService';
import { DatabaseEventType } from '../../common/DatabaseEventTypes';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { ConfigService } from '../../../config/ConfigService';
import { NebulaSpaceInfo, NebulaSpaceConfig } from '../NebulaTypes';
import { INebulaConnectionManager } from '../NebulaConnectionManager';
import { INebulaQueryBuilder } from '../query/NebulaQueryBuilder';
import { INebulaSchemaManager } from '../NebulaSchemaManager';
import { ISpaceNameUtils } from '../SpaceNameUtils';
import { CacheService } from '../../../infrastructure/caching/CacheService';
import { PerformanceMonitor } from '../../../infrastructure/monitoring/PerformanceMonitor';
import { DatabaseType } from '../../../infrastructure/types';

export interface INebulaSpaceManager {
  createSpace(projectId: string, config?: NebulaSpaceConfig): Promise<boolean>;
  deleteSpace(projectId: string): Promise<boolean>;
  listSpaces(): Promise<string[]>;
  getSpaceInfo(projectId: string): Promise<NebulaSpaceInfo | null>;
  checkSpaceExists(projectId: string): Promise<boolean>;
  clearSpace(projectId: string): Promise<boolean>;
  
  // 新增方法（来自 NebulaSpaceService）
  useSpace(spaceName: string): Promise<boolean>;
  executeQueryInSpace(space: string, query: string, parameters?: Record<string, any>): Promise<any>;
  getCurrentSpace(): string | undefined;
  validateSpace(spaceName: string): Promise<boolean>;
}

export interface GraphConfig {
  partitionNum?: number;
  replicaFactor?: number;
  vidType?: string;
}

@injectable()
export class NebulaSpaceManager implements INebulaSpaceManager {
  private nebulaConnection: INebulaConnectionManager;
  private nebulaQueryBuilder: INebulaQueryBuilder;
  private schemaManager: INebulaSchemaManager;
  private spaceNameUtils: ISpaceNameUtils;
  private databaseLogger: DatabaseLoggerService;
  private errorHandler: ErrorHandlerService;
  private configService: ConfigService;
  private cacheService: CacheService;
  private performanceMonitor: PerformanceMonitor;

  constructor(
    @inject(TYPES.INebulaConnectionManager) nebulaConnection: INebulaConnectionManager,
    @inject(TYPES.INebulaQueryBuilder) nebulaQueryBuilder: INebulaQueryBuilder,
    @inject(TYPES.INebulaSchemaManager) schemaManager: INebulaSchemaManager,
    @inject(TYPES.ISpaceNameUtils) spaceNameUtils: ISpaceNameUtils,
    @inject(TYPES.DatabaseLoggerService) databaseLogger: DatabaseLoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.ConfigService) configService: ConfigService,
    @inject(TYPES.CacheService) cacheService: CacheService,
    @inject(TYPES.PerformanceMonitor) performanceMonitor: PerformanceMonitor
  ) {
    this.nebulaConnection = nebulaConnection;
    this.nebulaQueryBuilder = nebulaQueryBuilder;
    this.schemaManager = schemaManager;
    this.spaceNameUtils = spaceNameUtils;
    this.databaseLogger = databaseLogger;
    this.errorHandler = errorHandler;
    this.configService = configService;
    this.cacheService = cacheService;
    this.performanceMonitor = performanceMonitor;
  }

  async createSpace(projectId: string, config: GraphConfig = {}): Promise<boolean> {
    const operationId = this.performanceMonitor.startOperation('createSpace', { projectId });
    
    try {
      const spaceName = projectId.startsWith('project_') ? projectId : this.spaceNameUtils.generateSpaceName(projectId);

      if (!spaceName || spaceName === 'undefined' || spaceName === '') {
        this.performanceMonitor.endOperation(operationId, { success: false, metadata: { error: 'Invalid space name' } });
        return false;
      }

      const createQuery = `
        CREATE SPACE IF NOT EXISTS \`${spaceName}\` (partition_num = ${config.partitionNum || 10}, replica_factor = ${config.replicaFactor || 1}, vid_type = ${config.vidType || 'FIXED_STRING(32)'})
      `;

      await this.nebulaConnection.executeQuery(createQuery);
      await this.waitForSpaceReady(spaceName);
      await this.nebulaConnection.executeQuery(`USE \`${spaceName}\``);
      await this.schemaManager.createGraphSchema(projectId, config);

      this.logSpaceOperation('createSpace', true, { projectId, spaceName });
      this.performanceMonitor.endOperation(operationId, { success: true });
      
      // 清除空间列表缓存
      await this.cacheService.invalidateDatabaseCache(DatabaseType.NEBULA);
      
      return true;
    } catch (error) {
      this.logSpaceOperation('createSpace', false, { projectId, error: error instanceof Error ? error.message : String(error) });
      this.performanceMonitor.endOperation(operationId, { success: false, metadata: { error: error instanceof Error ? error.message : String(error) } });
      return false;
    }
  }

  async deleteSpace(projectId: string): Promise<boolean> {
    const operationId = this.performanceMonitor.startOperation('deleteSpace', { projectId });
    
    try {
      const spaceName = projectId.startsWith('project_') ? projectId : this.spaceNameUtils.generateSpaceName(projectId);

      if (!spaceName || spaceName === 'undefined' || spaceName === '') {
        this.performanceMonitor.endOperation(operationId, { success: false, metadata: { error: 'Invalid space name' } });
        return false;
      }

      await this.nebulaConnection.executeQuery(`DROP SPACE IF EXISTS \`${spaceName}\``);

      this.logSpaceOperation('deleteSpace', true, { projectId, spaceName });
      this.performanceMonitor.endOperation(operationId, { success: true });
      
      // 清除空间列表缓存
      await this.cacheService.invalidateDatabaseCache(DatabaseType.NEBULA);
      
      return true;
    } catch (error) {
      this.logSpaceOperation('deleteSpace', false, { projectId, error: error instanceof Error ? error.message : String(error) });
      this.performanceMonitor.endOperation(operationId, { success: false, metadata: { error: error instanceof Error ? error.message : String(error) } });
      return false;
    }
  }

  async listSpaces(): Promise<string[]> {
    const operationId = this.performanceMonitor.startOperation('listSpaces');
    
    try {
      // 尝试从缓存获取
      const cacheKey = 'nebula:spaces:list';
      let spaces = await this.cacheService.getDatabaseSpecificCache<string[]>(cacheKey, DatabaseType.NEBULA);
      
      if (!spaces) {
        // 缓存未命中，从数据库获取
        spaces = await this.fetchSpacesFromDB();
        // 缓存结果（5分钟TTL）
        await this.cacheService.setDatabaseSpecificCache(
          cacheKey, 
          spaces, 
          DatabaseType.NEBULA, 
          300000
        );
        this.performanceMonitor.recordCacheMiss('nebula-spaces');
      } else {
        this.performanceMonitor.recordCacheHit('nebula-spaces');
      }
      
      this.performanceMonitor.endOperation(operationId, { success: true, resultCount: spaces.length });
      return spaces;
    } catch (error) {
      this.performanceMonitor.endOperation(operationId, { success: false, metadata: { error: error instanceof Error ? error.message : String(error) } });
      this.handleSpaceError(error, 'listSpaces');
    }
  }

  async getSpaceInfo(projectId: string): Promise<NebulaSpaceInfo | null> {
    const operationId = this.performanceMonitor.startOperation('getSpaceInfo', { projectId });
    
    try {
      const spaceName = projectId.startsWith('project_') ? projectId : this.spaceNameUtils.generateSpaceName(projectId);

      if (!spaceName || spaceName === 'undefined' || spaceName === '') {
        this.performanceMonitor.endOperation(operationId, { success: false, metadata: { error: 'Invalid space name' } });
        return null;
      }

      // 尝试从缓存获取
      const cacheKey = `nebula:space:info:${spaceName}`;
      let spaceInfo = await this.cacheService.getDatabaseSpecificCache<NebulaSpaceInfo>(cacheKey, DatabaseType.NEBULA);
      
      if (!spaceInfo) {
        // 缓存未命中，从数据库获取
        spaceInfo = await this.fetchSpaceInfoFromDB(spaceName);
        if (spaceInfo) {
          // 缓存结果（10分钟TTL）
          await this.cacheService.setDatabaseSpecificCache(
            cacheKey, 
            spaceInfo, 
            DatabaseType.NEBULA, 
            600000
          );
        }
        this.performanceMonitor.recordCacheMiss('nebula-space-info');
      } else {
        this.performanceMonitor.recordCacheHit('nebula-space-info');
      }
      
      this.performanceMonitor.endOperation(operationId, { success: true });
      return spaceInfo;
    } catch (error) {
      this.performanceMonitor.endOperation(operationId, { success: false, metadata: { error: error instanceof Error ? error.message : String(error) } });
      return null;
    }
  }

  async checkSpaceExists(projectId: string): Promise<boolean> {
    const operationId = this.performanceMonitor.startOperation('checkSpaceExists', { projectId });
    
    try {
      const spaceName = projectId.startsWith('project_') ? projectId : this.spaceNameUtils.generateSpaceName(projectId);

      if (!spaceName || spaceName === 'undefined' || spaceName === '') {
        this.performanceMonitor.endOperation(operationId, { success: false, metadata: { error: 'Invalid space name' } });
        return false;
      }

      const spaces = await this.listSpaces();
      const exists = spaces.includes(spaceName);
      
      this.performanceMonitor.endOperation(operationId, { success: true, metadata: { exists } });
      return exists;
    } catch (error) {
      this.performanceMonitor.endOperation(operationId, { success: false, metadata: { error: error instanceof Error ? error.message : String(error) } });
      return false;
    }
  }

  async clearSpace(projectId: string): Promise<boolean> {
    const operationId = this.performanceMonitor.startOperation('clearSpace', { projectId });
    
    try {
      const spaceName = projectId.startsWith('project_') ? projectId : this.spaceNameUtils.generateSpaceName(projectId);

      if (!spaceName || spaceName === 'undefined' || spaceName === '') {
        this.performanceMonitor.endOperation(operationId, { success: false, metadata: { error: 'Invalid space name' } });
        return false;
      }

      await this.nebulaConnection.executeQuery(`USE \`${spaceName}\``);

      // 获取并删除所有边类型
      const edgesResult = await this.nebulaConnection.executeQuery('SHOW EDGES');
      const edgesData = edgesResult?.data || edgesResult?.table || edgesResult?.results || [];
      const edges = Array.isArray(edgesData)
        ? edgesData.map((row: any) => row.Name || row.name || row.NAME || row.edge_name).filter(Boolean)
        : [];

      for (const edge of edges) {
        try {
          await this.nebulaConnection.executeQuery(`DELETE EDGE \`${edge}\` * -> *`);
        } catch (edgeError) {
          // 继续处理其他边
        }
      }

      // 获取并删除所有标签
      const tagsResult = await this.nebulaConnection.executeQuery('SHOW TAGS');
      const tagsData = tagsResult?.data || tagsResult?.table || tagsResult?.results || [];
      const tags = Array.isArray(tagsData)
        ? tagsData.map((row: any) => row.Name || row.name || row.NAME || row.tag_name).filter(Boolean)
        : [];

      for (const tag of tags) {
        try {
          await this.nebulaConnection.executeQuery(`DELETE VERTEX * WITH EDGE`);
        } catch (vertexError) {
          // 继续处理其他标签
        }
      }

      this.logSpaceOperation('clearSpace', true, { projectId, spaceName });
      this.performanceMonitor.endOperation(operationId, { success: true });
      return true;
    } catch (error) {
      this.logSpaceOperation('clearSpace', false, { projectId, error: error instanceof Error ? error.message : String(error) });
      this.performanceMonitor.endOperation(operationId, { success: false, metadata: { error: error instanceof Error ? error.message : String(error) } });
      return false;
    }
  }

  // 新增方法（来自 NebulaSpaceService）

  async useSpace(spaceName: string): Promise<boolean> {
    const operationId = this.performanceMonitor.startOperation('useSpace', { spaceName });
    
    try {
      this.validateSpaceName(spaceName);
      
      const result = await this.nebulaConnection.executeQuery(`USE \`${spaceName}\``);
      
      this.performanceMonitor.endOperation(operationId, { success: true });
      return !result || !result.error;
    } catch (error) {
      this.performanceMonitor.endOperation(operationId, { success: false, metadata: { error: error instanceof Error ? error.message : String(error) } });
      this.handleSpaceError(error, 'useSpace');
    }
  }

  async executeQueryInSpace(space: string, query: string, parameters?: Record<string, any>): Promise<any> {
    const operationId = this.performanceMonitor.startOperation('executeQueryInSpace', { space, query: query.substring(0, 100) });
    
    try {
      this.validateSpaceName(space);
      
      // 切换到指定空间
      const switchResult = await this.useSpace(space);
      if (!switchResult) {
        throw new Error(`Failed to switch to space ${space}`);
      }
      
      // 执行查询
      const queryResult = await this.nebulaConnection.executeQuery(query, parameters);
      
      this.performanceMonitor.endOperation(operationId, { 
        success: true, 
        metadata: {
          hasResult: !!queryResult,
          hasError: !!queryResult?.error 
        }
      });
      
      return queryResult;
    } catch (error) {
      this.performanceMonitor.endOperation(operationId, { success: false, metadata: { error: error instanceof Error ? error.message : String(error) } });
      this.handleSpaceError(error, 'executeQueryInSpace');
    }
  }

  getCurrentSpace(): string | undefined {
    try {
      const connectionStatus = this.nebulaConnection.getConnectionStatus();
      const space = connectionStatus?.space;
      
      if (!space || space === 'undefined' || space === '') {
        return undefined;
      }
      
      return space;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to get current space: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'NebulaSpaceManager', operation: 'getCurrentSpace' }
      );
      return undefined;
    }
  }

  async validateSpace(spaceName: string): Promise<boolean> {
    const operationId = this.performanceMonitor.startOperation('validateSpace', { spaceName });
    
    try {
      this.validateSpaceName(spaceName);
      
      // 检查空间是否存在
      const exists = await this.checkSpaceExists(spaceName);
      if (!exists) {
        this.performanceMonitor.endOperation(operationId, { success: false, metadata: { error: 'Space does not exist' } });
        return false;
      }

      // 尝试使用空间并运行简单查询
      const currentSpace = this.getCurrentSpace();
      const useResult = await this.nebulaConnection.executeQuery(`USE \`${spaceName}\``);
      
      if (useResult?.error) {
        this.performanceMonitor.endOperation(operationId, { success: false, metadata: { error: useResult.error } });
        return false;
      }
      
      // 执行简单查询验证空间可访问性
      const validationResult = await this.nebulaConnection.executeQuery('SHOW TAGS');
      
      // 恢复到之前的空间
      if (currentSpace) {
        await this.nebulaConnection.executeQuery(`USE \`${currentSpace}\``);
      }
      
      const isValid = !validationResult?.error;
      this.performanceMonitor.endOperation(operationId, { success: isValid });
      return isValid;
    } catch (error) {
      this.performanceMonitor.endOperation(operationId, { success: false, metadata: { error: error instanceof Error ? error.message : String(error) } });
      return false;
    }
  }

  // 私有辅助方法

  private async fetchSpacesFromDB(): Promise<string[]> {
    const result = await this.nebulaConnection.executeQuery('SHOW SPACES');

    if (!result) {
      return [];
    }

    let data = result.data;
    if (!data) {
      data = result.table || result.results || result.rows || [];
    }

    if (!Array.isArray(data)) {
      if (data && typeof data === 'object') {
        const keys = Object.keys(data);
        if (keys.length > 0) {
          const firstKey = keys[0];
          const value = data[firstKey];
          if (Array.isArray(value)) {
            data = (value as any[]).map((item: any) => ({
              [firstKey]: item
            }));
          } else {
            return [];
          }
        } else {
          return [];
        }
      } else {
        return [];
      }
    }

    if (!Array.isArray(data)) {
      return [];
    }
    
    const spaceNames = data.map((row: any, index: number) => {
      try {
        const name = row.Name || row.name || row.NAME || row.space_name || row.SpaceName;
        if (name && typeof name === 'string') return name.trim();

        const keys = Object.keys(row);
        for (const key of keys) {
          const value = row[key];
          if (typeof value === 'string' && value.length > 0 && value.length < 100) {
            return value.trim();
          }
        }

        return '';
      } catch (rowError) {
        return '';
      }
    }).filter((name: string) => name && name.length > 0);

    return spaceNames;
  }

  private async fetchSpaceInfoFromDB(spaceName: string): Promise<NebulaSpaceInfo | null> {
    const result = await this.nebulaConnection.executeQuery(`DESCRIBE SPACE \`${spaceName}\``);

    if (!result) {
      return null;
    }

    let data = result.data || result.table || result.results || result.rows || [];

    if (!Array.isArray(data) || data.length === 0) {
      return null;
    }

    const spaceInfo = data[0];

    if (!spaceInfo || typeof spaceInfo !== 'object') {
      return null;
    }

    const normalizedInfo: NebulaSpaceInfo = {
      name: spaceName,
      partition_num: parseInt(spaceInfo.partition_num || spaceInfo.PartitionNum || spaceInfo.partitionNum || '10') || 10,
      replica_factor: parseInt(spaceInfo.replica_factor || spaceInfo.ReplicaFactor || spaceInfo.replicaFactor || '1') || 1,
      vid_type: spaceInfo.vid_type || spaceInfo.VidType || spaceInfo.vidType || 'FIXED_STRING(32)',
      charset: spaceInfo.charset || spaceInfo.Charset || 'utf8',
      collate: spaceInfo.collate || spaceInfo.Collate || 'utf8_bin'
    };

    return normalizedInfo;
  }

  private async waitForSpaceReady(
    spaceName: string,
    maxRetries: number = 60,
    retryDelay: number = 2000
  ): Promise<void> {
    if (!spaceName || spaceName === 'undefined' || spaceName === '') {
      throw new Error(`Cannot wait for invalid space: ${spaceName}`);
    }

    for (let i = 0; i < maxRetries; i++) {
      try {
        const spaces = await this.listSpaces();
        if (spaces.includes(spaceName)) {
          try {
            const useResult = await this.nebulaConnection.executeQuery(`USE \`${spaceName}\``);
            if (!useResult || (useResult.error === undefined || useResult.error === null)) {
              return;
            }
          } catch (useError) {
            if (i % 5 === 0) {
              // 每5次尝试记录一次
            }
          }
        }

        if (i % 5 === 0) {
          // 每5次尝试记录一次
        }
      } catch (error) {
        if (i % 5 === 0) {
          // 每5次尝试记录一次
        }
      }

      await new Promise(resolve => setTimeout(resolve, retryDelay));
    }

    throw new Error(`Space ${spaceName} did not become ready within ${maxRetries} retries`);
  }

  private validateSpaceName(spaceName: string): void {
    if (!spaceName || spaceName === 'undefined' || spaceName === '') {
      throw new Error(`Invalid space name: ${spaceName}`);
    }
  }

  private logSpaceOperation(operation: string, success: boolean, data?: any): void {
    const eventType = success ? 
      this.getSuccessEventType(operation) : 
      DatabaseEventType.ERROR_OCCURRED;
      
    this.databaseLogger.logDatabaseEvent({
      type: eventType,
      source: 'nebula',
      timestamp: new Date(),
      data: { 
        message: `${operation} space ${success ? 'success' : 'failed'}`, 
        ...data 
      }
    }).catch(() => {}); // 简化错误处理，避免影响主流程
  }

  private getSuccessEventType(operation: string): DatabaseEventType {
    switch (operation) {
      case 'createSpace': return DatabaseEventType.SPACE_CREATED;
      case 'deleteSpace': return DatabaseEventType.SPACE_DELETED;
      case 'clearSpace': return DatabaseEventType.SPACE_CLEARED;
      default: return DatabaseEventType.SERVICE_INITIALIZED;
    }
  }

  private handleSpaceError(error: unknown, operation: string): never {
    const errorMessage = error instanceof Error ? error.message : String(error);
    
    this.errorHandler.handleError(
      new Error(`Failed to ${operation}: ${errorMessage}`),
      { 
        component: 'NebulaSpaceManager', 
        operation,
        timestamp: new Date()
      }
    );
    
    throw error;
  }
}