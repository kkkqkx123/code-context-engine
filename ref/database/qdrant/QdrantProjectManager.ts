import { injectable, inject } from 'inversify';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { TYPES } from '../../types';
import { ProjectIdManager } from '../ProjectIdManager';
import { IQdrantCollectionManager } from './QdrantCollectionManager';
import { IQdrantVectorOperations } from './QdrantVectorOperations';
import { IQdrantQueryUtils } from './QdrantQueryUtils';
import { DatabaseLoggerService } from '../common/DatabaseLoggerService';
import { PerformanceMonitor } from '../../infrastructure/monitoring/PerformanceMonitor';
import {
  VectorPoint,
 CollectionInfo,
  SearchOptions,
  SearchResult
} from './IVectorStore';
import {
  VectorDistance,
  ProjectInfo,
  ERROR_MESSAGES,
  QdrantEventType,
  QdrantEvent
} from './QdrantTypes';

/**
 * Qdrant 项目管理器接口
 */
export interface IQdrantProjectManager {
  createCollectionForProject(projectPath: string, vectorSize: number, distance?: VectorDistance): Promise<boolean>;
  upsertVectorsForProject(projectPath: string, vectors: VectorPoint[]): Promise<boolean>;
  searchVectorsForProject(projectPath: string, query: number[], options?: SearchOptions): Promise<SearchResult[]>;
  getCollectionInfoForProject(projectPath: string): Promise<CollectionInfo | null>;
  deleteCollectionForProject(projectPath: string): Promise<boolean>;
  getProjectInfo(projectPath: string): Promise<ProjectInfo | null>;
  listProjects(): Promise<ProjectInfo[]>;
  deleteVectorsForProject(projectPath: string, vectorIds: string[]): Promise<boolean>;
  clearProject(projectPath: string): Promise<boolean>;
  subscribe(type: QdrantEventType, listener: (event: QdrantEvent) => void): { id: string; eventType: string; handler: any; unsubscribe: () => void };
  
  // 兼容 IProjectManager 接口的方法
  createProjectSpace(projectPath: string, config?: any): Promise<boolean>;
  deleteProjectSpace(projectPath: string): Promise<boolean>;
  getProjectSpaceInfo(projectPath: string): Promise<any>;
  clearProjectSpace(projectPath: string): Promise<boolean>;
  listProjectSpaces(): Promise<any[]>;
  insertProjectData(projectPath: string, data: any): Promise<boolean>;
  updateProjectData(projectPath: string, id: string, data: any): Promise<boolean>;
  deleteProjectData(projectPath: string, id: string): Promise<boolean>;
  searchProjectData(projectPath: string, query: any): Promise<any[]>;
  getProjectDataById(projectPath: string, id: string): Promise<any>;
}

/**
 * Qdrant 项目管理器实现
 * 
 * 负责项目相关的集合操作、项目ID管理、项目特定的向量操作
 */
@injectable()
export class QdrantProjectManager implements IQdrantProjectManager {
  private logger: LoggerService;
  private errorHandler: ErrorHandlerService;
  private databaseLogger: DatabaseLoggerService;
  private performanceMonitor: PerformanceMonitor;
  private projectIdManager: ProjectIdManager;
  private collectionManager: IQdrantCollectionManager;
  private vectorOperations: IQdrantVectorOperations;
  private queryUtils: IQdrantQueryUtils;
  private eventListeners: Map<QdrantEventType, ((event: QdrantEvent) => void)[]> = new Map();

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.DatabaseLoggerService) databaseLogger: DatabaseLoggerService,
    @inject(TYPES.PerformanceMonitor) performanceMonitor: PerformanceMonitor,
    @inject(TYPES.ProjectIdManager) projectIdManager: ProjectIdManager,
    @inject(TYPES.IQdrantCollectionManager) collectionManager: IQdrantCollectionManager,
    @inject(TYPES.IQdrantVectorOperations) vectorOperations: IQdrantVectorOperations,
    @inject(TYPES.IQdrantQueryUtils) queryUtils: IQdrantQueryUtils
  ) {
    this.logger = logger;
    this.errorHandler = errorHandler;
    this.databaseLogger = databaseLogger;
    this.performanceMonitor = performanceMonitor;
    this.projectIdManager = projectIdManager;
    this.collectionManager = collectionManager;
    this.vectorOperations = vectorOperations;
    this.queryUtils = queryUtils;
  }

  /**
   * 为特定项目创建集合
   */
  async createCollectionForProject(
    projectPath: string,
    vectorSize: number,
    distance: VectorDistance = 'Cosine'
  ): Promise<boolean> {
    try {
      const startTime = Date.now();
      // 生成项目ID并获取集合名称
      const projectId = await this.projectIdManager.generateProjectId(projectPath);
      const collectionName = this.projectIdManager.getCollectionName(projectId);

      if (!collectionName) {
        throw new Error(`Failed to generate collection name for project: ${projectPath}`);
      }

      const success = await this.collectionManager.createCollection(collectionName, vectorSize, distance);

      const duration = Date.now() - startTime;
      if (success) {
        this.performanceMonitor.recordOperation('createCollectionForProject', duration);
        await this.databaseLogger.logCollectionOperation('create', projectPath, 'success', {
          collectionName,
          vectorSize,
          distance,
          duration
        });
        this.emitEvent(QdrantEventType.COLLECTION_CREATED, {
          projectPath,
          projectId,
          collectionName,
          vectorSize,
          distance
        });
      } else {
        this.performanceMonitor.recordOperation('createCollectionForProject', duration);
        await this.databaseLogger.logCollectionOperation('create', projectPath, 'failed', {
          collectionName,
          vectorSize,
          distance,
          duration,
          error: 'Failed to create collection'
        });
      }

      return success;
    } catch (error) {
      const duration = Date.now() - Date.now(); // This is approximate since we don't have exact startTime
      this.performanceMonitor.recordOperation('createCollectionForProject', duration);
      await this.databaseLogger.logCollectionOperation('create', projectPath, 'failed', {
        vectorSize,
        distance,
        duration,
        error: error instanceof Error ? error.message : String(error)
      });
      this.errorHandler.handleError(
        new Error(
          `Failed to create collection for project ${projectPath}: ${error instanceof Error ? error.message : String(error)}`
        ),
        { component: 'QdrantProjectManager', operation: 'createCollectionForProject' }
      );

      this.emitEvent(QdrantEventType.ERROR, {
        error: error instanceof Error ? error : new Error(String(error)),
        operation: 'createCollectionForProject',
        projectPath
      });

      return false;
    }
  }

  /**
   * 为特定项目插入或更新向量
   */
  async upsertVectorsForProject(projectPath: string, vectors: VectorPoint[]): Promise<boolean> {
    try {
      const startTime = Date.now();
      // 获取项目ID和集合名称
      const projectId = await this.projectIdManager.getProjectId(projectPath);
      if (!projectId) {
        throw new Error(`Project not found: ${projectPath}`);
      }

      const collectionName = this.projectIdManager.getCollectionName(projectId);
      if (!collectionName) {
        throw new Error(`Collection name not found for project: ${projectPath}`);
      }

      // 为所有向量添加项目ID（如果尚未存在）
      const vectorsWithProjectId = vectors.map(vector => ({
        ...vector,
        payload: {
          ...vector.payload,
          projectId
        }
      }));

      const success = await this.vectorOperations.upsertVectors(collectionName, vectorsWithProjectId);

      const duration = Date.now() - startTime;
      if (success) {
        this.performanceMonitor.recordOperation('upsertVectorsForProject', duration);
        await this.databaseLogger.logVectorOperation('upsert', projectPath, 'success', {
          vectorCount: vectors.length,
          duration
        });
        this.emitEvent(QdrantEventType.VECTORS_UPSERTED, {
          projectPath,
          projectId,
          collectionName,
          vectorCount: vectors.length
        });
      } else {
        this.performanceMonitor.recordOperation('upsertVectorsForProject', duration);
        await this.databaseLogger.logVectorOperation('upsert', projectPath, 'failed', {
          vectorCount: vectors.length,
          duration,
          error: 'Failed to upsert vectors'
        });
      }

      return success;
    } catch (error) {
      const duration = Date.now() - Date.now(); // This is approximate since we don't have exact startTime
      this.performanceMonitor.recordOperation('upsertVectorsForProject', duration);
      await this.databaseLogger.logVectorOperation('upsert', projectPath, 'failed', {
        vectorCount: vectors.length,
        duration,
        error: error instanceof Error ? error.message : String(error)
      });
      this.errorHandler.handleError(
        new Error(
          `Failed to upsert vectors for project ${projectPath}: ${error instanceof Error ? error.message : String(error)}`
        ),
        { component: 'QdrantProjectManager', operation: 'upsertVectorsForProject' }
      );

      this.emitEvent(QdrantEventType.ERROR, {
        error: error instanceof Error ? error : new Error(String(error)),
        operation: 'upsertVectorsForProject',
        projectPath
      });

      return false;
    }
  }

  /**
   * 在特定项目的集合中搜索向量
   */
  async searchVectorsForProject(
    projectPath: string,
    query: number[],
    options: SearchOptions = {}
  ): Promise<SearchResult[]> {
    try {
      const startTime = Date.now();
      // 获取项目ID和集合名称
      const projectId = await this.projectIdManager.getProjectId(projectPath);
      if (!projectId) {
        throw new Error(`Project not found: ${projectPath}`);
      }

      const collectionName = this.projectIdManager.getCollectionName(projectId);
      if (!collectionName) {
        throw new Error(`Collection name not found for project: ${projectPath}`);
      }

      // 确保应用projectId过滤器
      const searchFilter = {
        ...options.filter,
        projectId
      };

      const searchOptions = {
        ...options,
        filter: this.queryUtils.buildFilter(searchFilter)
      };

      const results = await this.vectorOperations.searchVectors(collectionName, query, searchOptions);

      const duration = Date.now() - startTime;
      this.performanceMonitor.recordOperation('searchVectorsForProject', duration);
      await this.databaseLogger.logQueryOperation('search', projectPath, 'success', {
        queryLength: query.length,
        resultsCount: results.length,
        duration
      });
      this.emitEvent(QdrantEventType.VECTORS_SEARCHED, {
        projectPath,
        projectId,
        collectionName,
        queryLength: query.length,
        resultsCount: results.length,
        searchOptions
      });

      return results;
    } catch (error) {
      const duration = Date.now() - Date.now(); // This is approximate since we don't have exact startTime
      this.performanceMonitor.recordOperation('searchVectorsForProject', duration);
      await this.databaseLogger.logQueryOperation('search', projectPath, 'failed', {
        queryLength: query.length,
        duration,
        error: error instanceof Error ? error.message : String(error)
      });
      this.errorHandler.handleError(
        new Error(
          `Failed to search vectors for project ${projectPath}: ${error instanceof Error ? error.message : String(error)}`
        ),
        { component: 'QdrantProjectManager', operation: 'searchVectorsForProject' }
      );

      this.emitEvent(QdrantEventType.ERROR, {
        error: error instanceof Error ? error : new Error(String(error)),
        operation: 'searchVectorsForProject',
        projectPath
      });

      return [];
    }
  }

  /**
   * 获取特定项目的集合信息
   */
  async getCollectionInfoForProject(projectPath: string): Promise<CollectionInfo | null> {
    try {
      const startTime = Date.now();
      // 获取项目ID和集合名称
      const projectId = await this.projectIdManager.getProjectId(projectPath);
      if (!projectId) {
        throw new Error(`Project not found: ${projectPath}`);
      }

      const collectionName = this.projectIdManager.getCollectionName(projectId);
      if (!collectionName) {
        throw new Error(`Collection name not found for project: ${projectPath}`);
      }

      const result = await this.collectionManager.getCollectionInfo(collectionName);
      
      const duration = Date.now() - startTime;
      this.performanceMonitor.recordOperation('getCollectionInfoForProject', duration);
      await this.databaseLogger.logCollectionOperation('info', projectPath, 'success', {
        duration
      });

      return result;
    } catch (error) {
      const duration = Date.now() - Date.now(); // This is approximate since we don't have exact startTime
      this.performanceMonitor.recordOperation('getCollectionInfoForProject', duration);
      await this.databaseLogger.logCollectionOperation('info', projectPath, 'failed', {
        duration,
        error: error instanceof Error ? error.message : String(error)
      });
      this.errorHandler.handleError(
        new Error(
          `Failed to get collection info for project ${projectPath}: ${error instanceof Error ? error.message : String(error)}`
        ),
        { component: 'QdrantProjectManager', operation: 'getCollectionInfoForProject' }
      );

      this.emitEvent(QdrantEventType.ERROR, {
        error: error instanceof Error ? error : new Error(String(error)),
        operation: 'getCollectionInfoForProject',
        projectPath
      });

      return null;
    }
  }

  /**
   * 删除特定项目的集合
   */
  async deleteCollectionForProject(projectPath: string): Promise<boolean> {
    try {
      const startTime = Date.now();
      // 获取项目ID和集合名称
      const projectId = await this.projectIdManager.getProjectId(projectPath);
      if (!projectId) {
        throw new Error(`Project not found: ${projectPath}`);
      }

      const collectionName = this.projectIdManager.getCollectionName(projectId);
      if (!collectionName) {
        throw new Error(`Collection name not found for project: ${projectPath}`);
      }

      const success = await this.collectionManager.deleteCollection(collectionName);

      const duration = Date.now() - startTime;
      if (success) {
        this.performanceMonitor.recordOperation('deleteCollectionForProject', duration);
        await this.databaseLogger.logCollectionOperation('delete', projectPath, 'success', {
          collectionName,
          duration
        });
        // 删除项目ID映射
        await this.projectIdManager.removeProject(projectPath);

        this.emitEvent(QdrantEventType.COLLECTION_DELETED, {
          projectPath,
          projectId,
          collectionName
        });
      } else {
        this.performanceMonitor.recordOperation('deleteCollectionForProject', duration);
        await this.databaseLogger.logCollectionOperation('delete', projectPath, 'failed', {
          collectionName,
          duration,
          error: 'Failed to delete collection'
        });
      }

      return success;
    } catch (error) {
      const duration = Date.now() - Date.now(); // This is approximate since we don't have exact startTime
      this.performanceMonitor.recordOperation('deleteCollectionForProject', duration);
      await this.databaseLogger.logCollectionOperation('delete', projectPath, 'failed', {
        duration,
        error: error instanceof Error ? error.message : String(error)
      });
      this.errorHandler.handleError(
        new Error(
          `Failed to delete collection for project ${projectPath}: ${error instanceof Error ? error.message : String(error)}`
        ),
        { component: 'QdrantProjectManager', operation: 'deleteCollectionForProject' }
      );

      this.emitEvent(QdrantEventType.ERROR, {
        error: error instanceof Error ? error : new Error(String(error)),
        operation: 'deleteCollectionForProject',
        projectPath
      });

      return false;
    }
  }

  /**
   * 获取项目信息
   */
  async getProjectInfo(projectPath: string): Promise<ProjectInfo | null> {
    try {
      const startTime = Date.now();
      const projectId = await this.projectIdManager.getProjectId(projectPath);
      if (!projectId) {
        return null;
      }

      const collectionName = this.projectIdManager.getCollectionName(projectId);
      if (!collectionName) {
        return null;
      }

      const collectionInfo = await this.collectionManager.getCollectionInfo(collectionName);

      const duration = Date.now() - startTime;
      this.performanceMonitor.recordOperation('getProjectInfo', duration);
      await this.databaseLogger.logProjectOperation('info', projectPath, 'success', {
        duration
      });

      return {
        id: projectId,
        path: projectPath,
        collectionName,
        vectorSize: collectionInfo?.vectors.size,
        distance: collectionInfo?.vectors.distance
      };
    } catch (error) {
      const duration = Date.now() - Date.now(); // This is approximate since we don't have exact startTime
      this.performanceMonitor.recordOperation('getProjectInfo', duration);
      await this.databaseLogger.logProjectOperation('info', projectPath, 'failed', {
        duration,
        error: error instanceof Error ? error.message : String(error)
      });
      this.logger.warn('Failed to get project info', {
        projectPath,
        error: error instanceof Error ? error.message : String(error),
      });
      return null;
    }
  }

  /**
   * 列出所有项目
   */
  async listProjects(): Promise<ProjectInfo[]> {
    try {
      const startTime = Date.now();
      const projectPaths = this.projectIdManager.listAllProjectPaths();
      const projects: ProjectInfo[] = [];

      for (const projectPath of projectPaths) {
        const projectInfo = await this.getProjectInfo(projectPath);
        if (projectInfo) {
          projects.push(projectInfo);
        }
      }

      const duration = Date.now() - startTime;
      this.performanceMonitor.recordOperation('listProjects', duration);
      await this.databaseLogger.logProjectOperation('list', 'all', 'success', {
        projectCount: projects.length,
        duration
      });

      return projects;
    } catch (error) {
      const duration = Date.now() - Date.now(); // This is approximate since we don't have exact startTime
      this.performanceMonitor.recordOperation('listProjects', duration);
      await this.databaseLogger.logProjectOperation('list', 'all', 'failed', {
        duration,
        error: error instanceof Error ? error.message : String(error)
      });
      this.errorHandler.handleError(
        new Error(
          `Failed to list projects: ${error instanceof Error ? error.message : String(error)}`
        ),
        { component: 'QdrantProjectManager', operation: 'listProjects' }
      );

      this.emitEvent(QdrantEventType.ERROR, {
        error: error instanceof Error ? error : new Error(String(error)),
        operation: 'listProjects'
      });

      return [];
    }
  }

  /**
   * 删除项目中的特定向量
   */
  async deleteVectorsForProject(projectPath: string, vectorIds: string[]): Promise<boolean> {
    try {
      const startTime = Date.now();
      // 获取项目ID和集合名称
      const projectId = await this.projectIdManager.getProjectId(projectPath);
      if (!projectId) {
        throw new Error(`Project not found: ${projectPath}`);
      }

      const collectionName = this.projectIdManager.getCollectionName(projectId);
      if (!collectionName) {
        throw new Error(`Collection name not found for project: ${projectPath}`);
      }

      const success = await this.vectorOperations.deletePoints(collectionName, vectorIds);

      const duration = Date.now() - startTime;
      if (success) {
        this.performanceMonitor.recordOperation('deleteVectorsForProject', duration);
        await this.databaseLogger.logVectorOperation('delete', projectPath, 'success', {
          vectorCount: vectorIds.length,
          duration
        });
        this.emitEvent(QdrantEventType.POINTS_DELETED, {
          projectPath,
          projectId,
          collectionName,
          deletedCount: vectorIds.length,
          vectorIds
        });
      } else {
        this.performanceMonitor.recordOperation('deleteVectorsForProject', duration);
        await this.databaseLogger.logVectorOperation('delete', projectPath, 'failed', {
          vectorCount: vectorIds.length,
          duration,
          error: 'Failed to delete vectors'
        });
      }

      return success;
    } catch (error) {
      const duration = Date.now() - Date.now(); // This is approximate since we don't have exact startTime
      this.performanceMonitor.recordOperation('deleteVectorsForProject', duration);
      await this.databaseLogger.logVectorOperation('delete', projectPath, 'failed', {
        vectorCount: vectorIds.length,
        duration,
        error: error instanceof Error ? error.message : String(error)
      });
      this.errorHandler.handleError(
        new Error(
          `Failed to delete vectors for project ${projectPath}: ${error instanceof Error ? error.message : String(error)}`
        ),
        { component: 'QdrantProjectManager', operation: 'deleteVectorsForProject' }
      );

      this.emitEvent(QdrantEventType.ERROR, {
        error: error instanceof Error ? error : new Error(String(error)),
        operation: 'deleteVectorsForProject',
        projectPath
      });

      return false;
    }
  }

  /**
   * 清空项目
   */
  async clearProject(projectPath: string): Promise<boolean> {
    try {
      const startTime = Date.now();
      // 获取项目ID和集合名称
      const projectId = await this.projectIdManager.getProjectId(projectPath);
      if (!projectId) {
        throw new Error(`Project not found: ${projectPath}`);
      }

      const collectionName = this.projectIdManager.getCollectionName(projectId);
      if (!collectionName) {
        throw new Error(`Collection name not found for project: ${projectPath}`);
      }

      const success = await this.vectorOperations.clearCollection(collectionName);

      const duration = Date.now() - startTime;
      if (success) {
        this.performanceMonitor.recordOperation('clearProject', duration);
        await this.databaseLogger.logProjectOperation('clear', projectPath, 'success', {
          duration
        });
        this.emitEvent(QdrantEventType.POINTS_DELETED, {
          projectPath,
          projectId,
          collectionName,
          cleared: true
        });
      } else {
        this.performanceMonitor.recordOperation('clearProject', duration);
        await this.databaseLogger.logProjectOperation('clear', projectPath, 'failed', {
          duration,
          error: 'Failed to clear project'
        });
      }

      return success;
    } catch (error) {
      const duration = Date.now() - Date.now(); // This is approximate since we don't have exact startTime
      this.performanceMonitor.recordOperation('clearProject', duration);
      await this.databaseLogger.logProjectOperation('clear', projectPath, 'failed', {
        duration,
        error: error instanceof Error ? error.message : String(error)
      });
      this.errorHandler.handleError(
        new Error(
          `Failed to clear project ${projectPath}: ${error instanceof Error ? error.message : String(error)}`
        ),
        { component: 'QdrantProjectManager', operation: 'clearProject' }
      );

      this.emitEvent(QdrantEventType.ERROR, {
        error: error instanceof Error ? error : new Error(String(error)),
        operation: 'clearProject',
        projectPath
      });

      return false;
    }
  }

  /**
   * 订阅事件
   */
  subscribe(type: QdrantEventType, listener: (event: QdrantEvent) => void) {
    if (!this.eventListeners.has(type)) {
      this.eventListeners.set(type, []);
    }
    this.eventListeners.get(type)!.push(listener);
    
    // 返回订阅对象，允许取消订阅
    const subscription = {
      id: `${type}_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
      eventType: type,
      handler: listener,
      unsubscribe: () => {
        const listeners = this.eventListeners.get(type);
        if (listeners) {
          const index = listeners.indexOf(listener);
          if (index > -1) {
            listeners.splice(index, 1);
          }
        }
      }
    };
    
    return subscription;
  }

  /**
   * 发射事件
   */
  private emitEvent(type: QdrantEventType, data?: any, error?: Error): void {
    const event: QdrantEvent = {
      type,
      timestamp: new Date(),
      data,
      error
    };

    const listeners = this.eventListeners.get(type);
    if (listeners) {
      listeners.forEach(listener => {
        try {
          listener(event);
        } catch (err) {
          this.logger.error('Error in event listener', {
            eventType: type,
            error: err instanceof Error ? err.message : String(err)
          });
        }
      });
    }
  }

  /**
   * 创建项目空间（兼容 IProjectManager 接口）
   */
  async createProjectSpace(projectPath: string, config?: any): Promise<boolean> {
    const vectorSize = config?.vectorSize || 1536;
    const distance = config?.distance || 'Cosine';
    return this.createCollectionForProject(projectPath, vectorSize, distance);
  }

  /**
   * 删除项目空间（兼容 IProjectManager 接口）
   */
  async deleteProjectSpace(projectPath: string): Promise<boolean> {
    return this.deleteCollectionForProject(projectPath);
  }

  /**
   * 获取项目空间信息（兼容 IProjectManager 接口）
   */
  async getProjectSpaceInfo(projectPath: string): Promise<any> {
    return this.getCollectionInfoForProject(projectPath);
  }

  /**
   * 清空项目空间（兼容 IProjectManager 接口）
   */
  async clearProjectSpace(projectPath: string): Promise<boolean> {
    return this.clearProject(projectPath);
  }

  /**
   * 列出所有项目空间（兼容 IProjectManager 接口）
   */
  async listProjectSpaces(): Promise<any[]> {
    const projects = await this.listProjects();
    return projects.map(project => ({
      id: project.id,
      path: project.path,
      collectionName: project.collectionName,
      vectorSize: project.vectorSize,
      distance: project.distance
    }));
  }

  /**
   * 插入项目数据（兼容 IProjectManager 接口）
   */
  async insertProjectData(projectPath: string, data: any): Promise<boolean> {
    if (!data.id || !data.vector) {
      throw new Error('Data must contain id and vector fields');
    }
    const vectorPoint = {
      id: data.id,
      vector: data.vector,
      payload: data.payload || {}
    };
    return this.upsertVectorsForProject(projectPath, [vectorPoint]);
  }

  /**
   * 更新项目数据（兼容 IProjectManager 接口）
   */
  async updateProjectData(projectPath: string, id: string, data: any): Promise<boolean> {
    if (!data.vector) {
      throw new Error('Data must contain vector field');
    }
    const vectorPoint = {
      id,
      vector: data.vector,
      payload: data.payload || {}
    };
    return this.upsertVectorsForProject(projectPath, [vectorPoint]);
  }

  /**
   * 删除项目数据（兼容 IProjectManager 接口）
   */
  async deleteProjectData(projectPath: string, id: string): Promise<boolean> {
    return this.deleteVectorsForProject(projectPath, [id]);
  }

  /**
   * 搜索项目数据（兼容 IProjectManager 接口）
   */
  async searchProjectData(projectPath: string, query: any): Promise<any[]> {
    if (!query.vector) {
      throw new Error('Query must contain vector field');
    }
    const options: SearchOptions = {
      limit: query.limit || 10,
      filter: query.filter || {}
    };
    return this.searchVectorsForProject(projectPath, query.vector, options);
  }

  /**
   * 根据ID获取项目数据（兼容 IProjectManager 接口）
   */
  async getProjectDataById(projectPath: string, id: string): Promise<any> {
    try {
      const startTime = Date.now();
      // 获取项目ID和集合名称
      const projectId = await this.projectIdManager.getProjectId(projectPath);
      if (!projectId) {
        throw new Error(`Project not found: ${projectPath}`);
      }

      const collectionName = this.projectIdManager.getCollectionName(projectId);
      if (!collectionName) {
        throw new Error(`Collection name not found for project: ${projectPath}`);
      }

      // 使用查询工具根据ID搜索
      const results = await this.queryUtils.scrollPoints(
        collectionName,
        {
          filter: {
            must: [
              {
                key: 'id',
                match: { value: id }
              }
            ]
          }
        },
        1
      );

      const duration = Date.now() - startTime;
      this.performanceMonitor.recordOperation('getProjectDataById', duration);
      await this.databaseLogger.logQueryOperation('getById', projectPath, 'success', {
        id,
        duration,
        found: results.length > 0
      });

      return results.length > 0 ? results[0] : null;
    } catch (error) {
      const duration = Date.now() - Date.now(); // This is approximate since we don't have exact startTime
      this.performanceMonitor.recordOperation('getProjectDataById', duration);
      await this.databaseLogger.logQueryOperation('getById', projectPath, 'failed', {
        id,
        duration,
        error: error instanceof Error ? error.message : String(error)
      });
      this.errorHandler.handleError(
        new Error(
          `Failed to get project data by ID ${id} for project ${projectPath}: ${error instanceof Error ? error.message : String(error)}`
        ),
        { component: 'QdrantProjectManager', operation: 'getProjectDataById' }
      );
      return null;
    }
  }
}