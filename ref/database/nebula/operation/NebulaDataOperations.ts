import { injectable, inject } from 'inversify';
import { TYPES } from '../../../types';
import { DatabaseLoggerService } from '../../common/DatabaseLoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { INebulaSpaceManager } from '../space/NebulaSpaceManager';
import { INebulaQueryService } from '../query/NebulaQueryService';
import { NebulaNode, NebulaRelationship } from '../NebulaTypes';
import { NebulaBaseOperations } from './NebulaBaseOperations';
import { INebulaGraphOperations } from './NebulaGraphOperations';

/**
 * Nebula数据操作服务接口
 */
export interface INebulaDataOperations {
  // 高级批量操作（带业务逻辑）
  insertNodes(projectId: string, spaceName: string, nodes: NebulaNode[]): Promise<boolean>;
  insertRelationships(projectId: string, spaceName: string, relationships: NebulaRelationship[]): Promise<boolean>;
  
  // 统一更新操作（自动识别节点或关系）
  updateData(projectId: string, spaceName: string, id: string, data: any): Promise<boolean>;
  
  // 统一删除操作（自动识别节点或关系）
  deleteData(projectId: string, spaceName: string, id: string): Promise<boolean>;
  
  // 高级查询操作
  findNodesByLabel(projectId: string, spaceName: string, label: string, filter?: any): Promise<any[]>;
  findRelationshipsByType(projectId: string, spaceName: string, type?: string, filter?: any): Promise<any[]>;
  getDataById(projectId: string, spaceName: string, id: string): Promise<any>;
  
  // 复杂搜索功能
  search(projectId: string, spaceName: string, query: any): Promise<any[]>;
}

/**
 * Nebula数据操作服务实现
 * 负责高级数据操作：批量插入、复杂查询、搜索功能、业务逻辑封装
 */
@injectable()
export class NebulaDataOperations extends NebulaBaseOperations implements INebulaDataOperations {
  private graphOperations: INebulaGraphOperations;

  constructor(
    @inject(TYPES.DatabaseLoggerService) databaseLogger: DatabaseLoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.INebulaSpaceManager) spaceManager: INebulaSpaceManager,
    @inject(TYPES.INebulaQueryService) queryService: INebulaQueryService,
    @inject(TYPES.INebulaGraphOperations) graphOperations: INebulaGraphOperations
  ) {
    super(databaseLogger, errorHandler, spaceManager, queryService);
    this.graphOperations = graphOperations;
  }

  async insertNodes(projectId: string, spaceName: string, nodes: NebulaNode[]): Promise<boolean> {
    try {
      this.validateRequiredParams({ projectId, spaceName, nodes }, ['projectId', 'spaceName', 'nodes']);
      if (nodes.length === 0) throw new Error('Nodes array cannot be empty');

      await this.ensureSpaceAndSwitch(projectId, spaceName);
      
      // 添加项目ID到节点属性
      const nodesWithProjectId = this.addProjectId(nodes, projectId);
      
      // 转换为BatchVertex格式
      const batchVertices = nodesWithProjectId.map(node => ({
        tag: node.label,
        id: node.id,
        properties: node.properties || {}
      }));

      // 使用底层图操作进行批量插入
      const result = await this.graphOperations.batchInsertVertices(batchVertices);
      
      if (result) {
        this.logSuccess('insertNodes', { projectId, nodeCount: nodes.length });
      }
      
      return result;
    } catch (error) {
      this.logError('insertNodes', { projectId, spaceName, nodeCount: nodes.length }, error);
      this.handleOperationError(error, 'NebulaDataOperations', 'insertNodes', 
        { projectId, spaceName, nodeCount: nodes.length });
    }
  }

  async insertRelationships(projectId: string, spaceName: string, relationships: NebulaRelationship[]): Promise<boolean> {
    try {
      this.validateRequiredParams({ projectId, spaceName, relationships }, ['projectId', 'spaceName', 'relationships']);
      if (relationships.length === 0) throw new Error('Relationships array cannot be empty');

      await this.ensureSpaceAndSwitch(projectId, spaceName);
      
      // 添加项目ID到关系属性
      const relationshipsWithProjectId = this.addProjectId(relationships, projectId);
      
      // 转换为BatchEdge格式
      const batchEdges = relationshipsWithProjectId.map(rel => ({
        type: rel.type,
        srcId: rel.sourceId,
        dstId: rel.targetId,
        properties: rel.properties || {}
      }));

      // 使用底层图操作进行批量插入
      const result = await this.graphOperations.batchInsertEdges(batchEdges);
      
      if (result) {
        this.logSuccess('insertRelationships', { projectId, relationshipCount: relationships.length });
      }
      
      return result;
    } catch (error) {
      this.logError('insertRelationships', { projectId, spaceName, relationshipCount: relationships.length }, error);
      this.handleOperationError(error, 'NebulaDataOperations', 'insertRelationships',
        { projectId, spaceName, relationshipCount: relationships.length });
    }
  }

  async updateData(projectId: string, spaceName: string, id: string, data: any): Promise<boolean> {
    try {
      this.validateRequiredParams({ projectId, spaceName, id }, ['projectId', 'spaceName', 'id']);
      await this.ensureSpaceAndSwitch(projectId, spaceName);

      // 首先尝试作为节点更新
      const nodeResult = await this.queryService.executeQuery(
        `MATCH (v) WHERE id(v) == "${id}" RETURN v LIMIT 1`
      );

      if (nodeResult?.data && nodeResult.data.length > 0) {
        const node = nodeResult.data[0].v;
        const tag = node.tags?.[0]; // 获取第一个标签
        if (tag) {
          const properties = data.properties || data;
          return await this.graphOperations.updateVertex(id, tag, properties);
        }
      }

      // 如果不是节点，尝试作为边更新
      const relResult = await this.queryService.executeQuery(
        `MATCH ()-[e]->() WHERE id(e) == "${id}" RETURN e LIMIT 1`
      );

      if (relResult?.data && relResult.data.length > 0) {
        const edge = relResult.data[0].e;
        const edgeType = edge.type;
        const srcId = edge.src;
        const dstId = edge.dst;
        if (edgeType && srcId && dstId) {
          const properties = data.properties || data;
          return await this.graphOperations.updateEdge(srcId, dstId, edgeType, properties);
        }
      }

      throw new Error(`Data with ID ${id} not found`);
    } catch (error) {
      this.logError('updateData', { projectId, spaceName, id }, error);
      this.handleOperationError(error, 'NebulaDataOperations', 'updateData',
        { projectId, spaceName, id });
    }
  }

  async deleteData(projectId: string, spaceName: string, id: string): Promise<boolean> {
    try {
      this.validateRequiredParams({ projectId, spaceName, id }, ['projectId', 'spaceName', 'id']);
      await this.ensureSpaceAndSwitch(projectId, spaceName);

      // 首先尝试作为节点删除
      const nodeResult = await this.queryService.executeQuery(
        `MATCH (v) WHERE id(v) == "${id}" RETURN v LIMIT 1`
      );

      if (nodeResult?.data && nodeResult.data.length > 0) {
        return await this.graphOperations.deleteVertex(id);
      }

      // 如果不是节点，尝试作为边删除
      const relResult = await this.queryService.executeQuery(
        `MATCH ()-[e]->() WHERE id(e) == "${id}" RETURN e LIMIT 1`
      );

      if (relResult?.data && relResult.data.length > 0) {
        const edge = relResult.data[0].e;
        const srcId = edge.src;
        const dstId = edge.dst;
        if (srcId && dstId) {
          return await this.graphOperations.deleteEdge(srcId, dstId);
        }
      }

      throw new Error(`Data with ID ${id} not found`);
    } catch (error) {
      this.logError('deleteData', { projectId, spaceName, id }, error);
      this.handleOperationError(error, 'NebulaDataOperations', 'deleteData',
        { projectId, spaceName, id });
    }
  }

  async findNodesByLabel(projectId: string, spaceName: string, label: string, filter?: any): Promise<any[]> {
    try {
      this.validateRequiredParams({ projectId, spaceName, label }, ['projectId', 'spaceName', 'label']);
      await this.ensureSpaceAndSwitch(projectId, spaceName);

      let query = `MATCH (v:${label}) WHERE v.projectId == "${projectId}"`;
      
      if (filter) {
        const conditions = Object.entries(filter)
          .map(([key, value]) => `v.${key} == ${this.formatValue(value)}`)
          .join(' AND ');
        query += ` AND ${conditions}`;
      }
      
      query += ' RETURN v';

      const result = await this.queryService.executeQuery(query);
      return result.data || [];
    } catch (error) {
      this.logError('findNodesByLabel', { projectId, spaceName, label, filter }, error);
      this.handleOperationError(error, 'NebulaDataOperations', 'findNodesByLabel',
        { projectId, spaceName, label, filter });
    }
  }

  async findRelationshipsByType(projectId: string, spaceName: string, type?: string, filter?: any): Promise<any[]> {
    try {
      this.validateRequiredParams({ projectId, spaceName }, ['projectId', 'spaceName']);
      await this.ensureSpaceAndSwitch(projectId, spaceName);

      let query = `MATCH ()-[e${type ? `:${type}` : ''}]->() WHERE e.projectId == "${projectId}"`;
      
      if (filter) {
        const conditions = Object.entries(filter)
          .map(([key, value]) => `e.${key} == ${this.formatValue(value)}`)
          .join(' AND ');
        query += ` AND ${conditions}`;
      }
      
      query += ' RETURN e';

      const result = await this.queryService.executeQuery(query);
      return result.data || [];
    } catch (error) {
      this.logError('findRelationshipsByType', { projectId, spaceName, type, filter }, error);
      this.handleOperationError(error, 'NebulaDataOperations', 'findRelationshipsByType',
        { projectId, spaceName, type, filter });
    }
  }

  async getDataById(projectId: string, spaceName: string, id: string): Promise<any> {
    try {
      this.validateRequiredParams({ projectId, spaceName, id }, ['projectId', 'spaceName', 'id']);
      await this.ensureSpaceAndSwitch(projectId, spaceName);

      const nodeResult = await this.queryService.executeQuery(
        `MATCH (v) WHERE id(v) == "${id}" RETURN v LIMIT 1`
      );

      if (nodeResult?.data && nodeResult.data.length > 0) {
        return nodeResult.data[0];
      }

      const relResult = await this.queryService.executeQuery(
        `MATCH ()-[e]->() WHERE id(e) == "${id}" RETURN e LIMIT 1`
      );

      return (relResult?.data && relResult.data.length > 0) ? relResult.data[0] : null;
    } catch (error) {
      if (error instanceof Error && error.message.includes('not found')) {
        return null;
      }
      this.logError('getDataById', { projectId, spaceName, id }, error);
      this.handleOperationError(error, 'NebulaDataOperations', 'getDataById',
        { projectId, spaceName, id });
    }
  }

  async search(projectId: string, spaceName: string, query: any): Promise<any[]> {
    try {
      this.validateRequiredParams({ projectId, spaceName }, ['projectId', 'spaceName']);
      await this.ensureSpaceAndSwitch(projectId, spaceName);

      const searchQuery = typeof query === 'string' 
        ? query 
        : this.buildSearchQuery(projectId, query);

      const result = await this.queryService.executeQuery(searchQuery);
      return result?.data || [];
    } catch (error) {
      this.logError('search', { projectId, spaceName, query }, error);
      this.handleOperationError(error, 'NebulaDataOperations', 'search',
        { projectId, spaceName, query });
    }
  }

  private buildSearchQuery(projectId: string, query: any): string {
    if (query.type === 'node') {
      const label = query.label || '*';
      let q = `MATCH (v${label !== '*' ? `:${label}` : ''}) WHERE v.projectId == "${projectId}"`;
      
      if (query.filter) {
        const conditions = Object.entries(query.filter)
          .map(([key, value]) => `v.${key} == ${this.formatValue(value)}`)
          .join(' AND ');
        q += ` AND ${conditions}`;
      }
      
      return q + ' RETURN v';
    }

    if (query.type === 'relationship') {
      const type = query.relationshipType || '*';
      let q = `MATCH ()-[e${type !== '*' ? `:${type}` : ''}]->() WHERE e.projectId == "${projectId}"`;
      
      if (query.filter) {
        const conditions = Object.entries(query.filter)
          .map(([key, value]) => `e.${key} == ${this.formatValue(value)}`)
          .join(' AND ');
        q += ` AND ${conditions}`;
      }
      
      return q + ' RETURN e';
    }

    if (query.type === 'graph') {
      const { startNode, pathLength = 1, direction = 'BOTH' } = query;
      
      if (startNode) {
        return `MATCH (start) WHERE id(start) == "${startNode}" ` +
               `MATCH p = (start)-[:*${direction} ${pathLength}]->(end) ` +
               `WHERE ANY(n IN nodes(p) WHERE n.projectId == "${projectId}") RETURN p`;
      }
      
      return `MATCH (n) WHERE n.projectId == "${projectId}" ` +
             `MATCH p = (n)-[:*${direction} ${pathLength}]->(m) RETURN p`;
    }

    return `MATCH (v) WHERE v.projectId == "${projectId}" RETURN v`;
  }
}