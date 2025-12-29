import { DatabaseError } from './DatabaseError';

/**
 * 数据库服务验证器
 * 提供统一的输入验证和参数检查功能
 */
export class DatabaseServiceValidator {

  /**
   * 验证项目路径
   */
  static validateProjectPath(projectPath: string): void {
    if (!projectPath || typeof projectPath !== 'string') {
      throw DatabaseError.validationError(
        'Project path must be a non-empty string',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateProjectPath',
          details: { projectPath }
        }
      );
    }

    // 检查路径是否包含非法字符
    if (/[<>:"/\\|?*]/.test(projectPath)) {
      throw DatabaseError.validationError(
        'Project path contains invalid characters',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateProjectPath',
          details: { projectPath }
        }
      );
    }
  }

  /**
   * 验证ID参数
   */
  static validateId(id: string): void {
    if (!id || typeof id !== 'string') {
      throw DatabaseError.validationError(
        'ID must be a non-empty string',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateId',
          details: { id }
        }
      );
    }

    if (id.length > 100) {
      throw DatabaseError.validationError(
        'ID is too long (max 100 characters)',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateId',
          details: { id }
        }
      );
    }
  }

  /**
   * 验证数据对象
   */
  static validateData(data: any): void {
    if (data === null || data === undefined) {
      throw DatabaseError.validationError(
        'Data cannot be null or undefined',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateData',
          details: { data }
        }
      );
    }

    if (typeof data !== 'object') {
      throw DatabaseError.validationError(
        'Data must be an object',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateData',
          details: { data }
        }
      );
    }
  }

  /**
   * 验证查询参数
   */
  static validateQuery(query: any): void {
    if (query === null || query === undefined) {
      throw DatabaseError.validationError(
        'Query cannot be null or undefined',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateQuery',
          details: { query }
        }
      );
    }

    if (typeof query !== 'object' && typeof query !== 'string') {
      throw DatabaseError.validationError(
        'Query must be an object or string',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateQuery',
          details: { query }
        }
      );
    }
  }

  /**
    * 验证配置对象
    */
  static validateConfig(config: any): void {
    if (config !== null && config !== undefined && typeof config !== 'object') {
      throw DatabaseError.validationError(
        'Config must be an object, null, or undefined',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateConfig',
          details: { config }
        }
      );
    }
  }

  /**
   * 验证空间名称
   */
  static validateSpaceName(spaceName: string): void {
    if (!spaceName || typeof spaceName !== 'string') {
      throw DatabaseError.validationError(
        'Space name must be a non-empty string',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateSpaceName',
          details: { spaceName }
        }
      );
    }

    // Nebula Graph空间名称规则：只能包含字母、数字、下划线，且不能以数字开头
    if (!/^[a-zA-Z_][a-zA-Z0-9_]*$/.test(spaceName)) {
      throw DatabaseError.validationError(
        'Space name must start with a letter or underscore and contain only letters, numbers, and underscores',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateSpaceName',
          details: { spaceName }
        }
      );
    }

    if (spaceName.length > 64) {
      throw DatabaseError.validationError(
        'Space name is too long (max 64 characters)',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateSpaceName',
          details: { spaceName }
        }
      );
    }
  }

  /**
   * 验证集合名称（用于Qdrant）
   */
  static validateCollectionName(collectionName: string): void {
    if (!collectionName || typeof collectionName !== 'string') {
      throw DatabaseError.validationError(
        'Collection name must be a non-empty string',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateCollectionName',
          details: { collectionName }
        }
      );
    }

    // Qdrant集合名称规则：只能包含字母、数字、连字符、下划线和点
    if (!/^[a-zA-Z0-9_-][a-zA-Z0-9_.-]*$/.test(collectionName)) {
      throw DatabaseError.validationError(
        'Collection name contains invalid characters',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateCollectionName',
          details: { collectionName }
        }
      );
    }

    if (collectionName.length > 255) {
      throw DatabaseError.validationError(
        'Collection name is too long (max 255 characters)',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateCollectionName',
          details: { collectionName }
        }
      );
    }
  }

  /**
   * 验证向量大小
   */
  static validateVectorSize(vectorSize: number): void {
    if (typeof vectorSize !== 'number' || vectorSize <= 0 || vectorSize > 65536) {
      throw DatabaseError.validationError(
        'Vector size must be a positive number between 1 and 65536',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateVectorSize',
          details: { vectorSize }
        }
      );
    }
  }

  /**
   * 验证点ID列表
   */
  static validatePointIds(pointIds: string[]): void {
    if (!Array.isArray(pointIds)) {
      throw DatabaseError.validationError(
        'Point IDs must be an array',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validatePointIds',
          details: { pointIds }
        }
      );
    }

    for (const id of pointIds) {
      if (typeof id !== 'string' || !id) {
        throw DatabaseError.validationError(
          'Each point ID must be a non-empty string',
          {
            component: 'DatabaseServiceValidator',
            operation: 'validatePointIds',
            details: { pointIds }
          }
        );
      }
    }
  }

  /**
   * 验证节点数据
   */
  static validateNode(node: any): void {
    if (!node || typeof node !== 'object') {
      throw DatabaseError.validationError(
        'Node must be a valid object',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateNode',
          details: { node }
        }
      );
    }

    if (!node.id || typeof node.id !== 'string') {
      throw DatabaseError.validationError(
        'Node must have a valid string ID',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateNode',
          details: { node }
        }
      );
    }

    if (!node.label || typeof node.label !== 'string') {
      throw DatabaseError.validationError(
        'Node must have a valid string label',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateNode',
          details: { node }
        }
      );
    }

    if (node.properties && typeof node.properties !== 'object') {
      throw DatabaseError.validationError(
        'Node properties must be an object',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateNode',
          details: { node }
        }
      );
    }
  }

  /**
    * 验证关系数据
    */
  static validateRelationship(relationship: any): void {
    if (!relationship || typeof relationship !== 'object') {
      throw DatabaseError.validationError(
        'Relationship must be a valid object',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateRelationship',
          details: { relationship }
        }
      );
    }

    if (!relationship.type || typeof relationship.type !== 'string') {
      throw DatabaseError.validationError(
        'Relationship must have a valid string type',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateRelationship',
          details: { relationship }
        }
      );
    }

    if (!relationship.sourceId || typeof relationship.sourceId !== 'string') {
      throw DatabaseError.validationError(
        'Relationship must have a valid string sourceId',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateRelationship',
          details: { relationship }
        }
      );
    }

    if (!relationship.targetId || typeof relationship.targetId !== 'string') {
      throw DatabaseError.validationError(
        'Relationship must have a valid string targetId',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateRelationship',
          details: { relationship }
        }
      );
    }

    if (relationship.properties && typeof relationship.properties !== 'object') {
      throw DatabaseError.validationError(
        'Relationship properties must be an object',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateRelationship',
          details: { relationship }
        }
      );
    }
  }

  /**
    * 验证节点数组
    */
  static validateNodes(nodes: any[]): void {
    if (!Array.isArray(nodes)) {
      throw DatabaseError.validationError(
        'Nodes must be an array',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateNodes',
          details: { nodes }
        }
      );
    }

    for (const node of nodes) {
      this.validateNode(node);
    }
  }

  /**
   * 验证关系数组
   */
  static validateRelationships(relationships: any[]): void {
    if (!Array.isArray(relationships)) {
      throw DatabaseError.validationError(
        'Relationships must be an array',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateRelationships',
          details: { relationships }
        }
      );
    }

    for (const relationship of relationships) {
      this.validateRelationship(relationship);
    }
  }

  /**
   * 验证搜索选项
   */
  static validateSearchOptions(options: any): void {
    if (options && typeof options !== 'object') {
      throw DatabaseError.validationError(
        'Search options must be an object or null/undefined',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateSearchOptions',
          details: { options }
        }
      );
    }

    if (options && options.limit && (typeof options.limit !== 'number' || options.limit <= 0)) {
      throw DatabaseError.validationError(
        'Search limit must be a positive number',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateSearchOptions',
          details: { options }
        }
      );
    }

    if (options && options.offset && (typeof options.offset !== 'number' || options.offset < 0)) {
      throw DatabaseError.validationError(
        'Search offset must be a non-negative number',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateSearchOptions',
          details: { options }
        }
      );
    }
  }

  /**
   * 验证向量点
   */
  static validateVectorPoint(point: any): void {
    if (!point || typeof point !== 'object') {
      throw DatabaseError.validationError(
        'Vector point must be a valid object',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateVectorPoint',
          details: { point }
        }
      );
    }

    if (!point.id || typeof point.id !== 'string') {
      throw DatabaseError.validationError(
        'Vector point must have a valid string ID',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateVectorPoint',
          details: { point }
        }
      );
    }

    if (!point.vector || !Array.isArray(point.vector)) {
      throw DatabaseError.validationError(
        'Vector point must have a valid vector array',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateVectorPoint',
          details: { point }
        }
      );
    }

    if (point.vector.some((v: any) => typeof v !== 'number')) {
      throw DatabaseError.validationError(
        'Vector values must be numbers',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateVectorPoint',
          details: { point }
        }
      );
    }
  }

  /**
   * 验证向量点数组
   */
  static validateVectorPoints(points: any[]): void {
    if (!Array.isArray(points)) {
      throw DatabaseError.validationError(
        'Vector points must be an array',
        {
          component: 'DatabaseServiceValidator',
          operation: 'validateVectorPoints',
          details: { points }
        }
      );
    }

    for (const point of points) {
      this.validateVectorPoint(point);
    }
  }
}