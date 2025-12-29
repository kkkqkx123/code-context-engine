import { injectable, inject } from 'inversify';
import { DatabaseLoggerService } from '../common/DatabaseLoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { TYPES } from '../../types';
import { INebulaQueryService } from './query/NebulaQueryService';
import { INebulaIndexManager } from './NebulaIndexManager';
import { DatabaseError } from '../common/DatabaseError';
import { NebulaSpaceConfig } from './NebulaTypes';

/**
 * Nebula 图模式管理器接口
 */
export interface INebulaSchemaManager {
  /**
   * 创建图空间的模式（标签、边类型、索引等）
   */
  createGraphSchema(projectId: string, config?: NebulaSpaceConfig): Promise<boolean>;

  /**
   * 创建标签（节点类型）
   */
  createTag(projectId: string, tagName: string, properties: { name: string; type: string; nullable?: boolean }[]): Promise<boolean>;

  /**
   * 创建边类型
   */
  createEdgeType(projectId: string, edgeName: string, properties: { name: string; type: string; nullable?: boolean }[]): Promise<boolean>;

  /**
   * 检查标签是否存在
   */
  tagExists(projectId: string, tagName: string): Promise<boolean>;

  /**
   * 检查边类型是否存在
   */
  edgeTypeExists(projectId: string, edgeName: string): Promise<boolean>;

  /**
   * 获取所有标签
   */
  getAllTags(projectId: string): Promise<any[]>;

  /**
   * 获取所有边类型
   */
  getAllEdgeTypes(projectId: string): Promise<any[]>;
}

/**
 * Nebula 图模式管理器实现
 * 
 * 负责管理图数据库的模式定义，包括标签、边类型等
 */
@injectable()
export class NebulaSchemaManager implements INebulaSchemaManager {
  private databaseLogger: DatabaseLoggerService;
  private errorHandler: ErrorHandlerService;
  private queryService: INebulaQueryService;
  private indexManager: INebulaIndexManager;

  constructor(
    @inject(TYPES.DatabaseLoggerService) databaseLogger: DatabaseLoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.INebulaQueryService) queryService: INebulaQueryService,
    @inject(TYPES.INebulaIndexManager) indexManager: INebulaIndexManager
  ) {
    this.databaseLogger = databaseLogger;
    this.errorHandler = errorHandler;
    this.queryService = queryService;
    this.indexManager = indexManager;
  }

  /**
   * 创建图空间的完整模式
   */
  async createGraphSchema(projectId: string, config?: NebulaSpaceConfig): Promise<boolean> {
    try {
      // 定义默认的标签配置
      const defaultTags = [
        {
          name: 'File',
          properties: [
            { name: 'name', type: 'string' },
            { name: 'path', type: 'string' },
            { name: 'content', type: 'string' },
            { name: 'type', type: 'string' },
            { name: 'projectId', type: 'string' }
          ]
        },
        {
          name: 'Function',
          properties: [
            { name: 'name', type: 'string' },
            { name: 'signature', type: 'string' },
            { name: 'code', type: 'string' },
            { name: 'projectId', type: 'string' }
          ]
        },
        {
          name: 'Class',
          properties: [
            { name: 'name', type: 'string' },
            { name: 'definition', type: 'string' },
            { name: 'projectId', type: 'string' }
          ]
        },
        {
          name: 'Variable',
          properties: [
            { name: 'name', type: 'string' },
            { name: 'type', type: 'string' },
            { name: 'value', type: 'string' },
            { name: 'projectId', type: 'string' }
          ]
        },
        {
          name: 'Comment',
          properties: [
            { name: 'content', type: 'string' },
            { name: 'projectId', type: 'string' }
          ]
        }
      ];

      // 定义默认的边类型配置
      const defaultEdges = [
        {
          name: 'contains',
          properties: [
            { name: 'order', type: 'int' },
            { name: 'projectId', type: 'string' }
          ]
        },
        {
          name: 'dependsOn',
          properties: [
            { name: 'type', type: 'string' },
            { name: 'projectId', type: 'string' }
          ]
        },
        {
          name: 'calls',
          properties: [
            { name: 'lineNumber', type: 'int' },
            { name: 'projectId', type: 'string' }
          ]
        },
        {
          name: 'imports',
          properties: [
            { name: 'alias', type: 'string' },
            { name: 'projectId', type: 'string' }
          ]
        },
        {
          name: 'extends',
          properties: [
            { name: 'projectId', type: 'string' }
          ]
        },
        {
          name: 'implements',
          properties: [
            { name: 'projectId', type: 'string' }
          ]
        }
      ];

      // 合并默认配置和用户提供的配置
      const tags = config?.tags ? [...defaultTags, ...config.tags] : defaultTags;
      const edges = config?.edges ? [...defaultEdges, ...config.edges] : defaultEdges;

      // 创建所有标签
      for (const tag of tags) {
        const tagCreated = await this.createTag(projectId, tag.name, tag.properties);
        if (!tagCreated) {
          throw new Error(`Failed to create tag: ${tag.name}`);
        }
      }

      // 创建所有边类型
      for (const edge of edges) {
        const edgeCreated = await this.createEdgeType(projectId, edge.name, edge.properties);
        if (!edgeCreated) {
          throw new Error(`Failed to create edge type: ${edge.name}`);
        }
      }

      // 为常用字段创建索引
      for (const tag of tags) {
        if (tag.properties.some((prop: { name: string; }) => prop.name === 'name')) {
          await this.indexManager.createTagIndex(projectId, tag.name, 'name');
        }
        if (tag.properties.some((prop: { name: string; }) => prop.name === 'projectId')) {
          await this.indexManager.createTagIndex(projectId, tag.name, 'projectId');
        }
      }

      for (const edge of edges) {
        if (edge.properties.some((prop: { name: string; }) => prop.name === 'projectId')) {
          await this.indexManager.createEdgeIndex(projectId, edge.name, 'projectId');
        }
      }

      return true;
    } catch (error) {
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaSchemaManager',
          operation: 'createGraphSchema',
          details: { projectId, config }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      return false;
    }
  }

  /**
   * 创建标签（节点类型）
   */
  async createTag(projectId: string, tagName: string, properties: { name: string; type: string; nullable?: boolean }[]): Promise<boolean> {
    try {
      if (await this.tagExists(projectId, tagName)) {
        return true; // 标签已存在，视为成功
      }

      // 构建属性定义字符串
      const propertiesDefinition = properties.map(prop => {
        let propDef = `${prop.name} ${prop.type}`;
        if (prop.nullable === false) {
          propDef += ' NOT NULL';
        }
        return propDef;
      }).join(', ');

      const query = `CREATE TAG \`${tagName}\` (${propertiesDefinition})`;
      await this.queryService.executeQuery(query);

      return true;
    } catch (error) {
      // 忽略"already exists"错误
      if (error instanceof Error && error.message.includes('already exist')) {
        return true;
      }

      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaSchemaManager',
          operation: 'createTag',
          details: { projectId, tagName, properties }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      return false;
    }
  }

  /**
   * 创建边类型
   */
  async createEdgeType(projectId: string, edgeName: string, properties: { name: string; type: string; nullable?: boolean }[]): Promise<boolean> {
    try {
      if (await this.edgeTypeExists(projectId, edgeName)) {
        return true; // 边类型已存在，视为成功
      }

      // 构建属性定义字符串
      const propertiesDefinition = properties.map(prop => {
        let propDef = `${prop.name} ${prop.type}`;
        if (prop.nullable === false) {
          propDef += ' NOT NULL';
        }
        return propDef;
      }).join(', ');

      const query = `CREATE EDGE \`${edgeName}\` (${propertiesDefinition})`;
      await this.queryService.executeQuery(query);

      return true;
    } catch (error) {
      // 忽略"already exists"错误
      if (error instanceof Error && error.message.includes('already exist')) {
        return true;
      }

      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaSchemaManager',
          operation: 'createEdgeType',
          details: { projectId, edgeName, properties }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      return false;
    }
  }

  /**
   * 检查标签是否存在
   */
  async tagExists(projectId: string, tagName: string): Promise<boolean> {
    try {
      const allTags = await this.getAllTags(projectId);
      return allTags.some(tag => tag.Name === tagName || tag.name === tagName);
    } catch (error) {
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaSchemaManager',
          operation: 'tagExists',
          details: { projectId, tagName }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      return false;
    }
  }

  /**
   * 检查边类型是否存在
   */
  async edgeTypeExists(projectId: string, edgeName: string): Promise<boolean> {
    try {
      const allEdges = await this.getAllEdgeTypes(projectId);
      return allEdges.some(edge => edge.Name === edgeName || edge.name === edgeName);
    } catch (error) {
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaSchemaManager',
          operation: 'edgeTypeExists',
          details: { projectId, edgeName }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      return false;
    }
  }

  /**
   * 获取所有标签
   */
  async getAllTags(projectId: string): Promise<any[]> {
    try {
      const result = await this.queryService.executeQuery('SHOW TAGS');
      return result.data || [];
    } catch (error) {
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaSchemaManager',
          operation: 'getAllTags',
          details: { projectId }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      return [];
    }
  }

  /**
   * 获取所有边类型
   */
  async getAllEdgeTypes(projectId: string): Promise<any[]> {
    try {
      const result = await this.queryService.executeQuery('SHOW EDGES');
      return result.data || [];
    } catch (error) {
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaSchemaManager',
          operation: 'getAllEdgeTypes',
          details: { projectId }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);
      return [];
    }
  }
}