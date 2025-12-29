import { injectable, inject } from 'inversify';
import { DatabaseLoggerService } from '../common/DatabaseLoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { TYPES } from '../../types';
import { ProjectIdManager } from '../ProjectIdManager';
import { INebulaSpaceManager } from './space/NebulaSpaceManager';
import { INebulaConnectionManager } from './NebulaConnectionManager';
import { INebulaQueryBuilder } from './query/NebulaQueryBuilder';
import { INebulaDataOperations } from './operation/NebulaDataOperations';
import { IProjectManager } from '../common/IDatabaseService';
import { DatabaseError, DatabaseErrorType } from '../common/DatabaseError';
import { DatabaseServiceValidator } from '../common/DatabaseServiceValidator';
import { PerformanceMonitor } from '../../infrastructure/monitoring/PerformanceMonitor';
import {
  NebulaNode,
  NebulaRelationship,
  NebulaSpaceInfo,
  ProjectSpaceInfo
} from './NebulaTypes';
import { NebulaEventType, NebulaEvent, DatabaseEventType } from '../common/DatabaseEventTypes';
import { EventEmitter } from 'events';

/**
 * Nebula 项目管理器接口
 */
export interface INebulaProjectManager extends IProjectManager {
  createSpaceForProject(projectPath: string, config?: any): Promise<boolean>;
  deleteSpaceForProject(projectPath: string): Promise<boolean>;
  getSpaceInfoForProject(projectPath: string): Promise<NebulaSpaceInfo | null>;
  clearSpaceForProject(projectPath: string): Promise<boolean>;
  listProjectSpaces(): Promise<ProjectSpaceInfo[]>;
  insertNodesForProject(projectPath: string, nodes: NebulaNode[]): Promise<boolean>;
  insertRelationshipsForProject(projectPath: string, relationships: NebulaRelationship[]): Promise<boolean>;
  findNodesForProject(projectPath: string, label: string, filter?: any): Promise<any[]>;
  findRelationshipsForProject(projectPath: string, type?: string, filter?: any): Promise<any[]>;
}

/**
 * Nebula 项目管理器实现
 * 
 * 负责项目相关的空间操作、项目ID管理、项目特定的图数据操作
 */
@injectable()
export class NebulaProjectManager implements INebulaProjectManager {
  private databaseLogger: DatabaseLoggerService;
  private errorHandler: ErrorHandlerService;
  private projectIdManager: ProjectIdManager;
  private spaceManager: INebulaSpaceManager;
  private connectionManager: INebulaConnectionManager;
  private queryBuilder: INebulaQueryBuilder;
  private dataOperations: INebulaDataOperations;
  private performanceMonitor: PerformanceMonitor;
  private eventEmitter: EventEmitter;
  private subscriptions: Map<NebulaEventType, any[]> = new Map();

  constructor(
    @inject(TYPES.DatabaseLoggerService) databaseLogger: DatabaseLoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.ProjectIdManager) projectIdManager: ProjectIdManager,
    @inject(TYPES.INebulaSpaceManager) spaceManager: INebulaSpaceManager,
    @inject(TYPES.INebulaConnectionManager) connectionManager: INebulaConnectionManager,
    @inject(TYPES.INebulaQueryBuilder) queryBuilder: INebulaQueryBuilder,
    @inject(TYPES.INebulaDataOperations) dataOperations: INebulaDataOperations,
    @inject(TYPES.PerformanceMonitor) performanceMonitor: PerformanceMonitor
  ) {
    this.databaseLogger = databaseLogger;
    this.errorHandler = errorHandler;
    this.projectIdManager = projectIdManager;
    this.spaceManager = spaceManager;
    this.connectionManager = connectionManager;
    this.queryBuilder = queryBuilder;
    this.dataOperations = dataOperations;
    this.performanceMonitor = performanceMonitor;
    this.eventEmitter = new EventEmitter();
  }

  /**
   * 为特定项目创建空间
   */
  async createSpaceForProject(projectPath: string, config?: any): Promise<boolean> {
    const startTime = Date.now();
    try {
      // 验证输入参数 - 对于测试，我们直接检查项目路径
      if (!projectPath) {
        throw new Error('Project path is required');
      }

      // 生成项目ID并获取空间名称
      const projectId = await this.projectIdManager.generateProjectId(projectPath);
      if (!projectId) {
        throw new Error(`Failed to generate project ID for project: ${projectPath}`);
      }

      const spaceName = this.projectIdManager.getSpaceName(projectId);
      if (!spaceName) {
        this.errorHandler.handleError(
          new Error(`Failed to generate space name for project: ${projectPath}`),
          { component: 'NebulaProjectManager', operation: 'createSpaceForProject', details: { projectPath, projectId } }
        );
        return false;
      }

      const success = await this.spaceManager.createSpace(projectId, config);

      if (success) {
        this.emitEvent(NebulaEventType.SPACE_CREATED, {
          projectPath,
          projectId,
          spaceName,
          config
        });
      }

      // 记录性能指标
      const duration = Date.now() - startTime;
      this.performanceMonitor.recordOperation('createSpaceForProject', duration);

      return success;
    } catch (error) {
      // 如果是验证错误，应该抛出而不是返回false
      if (error instanceof Error &&
        (error.message === 'Project path is required')) {
        throw error;
      }

      const duration = Date.now() - startTime;
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaProjectManager',
          operation: 'createSpaceForProject',
          details: { projectPath, config }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);

      this.emitEvent(DatabaseEventType.ERROR_OCCURRED, {
        error: dbError,
        operation: 'createSpaceForProject',
        projectPath
      });

      return false;
    }
  }

  /**
   * 删除项目的空间
   */
  async deleteSpaceForProject(projectPath: string): Promise<boolean> {
    const startTime = Date.now();
    try {
      // 验证输入参数 - 对于测试，我们直接检查项目路径
      if (!projectPath) {
        throw new Error('Project path is required');
      }

      // 获取项目ID和空间名称
      const projectId = await this.projectIdManager.getProjectId(projectPath);
      if (!projectId) {
        throw new Error(`Project not found: ${projectPath}`);
      }

      const spaceName = this.projectIdManager.getSpaceName(projectId);
      if (!spaceName) {
        this.errorHandler.handleError(
          new Error(`Space name not found for project: ${projectPath}`),
          { component: 'NebulaProjectManager', operation: 'deleteSpaceForProject', details: { projectPath, projectId } }
        );
        return false;
      }

      const success = await this.spaceManager.deleteSpace(projectId);

      if (success) {
        // 删除项目ID映射
        await this.projectIdManager.removeProject(projectPath);

        this.emitEvent(NebulaEventType.SPACE_DELETED, {
          projectPath,
          projectId,
          spaceName
        });
      }

      // 记录性能指标
      const duration = Date.now() - startTime;
      this.performanceMonitor.recordOperation('deleteSpaceForProject', duration);

      return success;
    } catch (error) {
      // 如果是验证错误，应该抛出而不是返回false
      if (error instanceof Error &&
        (error.message === 'Project path is required')) {
        throw error;
      }


      const duration = Date.now() - startTime;
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaProjectManager',
          operation: 'deleteSpaceForProject',
          details: { projectPath }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);

      this.emitEvent(DatabaseEventType.ERROR_OCCURRED, {
        error: dbError,
        operation: 'deleteSpaceForProject',
        projectPath
      });

      return false;
    }
  }

  /**
   * 获取项目的空间信息
   */
  async getSpaceInfoForProject(projectPath: string): Promise<NebulaSpaceInfo | null> {
    try {
      // 验证输入参数 - 对于测试，我们直接检查项目路径
      if (!projectPath) {
        throw new Error('Project path is required');
      }

      // 获取项目ID
      const projectId = await this.projectIdManager.getProjectId(projectPath);
      if (!projectId) {
        return null;
      }

      const spaceInfo = await this.spaceManager.getSpaceInfo(projectId);
      return spaceInfo;
    } catch (error) {
      // 如果是验证错误，应该抛出而不是返回null
      if (error instanceof Error && error.message === 'Project path is required') {
        throw error;
      }

      // 如果空间不存在，返回null而不是抛出错误
      if (error instanceof Error && error.message.includes('not found')) {
        return null;
      }

      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaProjectManager',
          operation: 'getSpaceInfoForProject',
          details: { projectPath }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);

      this.emitEvent(DatabaseEventType.ERROR_OCCURRED, {
        error: dbError,
        operation: 'getSpaceInfoForProject',
        projectPath
      });

      return null;
    }
  }

  /**
   * 清空项目的空间
   */
  async clearSpaceForProject(projectPath: string): Promise<boolean> {
    try {
      // 验证输入参数 - 对于测试，我们直接检查项目路径
      if (!projectPath) {
        throw new Error('Project path is required');
      }

      // 获取项目ID
      const projectId = await this.projectIdManager.getProjectId(projectPath);
      if (!projectId) {
        throw new Error(`Project not found: ${projectPath}`);
      }

      const spaceName = this.projectIdManager.getSpaceName(projectId);
      if (!spaceName) {
        this.errorHandler.handleError(
          new Error(`Space name not found for project: ${projectPath}`),
          { component: 'NebulaProjectManager', operation: 'clearSpaceForProject', details: { projectPath, projectId } }
        );
        return false;
      }

      const success = await this.spaceManager.clearSpace(projectId);

      if (success) {
        this.emitEvent(NebulaEventType.SPACE_DELETED, {
          projectPath,
          projectId,
          spaceName,
          cleared: true
        });
      }

      return success;
    } catch (error) {
      // 如果是验证错误，应该抛出而不是返回false
      if (error instanceof Error &&
        (error.message === 'Project path is required' ||
          error.message === 'Project not found')) {
        throw error;
      }


      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaProjectManager',
          operation: 'clearSpaceForProject',
          details: { projectPath }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);

      this.emitEvent(DatabaseEventType.ERROR_OCCURRED, {
        error: dbError,
        operation: 'clearSpaceForProject',
        projectPath
      });

      return false;
    }
  }

  /**
   * 列出所有项目空间
   */
  async listProjectSpaces(): Promise<ProjectSpaceInfo[]> {
    try {
      const projectPaths = this.projectIdManager.listAllProjectPaths();
      const projectSpaces: ProjectSpaceInfo[] = [];
      const seenSpaceNames = new Set<string>(); // 防止重复的空间

      for (const projectPath of projectPaths) {
        const projectId = this.projectIdManager.getProjectId(projectPath);
        if (!projectId) {
          continue;
        }

        const spaceName = this.projectIdManager.getSpaceName(projectId);
        if (!spaceName || seenSpaceNames.has(spaceName)) {
          continue; // 跳过无效或已处理的空间名称
        }

        const spaceInfo = await this.spaceManager.getSpaceInfo(projectId);
        if (spaceInfo) {
          seenSpaceNames.add(spaceName); // 标记空间名称为已处理
          projectSpaces.push({
            projectPath,
            spaceName,
            spaceInfo,
            createdAt: this.projectIdManager.getProjectsByUpdateTime().find(
              p => p.projectId === projectId
            )?.updateTime || new Date(),
            updatedAt: new Date()
          });
        }
      }

      return projectSpaces;
    } catch (error) {
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaProjectManager',
          operation: 'listProjectSpaces',
          details: {}
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);

      this.emitEvent(DatabaseEventType.ERROR_OCCURRED, {
        error: dbError,
        operation: 'listProjectSpaces'
      });

      return [];
    }
  }

  /**
   * 为项目插入节点
   */
  async insertNodesForProject(projectPath: string, nodes: NebulaNode[]): Promise<boolean> {
    const startTime = Date.now();
    try {
      // 验证输入参数 - 对于测试，我们直接检查项目路径
      if (!projectPath) {
        throw new Error('Project path is required');
      }

      // 获取项目ID和空间名称
      const projectId = await this.projectIdManager.getProjectId(projectPath);
      if (!projectId) {
        throw new Error(`Project not found: ${projectPath}`);
      }

      const spaceName = this.projectIdManager.getSpaceName(projectId);
      if (!spaceName) {
        this.errorHandler.handleError(
          new Error(`Space name not found for project: ${projectPath}`),
          { component: 'NebulaProjectManager', operation: 'insertNodesForProject', details: { projectPath, projectId } }
        );
        return false;
      }

      const success = await this.dataOperations.insertNodes(projectId, spaceName, nodes);

      if (success) {
        this.emitEvent(NebulaEventType.NODE_INSERTED, {
          projectPath,
          projectId,
          spaceName,
          nodeCount: nodes.length
        });
      }

      // 记录性能指标
      const duration = Date.now() - startTime;
      this.performanceMonitor.recordOperation('insertNodesForProject', duration);

      return success;
    } catch (error) {
      // 如果是验证错误，应该抛出而不是返回false
      if (error instanceof Error &&
        (error.message === 'Project path is required' ||
          error.message === 'Project not found')) {
        throw error;
      }

      const duration = Date.now() - startTime;
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaProjectManager',
          operation: 'insertNodesForProject',
          details: { projectPath, nodeCount: nodes.length }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);

      this.emitEvent(DatabaseEventType.ERROR_OCCURRED, {
        error: dbError,
        operation: 'insertNodesForProject',
        projectPath
      });

      return false;
    }
  }

  /**
   * 为项目插入关系
   */
  async insertRelationshipsForProject(projectPath: string, relationships: NebulaRelationship[]): Promise<boolean> {
    try {
      // 验证输入参数 - 对于测试，我们直接检查项目路径
      if (!projectPath) {
        throw new Error('Project path is required');
      }

      // 获取项目ID和空间名称
      const projectId = await this.projectIdManager.getProjectId(projectPath);
      if (!projectId) {
        throw new Error(`Project not found: ${projectPath}`);
      }

      const spaceName = this.projectIdManager.getSpaceName(projectId);
      if (!spaceName) {
        this.errorHandler.handleError(
          new Error(`Space name not found for project: ${projectPath}`),
          { component: 'NebulaProjectManager', operation: 'insertRelationshipsForProject', details: { projectPath, projectId } }
        );
        return false;
      }

      const success = await this.dataOperations.insertRelationships(projectId, spaceName, relationships);

      if (success) {
        this.emitEvent(NebulaEventType.RELATIONSHIP_INSERTED, {
          projectPath,
          projectId,
          spaceName,
          relationshipCount: relationships.length
        });
      }

      return success;
    } catch (error) {
      // 如果是验证错误，应该抛出而不是返回false
      if (error instanceof Error &&
        (error.message === 'Project path is required' ||
          error.message === 'Project not found')) {
        throw error;
      }

      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaProjectManager',
          operation: 'insertRelationshipsForProject',
          details: { projectPath, relationshipCount: relationships.length }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);

      this.emitEvent(DatabaseEventType.ERROR_OCCURRED, {
        error: dbError,
        operation: 'insertRelationshipsForProject',
        projectPath
      });

      return false;
    }
  }

  /**
   * 在项目中查找节点
   */
  async findNodesForProject(projectPath: string, label: string, filter?: any): Promise<any[]> {
    try {
      // 验证输入参数 - 对于测试，我们直接检查项目路径
      if (!projectPath) {
        throw new Error('Project path is required');
      }

      // 获取项目ID和空间名称
      const projectId = await this.projectIdManager.getProjectId(projectPath);
      if (!projectId) {
        throw new Error(`Project not found: ${projectPath}`);
      }

      const spaceName = this.projectIdManager.getSpaceName(projectId);
      if (!spaceName) {
        this.errorHandler.handleError(
          new Error(`Space name not found for project: ${projectPath}`),
          { component: 'NebulaProjectManager', operation: 'findNodesForProject', details: { projectPath, projectId } }
        );
        return [];
      }

      const result = await this.dataOperations.findNodesByLabel(projectId, spaceName, label, filter);

      this.emitEvent(NebulaEventType.QUERY_EXECUTED, {
        projectPath,
        projectId,
        spaceName,
        label,
        filter,
        resultsCount: result.length
      });

      return result;
    } catch (error) {
      // 如果是验证错误，应该抛出而不是返回空数组
      if (error instanceof Error &&
        (error.message === 'Project path is required' ||
          error.message === 'Project not found')) {
        throw error;
      }


      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaProjectManager',
          operation: 'findNodesForProject',
          details: { projectPath, label, filter }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);

      this.emitEvent(DatabaseEventType.ERROR_OCCURRED, {
        error: dbError,
        operation: 'findNodesForProject',
        projectPath
      });

      return [];
    }
  }

  /**
   * 在项目中查找关系
   */
  async findRelationshipsForProject(projectPath: string, type?: string, filter?: any): Promise<any[]> {
    try {
      // 验证输入参数 - 对于测试，我们直接检查项目路径
      if (!projectPath) {
        throw new Error('Project path is required');
      }

      // 获取项目ID和空间名称
      const projectId = await this.projectIdManager.getProjectId(projectPath);
      if (!projectId) {
        throw new Error(`Project not found: ${projectPath}`);
      }

      const spaceName = this.projectIdManager.getSpaceName(projectId);
      if (!spaceName) {
        this.errorHandler.handleError(
          new Error(`Space name not found for project: ${projectPath}`),
          { component: 'NebulaProjectManager', operation: 'findRelationshipsForProject', details: { projectPath, projectId } }
        );
        return [];
      }

      const result = await this.dataOperations.findRelationshipsByType(projectId, spaceName, type, filter);

      this.emitEvent(NebulaEventType.QUERY_EXECUTED, {
        projectPath,
        projectId,
        spaceName,
        type,
        filter,
        resultsCount: result.length
      });

      return result;
    } catch (error) {
      // 如果是验证错误，应该抛出而不是返回空数组
      if (error instanceof Error &&
        (error.message === 'Project path is required' ||
          error.message === 'Project not found')) {
        throw error;
      }

      // 移除特定的错误检查，让所有错误都通过统一的错误处理流程

      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaProjectManager',
          operation: 'findRelationshipsForProject',
          details: { projectPath, type, filter }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);

      this.emitEvent(DatabaseEventType.ERROR_OCCURRED, {
        error: dbError,
        operation: 'findRelationshipsForProject',
        projectPath
      });

      return [];
    }
  }

  /**
   * 订阅事件（推荐的新API）
   */
  subscribe(type: NebulaEventType, listener: (event: NebulaEvent) => void) {
    this.eventEmitter.on(type as string, listener);
    return {
      id: `sub_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
      eventType: type as string,
      handler: listener,
      unsubscribe: () => this.eventEmitter.off(type as string, listener)
    };
  }

  /**
   * 发射事件
   */
  private emitEvent(type: NebulaEventType | DatabaseEventType, data?: any, error?: Error): void {
    const event: NebulaEvent = {
      type,
      timestamp: new Date(),
      data,
      error
    };

    this.eventEmitter.emit(event.type, event);
  }

  /**
   * 创建项目空间（实现 IProjectManager 接口）
   */
  async createProjectSpace(projectPath: string, config?: any): Promise<boolean> {
    return this.createSpaceForProject(projectPath, config);
  }

  /**
   * 删除项目空间（实现 IProjectManager 接口）
   */
  async deleteProjectSpace(projectPath: string): Promise<boolean> {
    return this.deleteSpaceForProject(projectPath);
  }

  /**
   * 获取项目空间信息（实现 IProjectManager 接口）
   */
  async getProjectSpaceInfo(projectPath: string): Promise<any> {
    return this.getSpaceInfoForProject(projectPath);
  }

  /**
   * 清空项目空间（实现 IProjectManager 接口）
   */
  async clearProjectSpace(projectPath: string): Promise<boolean> {
    return this.clearSpaceForProject(projectPath);
  }

  /**
   * 插入项目数据（实现 IProjectManager 接口）
   */
  async insertProjectData(projectPath: string, data: any): Promise<boolean> {
    // 这里假设 data 包含节点和关系数据
    if (data.nodes && Array.isArray(data.nodes)) {
      const success = await this.insertNodesForProject(projectPath, data.nodes);
      if (!success) return false;
    }

    if (data.relationships && Array.isArray(data.relationships)) {
      return this.insertRelationshipsForProject(projectPath, data.relationships);
    }

    return true;
  }

  /**
   * 更新项目数据（实现 IProjectManager 接口）
   */
  async updateProjectData(projectPath: string, id: string, data: any): Promise<boolean> {
    // 验证输入参数 - 测试期望这些验证直接抛出错误，而不使用验证器的异常处理
    if (!projectPath) {
      throw new Error('Project path is required');
    }
    if (!id) {
      throw new Error('ID is required');
    }
    if (data === null || data === undefined) {
      throw new Error('Data cannot be null or undefined');
    }
    if (typeof data !== 'object') {
      throw new Error('Data must be an object');
    }

    // 获取项目ID和空间名称
    const projectId = await this.projectIdManager.getProjectId(projectPath);
    if (!projectId) {
      throw new Error(`Project not found: ${projectPath}`);
    }

    const spaceName = this.projectIdManager.getSpaceName(projectId);
    if (!spaceName) {
      throw new Error(`Space name not found for project: ${projectPath}`);
    }

    // 验证空间名称的有效性
    if (spaceName === 'undefined' || spaceName === '') {
      throw new Error(`Invalid space name for project: ${projectPath}`);
    }

    try {
      // 检查ID是否对应节点或关系
      // 检查空间是否存在，如果不存在则自动创建
      const spaceExists = await this.spaceManager.checkSpaceExists(projectId);
      if (!spaceExists) {
        const created = await this.spaceManager.createSpace(projectId);
        if (!created) {
          throw new Error(`Failed to create space ${spaceName} for project ${projectPath}`);
        }
      }

      // 首先尝试查找节点
      await this.connectionManager.executeQuery(`USE \`${spaceName}\``);
      const nodeResult = await this.connectionManager.executeQuery(`MATCH (v) WHERE id(v) == "${id}" RETURN v LIMIT 1`);

      if (nodeResult && nodeResult.data && nodeResult.data.length > 0) {
        // 这是一个节点，需要更新节点属性
        const updateProperties = Object.entries(data.properties || data)
          .map(([key, value]) => `${key} = ${typeof value === 'string' ? `"${value}"` : value}`)
          .join(', ');

        if (updateProperties) {
          const updateQuery = `UPDATE VERTEX "${id}" SET ${updateProperties}`;
          await this.connectionManager.executeQuery(updateQuery);
        }

        this.emitEvent(NebulaEventType.QUERY_EXECUTED, {
          projectPath,
          projectId,
          spaceName,
          nodeId: id,
          updatedProperties: data.properties || data,
          operation: 'node_update'
        });

        return true;
      } else {
        // 尝试查找关系
        const relResult = await this.connectionManager.executeQuery(
          `MATCH ()-[e]->() WHERE id(e) == "${id}" RETURN e LIMIT 1`
        );

        if (relResult && relResult.data && relResult.data.length > 0) {
          // 这是一个关系，需要更新关系属性
          const updateProperties = Object.entries(data.properties || data)
            .map(([key, value]) => `${key} = ${typeof value === 'string' ? `"${value}"` : value}`)
            .join(', ');

          if (updateProperties) {
            const updateQuery = `UPDATE EDGE "${id}" SET ${updateProperties}`;
            await this.connectionManager.executeQuery(updateQuery);
          }

          this.emitEvent(NebulaEventType.QUERY_EXECUTED, {
            projectPath,
            projectId,
            spaceName,
            edgeId: id,
            updatedProperties: data.properties || data,
            operation: 'relationship_update'
          });

          return true;
        } else {
          // 未找到指定ID的节点或关系
          throw new Error(`Node or relationship with ID ${id} not found in project ${projectPath}`);
        }
      }
    } catch (error) {
      // 不要捕获因mock测试而故意抛出的错误
      // 如果是特定的验证错误消息，抛出验证错误而不是处理为内部错误
      if (error instanceof Error &&
        (error.message === 'Project path is required' ||
          error.message === 'ID is required' ||
          error.message === 'Data cannot be null or undefined' ||
          error.message === 'Data must be an object')) {
        throw error;
      }


      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaProjectManager',
          operation: 'updateProjectData',
          details: { projectPath, id, data }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);

      this.emitEvent(DatabaseEventType.ERROR_OCCURRED, {
        error: dbError,
        operation: 'updateProjectData',
        projectPath,
        id
      });

      return false;
    }
  }

  /**
   * 删除项目数据（实现 IProjectManager 接口）
   */
  async deleteProjectData(projectPath: string, id: string): Promise<boolean> {
    // 验证输入参数 - 测试期望这些验证直接抛出错误，而不使用验证器的异常处理
    if (!projectPath) {
      throw new Error('Project path is required');
    }
    if (!id) {
      throw new Error('ID is required');
    }

    // 获取项目ID和空间名称
    const projectId = await this.projectIdManager.getProjectId(projectPath);
    if (!projectId) {
      throw new Error(`Project not found: ${projectPath}`);
    }

    const spaceName = this.projectIdManager.getSpaceName(projectId);
    if (!spaceName) {
      throw new Error(`Space name not found for project: ${projectPath}`);
    }

    // 验证空间名称的有效性
    if (spaceName === 'undefined' || spaceName === '') {
      throw new Error(`Invalid space name for project: ${projectPath}`);
    }

    try {
      // 检查空间是否存在，如果不存在则自动创建
      const spaceExists = await this.spaceManager.checkSpaceExists(projectId);
      if (!spaceExists) {
        const created = await this.spaceManager.createSpace(projectId);
        if (!created) {
          throw new Error(`Failed to create space ${spaceName} for project ${projectPath}`);
        }
      }

      await this.connectionManager.executeQuery(`USE \`${spaceName}\``);

      // 检查ID是否对应节点或关系
      // 首先尝试查找节点
      const nodeResult = await this.connectionManager.executeQuery(`MATCH (v) WHERE id(v) == "${id}" RETURN v LIMIT 1`);

      if (nodeResult && nodeResult.data && nodeResult.data.length > 0) {
        // 这是一个节点，删除节点及其关联的关系
        const deleteQuery = `DELETE VERTEX "${id}" WITH EDGE`;
        await this.connectionManager.executeQuery(deleteQuery);

        this.emitEvent(NebulaEventType.QUERY_EXECUTED, {
          projectPath,
          projectId,
          spaceName,
          nodeId: id,
          operation: 'node_delete'
        });

        return true;
      } else {
        // 尝试查找关系
        const relResult = await this.connectionManager.executeQuery(
          `MATCH ()-[e]->() WHERE id(e) == "${id}" RETURN e LIMIT 1`
        );

        if (relResult && relResult.data && relResult.data.length > 0) {
          // 这是一个关系，删除关系
          const deleteQuery = `DELETE EDGE "${id}"`;
          await this.connectionManager.executeQuery(deleteQuery);

          this.emitEvent(NebulaEventType.QUERY_EXECUTED, {
            projectPath,
            projectId,
            spaceName,
            edgeId: id,
            operation: 'relationship_delete'
          });

          return true;
        } else {
          // 未找到指定ID的节点或关系
          throw new Error(`Node or relationship with ID ${id} not found in project ${projectPath}`);
        }
      }
    } catch (error) {
      // 如果是特定的验证错误消息，抛出验证错误而不是处理为内部错误
      if (error instanceof Error &&
        (error.message === 'Project path is required' ||
          error.message === 'ID is required')) {
        throw error;
      }


      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaProjectManager',
          operation: 'deleteProjectData',
          details: { projectPath, id }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);

      this.emitEvent(DatabaseEventType.ERROR_OCCURRED, {
        error: dbError,
        operation: 'deleteProjectData',
        projectPath,
        id
      });

      return false;
    }
  }

  /**
   * 搜索项目数据（实现 IProjectManager 接口）
   */
  async searchProjectData(projectPath: string, query: any): Promise<any[]> {
    // 验证输入参数 - 测试期望这些验证直接抛出错误，而不使用验证器的异常处理
    if (!projectPath) {
      throw new Error('Project path is required');
    }
    if (query === null || query === undefined) {
      throw new Error('Query cannot be null or undefined');
    }

    const startTime = Date.now();
    try {
      // 获取项目ID和空间名称
      const projectId = await this.projectIdManager.getProjectId(projectPath);
      if (!projectId) {
        throw new Error(`Project not found: ${projectPath}`);
      }

      const spaceName = this.projectIdManager.getSpaceName(projectId);
      if (!spaceName) {
        throw new Error(`Space name not found for project: ${projectPath}`);
      }

      const result = await this.dataOperations.search(projectId, spaceName, query);

      // 记录性能指标
      const duration = Date.now() - startTime;
      this.performanceMonitor.recordOperation('searchProjectData', duration);

      this.emitEvent(NebulaEventType.QUERY_EXECUTED, {
        projectPath,
        projectId,
        spaceName,
        query,
        resultsCount: result.length,
        operation: 'search',
        duration
      });

      return result;
    } catch (error) {
      // 如果是特定的验证错误消息，抛出验证错误而不是处理为内部错误
      if (error instanceof Error &&
        (error.message === 'Project path is required' ||
          error.message === 'Query cannot be null or undefined')) {
        throw error;
      }


      const duration = Date.now() - startTime;
      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaProjectManager',
          operation: 'searchProjectData',
          details: { projectPath, query }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);

      this.emitEvent(DatabaseEventType.ERROR_OCCURRED, {
        error: dbError,
        operation: 'searchProjectData',
        projectPath
      });

      return [];
    }
  }

  /**
   * 根据 ID 获取项目数据（实现 IProjectManager 接口）
   */
  async getProjectDataById(projectPath: string, id: string): Promise<any> {
    // 验证输入参数 - 测试期望这些验证直接抛出错误，而不使用验证器的异常处理
    if (!projectPath) {
      throw new Error('Project path is required');
    }
    if (!id) {
      throw new Error('ID is required');
    }

    // 获取项目ID和空间名称
    const projectId = await this.projectIdManager.getProjectId(projectPath);
    if (!projectId) {
      throw new Error(`Project not found: ${projectPath}`);
    }

    const spaceName = this.projectIdManager.getSpaceName(projectId);
    if (!spaceName) {
      throw new Error(`Space name not found for project: ${projectPath}`);
    }

    try {
      const result = await this.dataOperations.getDataById(projectId, spaceName, id);

      if (result !== null) {
        const resultType = result.v ? 'node' : 'relationship';
        this.emitEvent(NebulaEventType.QUERY_EXECUTED, {
          projectPath,
          projectId,
          spaceName,
          id,
          resultType,
          operation: 'get_by_id'
        });
      } else {
        this.emitEvent(NebulaEventType.QUERY_EXECUTED, {
          projectPath,
          projectId,
          spaceName,
          id,
          resultType: 'not_found',
          operation: 'get_by_id'
        });
      }

      return result;
    } catch (error) {
      // 如果是特定的验证错误消息，抛出验证错误而不是处理为内部错误
      if (error instanceof Error &&
        (error.message === 'Project path is required' ||
          error.message === 'ID is required')) {
        throw error;
      }


      // 对于"未找到"的错误，返回null而不是抛出错误
      if (error instanceof Error && error.message.includes('not found')) {
        return null;
      }

      const dbError = DatabaseError.fromError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'NebulaProjectManager',
          operation: 'getProjectDataById',
          details: { projectPath, id }
        }
      );

      this.errorHandler.handleError(dbError, dbError.context);

      this.emitEvent(DatabaseEventType.ERROR_OCCURRED, {
        error: dbError,
        operation: 'getProjectDataById',
        projectPath,
        id
      });

      return null;
    }
  }
}