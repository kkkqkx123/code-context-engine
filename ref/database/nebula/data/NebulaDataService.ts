import { injectable, inject } from 'inversify';
import { INebulaConnectionManager } from '../NebulaConnectionManager';
import { TYPES } from '../../../types';
import { DatabaseLoggerService } from '../../common/DatabaseLoggerService';
import { DatabaseEventType } from '../../common/DatabaseEventTypes';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { NodeIdGenerator } from '../../../utils/deterministic-node-id';

export interface INebulaDataService {
  // 节点操作
  createNode(node: { label: string; properties: Record<string, any> }): Promise<string>;
  findNodesByLabel(label: string, properties?: Record<string, any>): Promise<any[]>;
  updateNode(nodeId: string, properties: Record<string, any>): Promise<boolean>;
  deleteNode(nodeId: string): Promise<boolean>;

  // 关系操作
  createRelationship(relationship: {
    type: string;
    sourceId: string;
    targetId: string;
    properties?: Record<string, any>;
  }): Promise<void>;
  findRelationships(type?: string, properties?: Record<string, any>): Promise<any[]>;
  updateRelationship(relationshipId: string, properties: Record<string, any>): Promise<boolean>;
  deleteRelationship(relationshipId: string): Promise<boolean>;

  // 批量操作
  batchInsertNodes(nodes: Array<{ label: string; properties: Record<string, any> }>): Promise<boolean>;
  batchInsertRelationships(relationships: Array<{
    type: string;
    sourceId: string;
    targetId: string;
    properties?: Record<string, any>;
  }>): Promise<boolean>;

  // 统计信息
  getDatabaseStats(): Promise<any>;
}

@injectable()
export class NebulaDataService implements INebulaDataService {
  private connectionManager: INebulaConnectionManager;
  private databaseLogger: DatabaseLoggerService;
  private errorHandler: ErrorHandlerService;

  constructor(
    @inject(TYPES.NebulaConnectionManager) connectionManager: INebulaConnectionManager,
    @inject(TYPES.DatabaseLoggerService) databaseLogger: DatabaseLoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService
  ) {
    this.connectionManager = connectionManager;
    this.databaseLogger = databaseLogger;
    this.errorHandler = errorHandler;
  }

  async createNode(node: { label: string; properties: Record<string, any> }): Promise<string> {
    if (!this.connectionManager.isConnected()) {
      throw new Error('Not connected to Nebula Graph');
    }

    try {
      // 使用 DatabaseLoggerService 记录节点创建信息
      this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_INSERTED,
        source: 'nebula',
        timestamp: new Date(),
        data: {
          message: 'Creating node',
          label: node.label,
          properties: node.properties
        }
      }).catch(error => {
        console.error('Failed to log node creation info:', error);
      });

      // 生成节点ID
      const nodeId = NodeIdGenerator.forDatabase(node.label);

      // 构建插入节点的nGQL，使用安全的参数处理
      const propertyNames = Object.keys(node.properties).join(', ');

      let nGQL = `INSERT VERTEX ${node.label}`;
      if (propertyNames) {
        const escapedProperties = this.escapeProperties(node.properties);
        const propertyValues = Object.values(escapedProperties).map(v => `"${v}"`).join(', ');
        nGQL += `(${propertyNames}) VALUES "${nodeId}":(${propertyValues})`;
      } else {
        nGQL += `() VALUES "${nodeId}":()`;
      }

      const result = await this.connectionManager.executeQuery(nGQL);

      if (result.error) {
        throw new Error(`Failed to create node: ${result.error}`);
      }

      // 使用 DatabaseLoggerService 记录节点创建成功信息
      this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_INSERTED,
        source: 'nebula',
        timestamp: new Date(),
        data: {
          message: 'Node created successfully',
          nodeId
        }
      }).catch(error => {
        console.error('Failed to log node creation success:', error);
      });

      return nodeId;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.errorHandler.handleError(
        new Error(`Failed to create node: ${errorMessage}`),
        {
          component: 'NebulaDataService',
          operation: 'createNode',
          node
        }
      );

      throw error;
    }
  }

  async findNodesByLabel(label: string, properties?: Record<string, any>): Promise<any[]> {
    if (!this.connectionManager.isConnected()) {
      throw new Error('Not connected to Nebula Graph');
    }

    try {
      // 使用 DatabaseLoggerService 记录按标签查找节点信息
      this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_QUERIED,
        source: 'nebula',
        timestamp: new Date(),
        data: {
          message: 'Finding nodes by label',
          label,
          properties
        }
      }).catch(error => {
        console.error('Failed to log node search info:', error);
      });

      // 构建查询节点的nGQL，使用安全的参数处理
      let nGQL = `MATCH (n:${label})`;

      if (properties && Object.keys(properties).length > 0) {
        const escapedProperties = this.escapeProperties(properties);
        const conditions = Object.entries(escapedProperties)
          .map(([key, value]) => `n.${key} == "${value}"`)
          .join(' AND ');
        nGQL += ` WHERE ${conditions}`;
      }

      nGQL += ' RETURN n';

      const result = await this.connectionManager.executeQuery(nGQL);

      if (result.error) {
        throw new Error(`Failed to find nodes: ${result.error}`);
      }

      // 提取节点数据
      const nodes = result?.data || [];

      // 使用 DatabaseLoggerService 记录找到的节点信息
      this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_QUERIED,
        source: 'nebula',
        timestamp: new Date(),
        data: {
          message: 'Found nodes',
          count: nodes.length
        }
      }).catch(error => {
        console.error('Failed to log found nodes info:', error);
      });

      return nodes;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.errorHandler.handleError(
        new Error(`Failed to find nodes: ${errorMessage}`),
        {
          component: 'NebulaDataService',
          operation: 'findNodesByLabel',
          label,
          properties
        }
      );

      throw error;
    }
  }

  async updateNode(nodeId: string, properties: Record<string, any>): Promise<boolean> {
    if (!this.connectionManager.isConnected()) {
      throw new Error('Not connected to Nebula Graph');
    }

    try {
      if (!nodeId || !properties || Object.keys(properties).length === 0) {
        return false;
      }

      // 使用 DatabaseLoggerService 记录节点更新信息
      this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_UPDATED,
        source: 'nebula',
        timestamp: new Date(),
        data: {
          message: 'Updating node',
          nodeId,
          properties
        }
      }).catch(error => {
        console.error('Failed to log node update info:', error);
      });

      const escapedProperties = this.escapeProperties(properties);
      const setClauses = Object.entries(escapedProperties)
        .map(([key, value]) => `${key} = "${value}"`)
        .join(', ');

      const nGQL = `UPDATE VERTEX ON ANY SET ${setClauses} WHERE id == "${nodeId}"`;
      const result = await this.connectionManager.executeQuery(nGQL);

      if (result.error) {
        throw new Error(`Failed to update node: ${result.error}`);
      }

      // 使用 DatabaseLoggerService 记录节点更新成功信息
      this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_UPDATED,
        source: 'nebula',
        timestamp: new Date(),
        data: {
          message: 'Node updated successfully',
          nodeId
        }
      }).catch(error => {
        console.error('Failed to log node update success:', error);
      });

      return true;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.errorHandler.handleError(
        new Error(`Failed to update node: ${errorMessage}`),
        {
          component: 'NebulaDataService',
          operation: 'updateNode',
          nodeId,
          properties
        }
      );

      return false;
    }
  }

  async deleteNode(nodeId: string): Promise<boolean> {
    if (!this.connectionManager.isConnected()) {
      throw new Error('Not connected to Nebula Graph');
    }

    try {
      if (!nodeId) {
        return false;
      }

      // 使用 DatabaseLoggerService 记录节点删除信息
      this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_DELETED,
        source: 'nebula',
        timestamp: new Date(),
        data: {
          message: 'Deleting node',
          nodeId
        }
      }).catch(error => {
        console.error('Failed to log node deletion info:', error);
      });

      const nGQL = `DELETE VERTEX "${nodeId}" WITH EDGE`;
      const result = await this.connectionManager.executeQuery(nGQL);

      if (result.error) {
        throw new Error(`Failed to delete node: ${result.error}`);
      }

      // 使用 DatabaseLoggerService 记录节点删除成功信息
      this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_DELETED,
        source: 'nebula',
        timestamp: new Date(),
        data: {
          message: 'Node deleted successfully',
          nodeId
        }
      }).catch(error => {
        console.error('Failed to log node deletion success:', error);
      });

      return true;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.errorHandler.handleError(
        new Error(`Failed to delete node: ${errorMessage}`),
        {
          component: 'NebulaDataService',
          operation: 'deleteNode',
          nodeId
        }
      );

      return false;
    }
  }

  async createRelationship(relationship: {
    type: string;
    sourceId: string;
    targetId: string;
    properties?: Record<string, any>;
  }): Promise<void> {
    if (!this.connectionManager.isConnected()) {
      throw new Error('Not connected to Nebula Graph');
    }

    try {
      // 使用 DatabaseLoggerService 记录关系创建信息
      this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_INSERTED,
        source: 'nebula',
        timestamp: new Date(),
        data: {
          message: 'Creating relationship',
          type: relationship.type,
          sourceId: relationship.sourceId,
          targetId: relationship.targetId,
          properties: relationship.properties
        }
      }).catch(error => {
        console.error('Failed to log relationship creation info:', error);
      });

      // 构建插入边的nGQL，使用安全的参数处理
      let nGQL = `INSERT EDGE ${relationship.type}`;

      if (relationship.properties && Object.keys(relationship.properties).length > 0) {
        const propertyNames = Object.keys(relationship.properties).join(', ');
        const escapedProperties = this.escapeProperties(relationship.properties);
        const propertyValues = Object.values(escapedProperties).map(v => `"${v}"`).join(', ');
        nGQL += `(${propertyNames}) VALUES "${relationship.sourceId}"->"${relationship.targetId}":(${propertyValues})`;
      } else {
        nGQL += `() VALUES "${relationship.sourceId}"->"${relationship.targetId}":()`;
      }

      const result = await this.connectionManager.executeQuery(nGQL);

      if (result.error) {
        throw new Error(`Failed to create relationship: ${result.error}`);
      }

      // 使用 DatabaseLoggerService 记录关系创建成功信息
      this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_INSERTED,
        source: 'nebula',
        timestamp: new Date(),
        data: { message: 'Relationship created successfully' }
      }).catch(error => {
        console.error('Failed to log relationship creation success:', error);
      });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.errorHandler.handleError(
        new Error(`Failed to create relationship: ${errorMessage}`),
        {
          component: 'NebulaDataService',
          operation: 'createRelationship',
          relationship
        }
      );

      throw error;
    }
  }

  async findRelationships(type?: string, properties?: Record<string, any>): Promise<any[]> {
    if (!this.connectionManager.isConnected()) {
      throw new Error('Not connected to Nebula Graph');
    }

    try {
      // 使用 DatabaseLoggerService 记录查找关系信息
      this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_QUERIED,
        source: 'nebula',
        timestamp: new Date(),
        data: {
          message: 'Finding relationships',
          type,
          properties
        }
      }).catch(error => {
        console.error('Failed to log relationship search info:', error);
      });

      // 构建查询边的nGQL，使用安全的参数处理
      let nGQL = 'MATCH ()-[r';

      if (type) {
        nGQL += `:${type}`;
      }

      nGQL += ']->()';

      if (properties && Object.keys(properties).length > 0) {
        const escapedProperties = this.escapeProperties(properties);
        const conditions = Object.entries(escapedProperties)
          .map(([key, value]) => `r.${key} == "${value}"`)
          .join(' AND ');
        nGQL += ` WHERE ${conditions}`;
      }

      nGQL += ' RETURN r';

      const result = await this.connectionManager.executeQuery(nGQL);

      if (result.error) {
        throw new Error(`Failed to find relationships: ${result.error}`);
      }

      // 提取关系数据
      const relationships = result?.data || [];

      // 使用 DatabaseLoggerService 记录找到的关系信息
      this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_QUERIED,
        source: 'nebula',
        timestamp: new Date(),
        data: {
          message: 'Found relationships',
          count: relationships.length
        }
      }).catch(error => {
        console.error('Failed to log found relationships info:', error);
      });

      return relationships;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.errorHandler.handleError(
        new Error(`Failed to find relationships: ${errorMessage}`),
        {
          component: 'NebulaDataService',
          operation: 'findRelationships',
          type,
          properties
        }
      );

      throw error;
    }
  }

  async updateRelationship(relationshipId: string, properties: Record<string, any>): Promise<boolean> {
    if (!this.connectionManager.isConnected()) {
      throw new Error('Not connected to Nebula Graph');
    }

    try {
      if (!relationshipId || !properties || Object.keys(properties).length === 0) {
        return false;
      }

      // 在Nebula中，边的更新通常需要更具体的查询基于源节点和目标节点
      // 这里需要更复杂的实现，需要明确源节点、目标节点和边类型
      throw new Error('Relationship update not fully implemented - requires source/target information');
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.errorHandler.handleError(
        new Error(`Failed to update relationship: ${errorMessage}`),
        {
          component: 'NebulaDataService',
          operation: 'updateRelationship',
          relationshipId,
          properties
        }
      );

      return false;
    }
  }

  async deleteRelationship(relationshipId: string): Promise<boolean> {
    if (!this.connectionManager.isConnected()) {
      throw new Error('Not connected to Nebula Graph');
    }

    try {
      if (!relationshipId) {
        return false;
      }

      // 这里需要更具体的实现来删除特定的关系
      // 通常需要知道源节点、目标节点和边类型
      throw new Error('Relationship deletion not fully implemented - requires source/target information');
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.errorHandler.handleError(
        new Error(`Failed to delete relationship: ${errorMessage}`),
        {
          component: 'NebulaDataService',
          operation: 'deleteRelationship',
          relationshipId
        }
      );

      return false;
    }
  }

  async batchInsertNodes(nodes: Array<{ label: string; properties: Record<string, any> }>): Promise<boolean> {
    if (!this.connectionManager.isConnected()) {
      throw new Error('Not connected to Nebula Graph');
    }

    try {
      // 使用 DatabaseLoggerService 记录批量节点插入信息
      this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_INSERTED,
        source: 'nebula',
        timestamp: new Date(),
        data: {
          message: 'Batch inserting nodes',
          count: nodes.length
        }
      }).catch(error => {
        console.error('Failed to log batch node insertion info:', error);
      });

      // 批量执行节点插入 - 由于Nebula不支持真正事务，这里按顺序执行
      for (const node of nodes) {
        await this.createNode(node);
      }

      // 使用 DatabaseLoggerService 记录批量节点插入成功信息
      this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_INSERTED,
        source: 'nebula',
        timestamp: new Date(),
        data: {
          message: 'Batch nodes inserted successfully',
          count: nodes.length
        }
      }).catch(error => {
        console.error('Failed to log batch node insertion success:', error);
      });

      return true;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.errorHandler.handleError(
        new Error(`Failed to batch insert nodes: ${errorMessage}`),
        {
          component: 'NebulaDataService',
          operation: 'batchInsertNodes',
          nodes
        }
      );

      return false;
    }
  }

  async batchInsertRelationships(relationships: Array<{
    type: string;
    sourceId: string;
    targetId: string;
    properties?: Record<string, any>;
  }>): Promise<boolean> {
    if (!this.connectionManager.isConnected()) {
      throw new Error('Not connected to Nebula Graph');
    }

    try {
      // 使用 DatabaseLoggerService 记录批量关系插入信息
      this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_INSERTED,
        source: 'nebula',
        timestamp: new Date(),
        data: {
          message: 'Batch inserting relationships',
          count: relationships.length
        }
      }).catch(error => {
        console.error('Failed to log batch relationship insertion info:', error);
      });

      // 批量执行关系插入 - 由于Nebula不支持真正事务，这里按顺序执行
      for (const relationship of relationships) {
        await this.createRelationship(relationship);
      }

      // 使用 DatabaseLoggerService 记录批量关系插入成功信息
      this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_INSERTED,
        source: 'nebula',
        timestamp: new Date(),
        data: {
          message: 'Batch relationships inserted successfully',
          count: relationships.length
        }
      }).catch(error => {
        console.error('Failed to log batch relationship insertion success:', error);
      });

      return true;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.errorHandler.handleError(
        new Error(`Failed to batch insert relationships: ${errorMessage}`),
        {
          component: 'NebulaDataService',
          operation: 'batchInsertRelationships',
          relationships
        }
      );

      return false;
    }
  }

  async getDatabaseStats(): Promise<any> {
    if (!this.connectionManager.isConnected()) {
      throw new Error('Not connected to Nebula Graph');
    }

    try {
      // 使用 DatabaseLoggerService 记录获取数据库统计信息
      this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.PERFORMANCE_METRIC,
        source: 'nebula',
        timestamp: new Date(),
        data: { message: 'Getting database stats' }
      }).catch(error => {
        console.error('Failed to log database stats info:', error);
      });

      // 获取spaces信息
      const spacesResult = await this.connectionManager.executeQuery('SHOW SPACES');
      if (spacesResult.error) {
        throw new Error(`Failed to get spaces: ${spacesResult.error}`);
      }

      const spaces = spacesResult?.data || [];

      // 获取当前space的标签和边类型信息
      let tags: any[] = [];
      let edgeTypes: any[] = [];
      let nodeCount = 0;
      let edgeCount = 0;

      // 获取当前空间名称
      const currentSpace = this.connectionManager.getConnectionStatus().space;

      // 只有在当前空间存在的情况下才执行需要空间上下文的查询
      if (currentSpace) {
        const tagsResult = await this.connectionManager.executeQuery('SHOW TAGS');
        if (!tagsResult.error) {
          // 确保 tags 是一个数组
          tags = Array.isArray(tagsResult?.data) ? tagsResult.data : [];
        }

        const edgeTypesResult = await this.connectionManager.executeQuery('SHOW EDGES');
        if (!edgeTypesResult.error) {
          // 确保 edgeTypes 是一个数组
          edgeTypes = Array.isArray(edgeTypesResult?.data) ? edgeTypesResult.data : [];
        }

        // 统计节点数量
        for (const tag of tags) {
          try {
            // 获取标签名，可能是第一个字段的值
            const tagName = Object.values(tag)[0] || '';
            if (tagName) {
              const countResult = await this.connectionManager.executeQuery(`MATCH (n:${tagName}) RETURN count(n) AS count`);
              if (!countResult.error) {
                nodeCount += countResult?.data?.[0]?.count || 0;
              }
            }
          } catch (countError) {
            // 使用 DatabaseLoggerService 记录节点计数失败警告
            this.databaseLogger.logDatabaseEvent({
              type: DatabaseEventType.CONNECTION_ERROR,
              source: 'nebula',
              timestamp: new Date(),
              data: {
                message: `Failed to count nodes for tag`,
                tag,
                error: countError
              }
            }).catch(error => {
              console.error('Failed to log node count failure:', error);
            });
          }
        }

        // 统计边数量
        for (const edgeType of edgeTypes) {
          try {
            // 获取边类型名，可能是第一个字段的值
            const edgeTypeName = Object.values(edgeType)[0] || '';
            if (edgeTypeName) {
              const countResult = await this.connectionManager.executeQuery(`MATCH ()-[r:${edgeTypeName}]->() RETURN count(r) AS count`);
              if (!countResult.error) {
                edgeCount += countResult?.data?.[0]?.count || 0;
              }
            }
          } catch (countError) {
            // 使用 DatabaseLoggerService 记录边计数失败警告
            this.databaseLogger.logDatabaseEvent({
              type: DatabaseEventType.CONNECTION_ERROR,
              source: 'nebula',
              timestamp: new Date(),
              data: {
                message: `Failed to count edges for type`,
                edgeType,
                error: countError
              }
            }).catch(error => {
              console.error('Failed to log edge count failure:', error);
            });
          }
        }
      }

      return {
        version: '3.0.0',
        status: 'online',
        spaces: spaces.length,
        currentSpace: currentSpace || null,
        tags: tags.length,
        edgeTypes: edgeTypes.length,
        nodes: nodeCount,
        relationships: edgeCount,
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.errorHandler.handleError(
        new Error(`Failed to get database stats: ${errorMessage}`),
        {
          component: 'NebulaDataService',
          operation: 'getDatabaseStats'
        }
      );

      throw error;
    }
  }

  /**
   * 转义属性值中的特殊字符，防止nGQL注入
   */
  private escapeProperties(properties: Record<string, any>): Record<string, any> {
    const escaped: Record<string, any> = {};
    for (const [key, value] of Object.entries(properties)) {
      if (typeof value === 'string') {
        // 转义字符串中的引号和反斜杠
        escaped[key] = value.replace(/"/g, '\\"').replace(/\\/g, '\\\\');
      } else {
        escaped[key] = value;
      }
    }
    return escaped;
  }
}