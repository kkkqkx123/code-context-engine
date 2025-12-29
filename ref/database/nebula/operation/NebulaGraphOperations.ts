import { injectable, inject } from 'inversify';
import { TYPES } from '../../../types';
import { DatabaseLoggerService } from '../../common/DatabaseLoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { INebulaSpaceManager } from '../space/NebulaSpaceManager';
import { INebulaQueryService } from '../query/NebulaQueryService';
import { INebulaQueryBuilder } from '../query/NebulaQueryBuilder';
import { BatchVertex, BatchEdge } from '../NebulaTypes';
import { INebulaClient } from '../client/NebulaClient';
import { NebulaBaseOperations } from './NebulaBaseOperations';

export interface INebulaGraphOperations {
  // 单个节点操作
  insertVertex(tag: string, vertexId: string, properties: Record<string, any>): Promise<boolean>;
  updateVertex(vertexId: string, tag: string, properties: Record<string, any>): Promise<boolean>;
  deleteVertex(vertexId: string, tag?: string): Promise<boolean>;
  
  // 单个边操作
  insertEdge(edgeType: string, srcId: string, dstId: string, properties: Record<string, any>): Promise<boolean>;
  updateEdge(srcId: string, dstId: string, edgeType: string, properties: Record<string, any>): Promise<boolean>;
  deleteEdge(srcId: string, dstId: string, edgeType?: string): Promise<boolean>;
  
  // 批量操作
  batchInsertVertices(vertices: BatchVertex[]): Promise<boolean>;
  batchInsertEdges(edges: BatchEdge[]): Promise<boolean>;
  
  // 图统计
  getGraphStats(): Promise<{ nodeCount: number; relationshipCount: number }>;
}

/**
 * Nebula图操作服务
 * 负责底层图操作：单个节点/边的CRUD、批量操作、图统计
 */
@injectable()
export class NebulaGraphOperations extends NebulaBaseOperations implements INebulaGraphOperations {
  private queryBuilder: INebulaQueryBuilder;
  private nebulaClient: INebulaClient;

  constructor(
    @inject(TYPES.DatabaseLoggerService) databaseLogger: DatabaseLoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.INebulaSpaceManager) spaceManager: INebulaSpaceManager,
    @inject(TYPES.INebulaQueryService) queryService: INebulaQueryService,
    @inject(TYPES.INebulaQueryBuilder) queryBuilder: INebulaQueryBuilder,
    @inject(TYPES.INebulaClient) nebulaClient: INebulaClient
  ) {
    super(databaseLogger, errorHandler, spaceManager, queryService);
    this.queryBuilder = queryBuilder;
    this.nebulaClient = nebulaClient;
  }

  async insertVertex(tag: string, vertexId: string, properties: Record<string, any>): Promise<boolean> {
    try {
      const { query, params } = this.queryBuilder.insertVertex(tag, vertexId, properties);
      await this.nebulaClient.executeQuery(query, params);
      
      this.logSuccess('insertVertex', { tag, vertexId });
      return true;
    } catch (error) {
      this.logError('insertVertex', { tag, vertexId }, error);
      this.handleOperationError(error, 'NebulaGraphOperations', 'insertVertex', { tag, vertexId });
    }
  }

  async insertEdge(edgeType: string, srcId: string, dstId: string, properties: Record<string, any>): Promise<boolean> {
    try {
      const { query, params } = this.queryBuilder.insertEdge(edgeType, srcId, dstId, properties);
      await this.nebulaClient.executeQuery(query, params);
      
      this.logSuccess('insertEdge', { edgeType, srcId, dstId });
      return true;
    } catch (error) {
      this.logError('insertEdge', { edgeType, srcId, dstId }, error);
      this.handleOperationError(error, 'NebulaGraphOperations', 'insertEdge', { edgeType, srcId, dstId });
    }
  }

  async batchInsertVertices(vertices: BatchVertex[]): Promise<boolean> {
    try {
      if (vertices.length === 0) return true;
      
      const { query, params } = this.queryBuilder.batchInsertVertices(vertices);
      if (query && query.length > 0) {
        await this.nebulaClient.executeQuery(query, params);
      }
      
      this.logSuccess('batchInsertVertices', { count: vertices.length });
      return true;
    } catch (error) {
      this.logError('batchInsertVertices', { count: vertices.length }, error);
      this.handleOperationError(error, 'NebulaGraphOperations', 'batchInsertVertices', { count: vertices.length });
    }
  }

  async batchInsertEdges(edges: BatchEdge[]): Promise<boolean> {
    try {
      if (edges.length === 0) return true;
      
      const { query, params } = this.queryBuilder.batchInsertEdges(edges);
      if (query && query.length > 0) {
        await this.nebulaClient.executeQuery(query, params);
      }
      
      this.logSuccess('batchInsertEdges', { count: edges.length });
      return true;
    } catch (error) {
      this.logError('batchInsertEdges', { count: edges.length }, error);
      this.handleOperationError(error, 'NebulaGraphOperations', 'batchInsertEdges', { count: edges.length });
    }
  }

  async updateVertex(vertexId: string, tag: string, properties: Record<string, any>): Promise<boolean> {
    try {
      if (Object.keys(properties).length === 0) return true;

      const { query, params } = this.queryBuilder.updateVertex(vertexId, tag, properties);
      await this.nebulaClient.executeQuery(query, params);
      
      this.logSuccess('updateVertex', { vertexId, tag });
      return true;
    } catch (error) {
      this.logError('updateVertex', { vertexId, tag }, error);
      this.handleOperationError(error, 'NebulaGraphOperations', 'updateVertex', { vertexId, tag });
    }
  }

  async updateEdge(srcId: string, dstId: string, edgeType: string, properties: Record<string, any>): Promise<boolean> {
    try {
      if (Object.keys(properties).length === 0) return true;

      const { query, params } = this.queryBuilder.updateEdge(srcId, dstId, edgeType, properties);
      await this.nebulaClient.executeQuery(query, params);
      
      this.logSuccess('updateEdge', { srcId, dstId, edgeType });
      return true;
    } catch (error) {
      this.logError('updateEdge', { srcId, dstId, edgeType }, error);
      this.handleOperationError(error, 'NebulaGraphOperations', 'updateEdge', { srcId, dstId, edgeType });
    }
  }

  async deleteVertex(vertexId: string, tag?: string): Promise<boolean> {
    try {
      const query = tag 
        ? `DELETE VERTEX "${vertexId}" TAG \`${tag}\``
        : `DELETE VERTEX "${vertexId}" WITH EDGE`;

      await this.nebulaClient.executeQuery(query);
      
      this.logSuccess('deleteVertex', { vertexId, tag });
      return true;
    } catch (error) {
      this.logError('deleteVertex', { vertexId, tag }, error);
      this.handleOperationError(error, 'NebulaGraphOperations', 'deleteVertex', { vertexId, tag });
    }
  }

  async deleteEdge(srcId: string, dstId: string, edgeType?: string): Promise<boolean> {
    try {
      const query = edgeType
        ? `DELETE EDGE \`${edgeType}\` "${srcId}" -> "${dstId}"`
        : `DELETE EDGE "${srcId}" -> "${dstId}"`;

      await this.nebulaClient.executeQuery(query);
      
      this.logSuccess('deleteEdge', { srcId, dstId, edgeType });
      return true;
    } catch (error) {
      this.logError('deleteEdge', { srcId, dstId, edgeType }, error);
      this.handleOperationError(error, 'NebulaGraphOperations', 'deleteEdge', { srcId, dstId, edgeType });
    }
  }

  async getGraphStats(): Promise<{ nodeCount: number; relationshipCount: number }> {
    try {
      let totalNodeCount = 0;
      let totalRelationshipCount = 0;

      const tagsResult = await this.nebulaClient.executeQuery('SHOW TAGS');
      if (tagsResult?.data && Array.isArray(tagsResult.data)) {
        for (const tag of tagsResult.data) {
          const tagName = tag.Name || tag.name;
          if (tagName) {
            const { query, params } = this.queryBuilder.buildNodeCountQuery(tagName);
            const countResult = await this.nebulaClient.executeQuery(query, params);
            if (countResult?.data && Array.isArray(countResult.data) && countResult.data.length > 0) {
              totalNodeCount += countResult.data[0].total || 0;
            }
          }
        }
      }

      const edgesResult = await this.nebulaClient.executeQuery('SHOW EDGES');
      if (edgesResult?.data && Array.isArray(edgesResult.data)) {
        for (const edge of edgesResult.data) {
          const edgeName = edge.Name || edge.name;
          if (edgeName) {
            const { query, params } = this.queryBuilder.buildRelationshipCountQuery(edgeName);
            const countResult = await this.nebulaClient.executeQuery(query, params);
            if (countResult?.data && Array.isArray(countResult.data) && countResult.data.length > 0) {
              totalRelationshipCount += countResult.data[0].total || 0;
            }
          }
        }
      }

      return { nodeCount: totalNodeCount, relationshipCount: totalRelationshipCount };
    } catch (error) {
      this.logError('getGraphStats', {}, error);
      return { nodeCount: 0, relationshipCount: 0 };
    }
  }
}