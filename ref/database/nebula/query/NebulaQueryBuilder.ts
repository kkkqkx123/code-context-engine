import { injectable } from 'inversify';
import { BatchVertex, BatchEdge } from '../NebulaTypes';

export interface INebulaQueryBuilder {
  insertVertex(tag: string, vertexId: string, properties: Record<string, any>): { query: string; params: Record<string, any> };
  batchInsertVertices(vertices: BatchVertex[]): { query: string; params: Record<string, any> };
  insertEdge(edgeType: string, srcId: string, dstId: string, properties: Record<string, any>): { query: string; params: Record<string, any> };
  batchInsertEdges(edges: BatchEdge[]): { query: string; params: Record<string, any> };
  go(steps: number, vertexId: string, yieldFields: string, edgeType?: string): string;
  buildComplexTraversal(startId: string, edgeTypes: string[], options?: any): { query: string; params: Record<string, any> };
  buildShortestPath(sourceId: string, targetId: string, edgeTypes?: string[], maxDepth?: number): { query: string; params: Record<string, any> };
  updateVertex(vertexId: string, tag: string, properties: Record<string, any>): { query: string; params: Record<string, any> };
  updateEdge(srcId: string, dstId: string, edgeType: string, properties: Record<string, any>): { query: string; params: Record<string, any> };
  deleteVertices(vertexIds: string[], tag?: string): { query: string; params: Record<string, any> };
  deleteEdges(edges: Array<{ srcId: string; dstId: string; edgeType: string }>): { query: string; params: Record<string, any> };
  buildNodeCountQuery(tag: string): { query: string; params: Record<string, any> };
  buildRelationshipCountQuery(edgeType: string): { query: string; params: Record<string, any> };
}

@injectable()
export class NebulaQueryBuilder implements INebulaQueryBuilder {
  /**
   * 构建INSERT VERTEX语句
   * @param tag 标签名称
   * @param vertexId 顶点ID
   * @param properties 属性对象
   * @returns {query: string, params: Record<string, any>} 查询语句和参数
   */
  insertVertex(
    tag: string,
    vertexId: string,
    properties: Record<string, any>
  ): { query: string; params: Record<string, any> } {
    // 获取属性键
    const propertyKeys = Object.keys(properties);

    // 如果没有属性，构建简单的INSERT语句
    if (propertyKeys.length === 0) {
      const query = `INSERT VERTEX ${tag}() VALUES "${vertexId}":()`;
      return { query, params: {} };
    }

    // 构建属性名列表
    const propertyNames = propertyKeys.join(', ');

    // 构建参数占位符
    const paramPlaceholders = propertyKeys.map((_, index) => `$param${index}`).join(', ');

    // 构建查询语句
    const query = `INSERT VERTEX ${tag}(${propertyNames}) VALUES "${vertexId}":(${paramPlaceholders})`;

    // 构建参数对象
    const params: Record<string, any> = {};
    propertyKeys.forEach((key, index) => {
      params[`param${index}`] = properties[key];
    });

    return { query, params };
  }

  /**
   * 构建批量插入顶点语句
   * @param vertices 顶点数组
   * @returns {query: string, params: Record<string, any>} 查询语句和参数
   */
  batchInsertVertices(vertices: BatchVertex[]): { query: string; params: Record<string, any> } {
    if (vertices.length === 0) {
      return { query: '', params: {} };
    }

    // 按标签分组顶点以实现更高效的批量插入
    const verticesByTag = vertices.reduce(
      (acc, vertex) => {
        if (!acc[vertex.tag]) {
          acc[vertex.tag] = [];
        }
        acc[vertex.tag].push(vertex);
        return acc;
      },
      {} as Record<string, BatchVertex[]>
    );

    const queries: string[] = [];
    const params: Record<string, any> = {};

    for (const [tag, tagVertices] of Object.entries(verticesByTag)) {
      if (tagVertices.length === 0) continue;

      // 获取此标签所有顶点的所有属性键
      const allPropertyKeys = new Set<string>();
      tagVertices.forEach(vertex => {
        Object.keys(vertex.properties).forEach(key => allPropertyKeys.add(key));
      });

      const propertyNames = Array.from(allPropertyKeys).join(', ');

      // 为每个顶点构建值子句
      const valuesClauses = tagVertices
        .map((vertex, vertexIndex) => {
          const paramPrefix = `${tag}_${vertex.id.replace(/[^a-zA-Z0-9_]/g, '_')}`;
          const paramPlaceholders = Array.from(allPropertyKeys)
            .map((key, keyIndex) => {
              const paramName = `${paramPrefix}_param${keyIndex}`;
              params[paramName] = vertex.properties[key] || null;
              return `$${paramName}`;
            })
            .join(', ');

          return `"${vertex.id}":(${paramPlaceholders})`;
        })
        .join(', ');

      const query = `INSERT VERTEX ${tag}(${propertyNames}) VALUES ${valuesClauses}`;
      queries.push(query);
    }

    return { query: queries.join('; '), params };
  }

  /**
   * 构建INSERT EDGE语句
   * @param edgeType 边类型
   * @param srcId 源顶点ID
   * @param dstId 目标顶点ID
   * @param properties 属性对象
   * @returns {query: string, params: Record<string, any>} 查询语句和参数
   */
  insertEdge(
    edgeType: string,
    srcId: string,
    dstId: string,
    properties: Record<string, any>
  ): { query: string; params: Record<string, any> } {
    // 获取属性键
    const propertyKeys = Object.keys(properties);

    // 如果没有属性，构建简单的INSERT语句
    if (propertyKeys.length === 0) {
      const query = `INSERT EDGE ${edgeType}() VALUES "${srcId}"->"${dstId}":()`;
      return { query, params: {} };
    }

    // 构建属性名列表
    const propertyNames = propertyKeys.join(', ');

    // 构建参数占位符
    const paramPlaceholders = propertyKeys.map((_, index) => `$param${index}`).join(', ');

    // 构建查询语句
    const query = `INSERT EDGE ${edgeType}(${propertyNames}) VALUES "${srcId}"->"${dstId}":(${paramPlaceholders})`;

    // 构建参数对象
    const params: Record<string, any> = {};
    propertyKeys.forEach((key, index) => {
      params[`param${index}`] = properties[key];
    });

    return { query, params };
  }

  /**
   * 构建批量插入边语句
   * @param edges 边数组
   * @returns {query: string, params: Record<string, any>} 查询语句和参数
   */
  batchInsertEdges(
    edges: BatchEdge[]
  ): { query: string; params: Record<string, any> } {
    if (edges.length === 0) {
      return { query: '', params: {} };
    }

    // 按类型分组边以实现更高效的批量插入
    const edgesByType = edges.reduce(
      (acc, edge) => {
        if (!acc[edge.type]) {
          acc[edge.type] = [];
        }
        acc[edge.type].push(edge);
        return acc;
      },
      {} as Record<string, BatchEdge[]>
    );

    const queries: string[] = [];
    const params: Record<string, any> = {};

    for (const [edgeType, typeEdges] of Object.entries(edgesByType)) {
      if (typeEdges.length === 0) continue;

      // 获取此类型所有边的所有属性键
      const allPropertyKeys = new Set<string>();
      typeEdges.forEach(edge => {
        Object.keys(edge.properties).forEach(key => allPropertyKeys.add(key));
      });

      const propertyNames = Array.from(allPropertyKeys).join(', ');

      // 为每条边构建值子句
      const valuesClauses = typeEdges
        .map((edge, edgeIndex) => {
          const paramPrefix = `${edgeType}_${edge.srcId.replace(/[^a-zA-Z0-9_]/g, '_')}_${edge.dstId.replace(/[^a-zA-Z0-9_]/g, '_')}`;
          const paramPlaceholders = Array.from(allPropertyKeys)
            .map((key, keyIndex) => {
              const paramName = `${paramPrefix}_param${keyIndex}`;
              params[paramName] = edge.properties[key] || null;
              return `$${paramName}`;
            })
            .join(', ');

          return `"${edge.srcId}"->"${edge.dstId}":(${paramPlaceholders})`;
        })
        .join(', ');

      const query = `INSERT EDGE ${edgeType}(${propertyNames}) VALUES ${valuesClauses}`;
      queries.push(query);
    }

    return { query: queries.join('; '), params };
  }

  /**
   * 构建GO语句
   * @param steps 步数
   * @param vertexId 起始顶点ID
   * @param yieldFields 返回字段
   * @param edgeType 边类型（可选）
   * @returns 查询语句
   */
  go(steps: number, vertexId: string, yieldFields: string, edgeType?: string): string {
    let query = `GO ${steps} STEPS FROM "${vertexId}"`;

    if (edgeType) {
      query += ` OVER ${edgeType}`;
    }

    query += ` YIELD ${yieldFields}`;

    return query;
  }

  /**
   * 构建复杂图遍历查询
   * @param startId 起始顶点ID
   * @param edgeTypes 边类型数组
   * @param options 遍历选项
   * @returns {query: string, params: Record<string, any>} 查询语句和参数
   */
  buildComplexTraversal(
    startId: string,
    edgeTypes: string[],
    options: any = {}
  ): { query: string; params: Record<string, any> } {
    const { maxDepth = 3, filterConditions = [], returnFields = ['vertex'], limit = 10 } = options;

    const edgeTypeClause = edgeTypes.length > 0 ? `OVER ${edgeTypes.join(',')}` : 'OVER *';
    const filterClause =
      filterConditions.length > 0 ? `WHERE ${filterConditions.join(' AND ')}` : '';
    const returnClause = returnFields.join(', ');

    const query = `
      GO ${maxDepth} STEPS FROM $startId ${edgeTypeClause}
      YIELD dst(edge) AS destination
      ${filterClause}
      | FETCH PROP ON * $-.destination YIELD ${returnClause}
      LIMIT ${limit}
    `;

    return { query, params: { startId } };
  }

  /**
   * 构建最短路径查询
   * @param sourceId 源顶点ID
   * @param targetId 目标顶点ID
   * @param edgeTypes 边类型数组（可选）
   * @param maxDepth 最大深度
   * @returns {query: string, params: Record<string, any>} 查询语句和参数
   */
  buildShortestPath(
    sourceId: string,
    targetId: string,
    edgeTypes: string[] = [],
    maxDepth = 10
  ): { query: string; params: Record<string, any> } {
    const edgeTypeClause = edgeTypes.length > 0 ? `OVER ${edgeTypes.join(',')}` : 'OVER *';

    const query = `
      FIND SHORTEST PATH FROM $sourceId TO $targetId ${edgeTypeClause} UPTO ${maxDepth} STEPS
    `;

    return { query, params: { sourceId, targetId } };
  }

  /**
   * 构建节点属性更新查询
   * @param vertexId 顶点ID
   * @param tag 标签名称
   * @param properties 更新的属性对象
   * @returns {query: string, params: Record<string, any>} 查询语句和参数
   */
  updateVertex(
    vertexId: string,
    tag: string,
    properties: Record<string, any>
  ): { query: string; params: Record<string, any> } {
    const propertyKeys = Object.keys(properties);

    if (propertyKeys.length === 0) {
      return { query: '', params: {} };
    }

    const setClauses = propertyKeys
      .map((key, index) => {
        const paramName = `set_param${index}`;
        return `${key} = $${paramName}`;
      })
      .join(', ');

    const query = `
      UPDATE VERTEX ON ${tag} "${vertexId}"
      SET ${setClauses}
    `;

    const params: Record<string, any> = {};
    propertyKeys.forEach((key, index) => {
      params[`set_param${index}`] = properties[key];
    });

    return { query, params };
  }

  /**
   * 构建边属性更新查询
   * @param srcId 源顶点ID
   * @param dstId 目标顶点ID
   * @param edgeType 边类型
   * @param properties 更新的属性对象
   * @returns {query: string, params: Record<string, any>} 查询语句和参数
   */
  updateEdge(
    srcId: string,
    dstId: string,
    edgeType: string,
    properties: Record<string, any>
  ): { query: string; params: Record<string, any> } {
    const propertyKeys = Object.keys(properties);

    if (propertyKeys.length === 0) {
      return { query: '', params: {} };
    }

    const setClauses = propertyKeys
      .map((key, index) => {
        const paramName = `set_param${index}`;
        return `${key} = $${paramName}`;
      })
      .join(', ');

    const query = `
      UPDATE EDGE ON ${edgeType} "${srcId}" -> "${dstId}"
      SET ${setClauses}
    `;

    const params: Record<string, any> = {};
    propertyKeys.forEach((key, index) => {
      params[`set_param${index}`] = properties[key];
    });

    return { query, params };
  }

  /**
   * 构建删除顶点查询
   * @param vertexIds 顶点ID数组
   * @param tag 标签名称（可选）
   * @returns {query: string, params: Record<string, any>} 查询语句和参数
   */
  deleteVertices(
    vertexIds: string[],
    tag?: string
  ): { query: string; params: Record<string, any> } {
    if (vertexIds.length === 0) {
      return { query: '', params: {} };
    }

    const idParams = vertexIds.map((id, index) => `$id${index}`).join(', ');
    const params: Record<string, any> = {};
    vertexIds.forEach((id, index) => {
      params[`id${index}`] = id;
    });

    let query = `DELETE VERTEX ${idParams}`;
    if (tag) {
      query += ` TAG ${tag}`;
    }

    return { query, params };
  }

  /**
   * 构建删除边查询
   * @param edges 边数组
   * @returns {query: string, params: Record<string, any>} 查询语句和参数
   */
  deleteEdges(edges: Array<{ srcId: string; dstId: string; edgeType: string }>): {
    query: string;
    params: Record<string, any>;
  } {
    if (edges.length === 0) {
      return { query: '', params: {} };
    }

    const params: Record<string, any> = {};

    const deleteClauses = edges
      .map((edge, index) => {
        const srcParam = `src${index}`;
        const dstParam = `dst${index}`;
        params[srcParam] = edge.srcId;
        params[dstParam] = edge.dstId;
        return `$${srcParam} -> $${dstParam}`;
      })
      .join(', ');

    const query = `DELETE EDGE ${deleteClauses}`;
    return { query, params };
  }

  /**
   * 构建节点计数查询
   * @param tag 标签名称
   * @returns {query: string, params: Record<string, any>} 查询语句和参数
   */
  buildNodeCountQuery(tag: string): { query: string; params: Record<string, any> } {
    const query = `
      MATCH (n:${tag}) 
      RETURN count(n) AS total
    `;
    return { query, params: {} };
  }

  /**
   * 构建关系计数查询
   * @param edgeType 边类型
   * @returns {query: string, params: Record<string, any>} 查询语句和参数
   */
  buildRelationshipCountQuery(edgeType: string): { query: string; params: Record<string, any> } {
    const query = `
      MATCH ()-[r:${edgeType}]->() 
      RETURN count(r) AS total
    `;
    return { query, params: {} };
  }
}