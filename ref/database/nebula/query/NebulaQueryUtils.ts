import { injectable } from 'inversify';

export interface INebulaQueryUtils {
  interpolateParameters(nGQL: string, parameters: Record<string, any>): string;
  escapeValue(value: any): string;
  escapeProperties(properties: Record<string, any>): Record<string, any>;
  validateQuery(nGQL: string): boolean;
  detectQueryType(nGQL: string): QueryType;
}

export enum QueryType {
  DDL = 'DDL',       // CREATE, DROP, ALTER
  DML = 'DML',       // INSERT, UPDATE, DELETE
  QUERY = 'QUERY',   // MATCH, FETCH, GO
  ADMIN = 'ADMIN',   // SHOW, DESCRIBE
  OTHER = 'OTHER'
}

@injectable()
export class NebulaQueryUtils implements INebulaQueryUtils {
  /**
   * 插值参数到nGQL查询中
   */
  static interpolateParameters(nGQL: string, parameters: Record<string, any>): string {
    if (!parameters || Object.keys(parameters).length === 0) {
      return nGQL;
    }

    let interpolatedQuery = nGQL;

    for (const [key, value] of Object.entries(parameters)) {
      const placeholder = `:${key}`;
      const escapedValue = NebulaQueryUtils.escapeValue(value);

      interpolatedQuery = interpolatedQuery.replace(
        new RegExp(placeholder.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'g'),
        escapedValue
      );
    }

    return interpolatedQuery;
  }

  /**
   * 转义值，防止nGQL注入
   */
  static escapeValue(value: any): string {
    if (value === null || value === undefined) {
      return 'NULL';
    }

    if (typeof value === 'number' || typeof value === 'boolean') {
      return String(value);
    }

    if (typeof value === 'string') {
      // 转义引号和反斜杠
      const escaped = value
        .replace(/\\/g, '\\\\')
        .replace(/"/g, '\\"')
        .replace(/'/g, "\\'");

      return `"${escaped}"`;
    }

    if (Array.isArray(value)) {
      return `[${value.map(v => NebulaQueryUtils.escapeValue(v)).join(', ')}]`;
    }

    if (typeof value === 'object') {
      return JSON.stringify(NebulaQueryUtils.escapeProperties(value));
    }

    return String(value);
  }

  /**
   * 转义属性值中的特殊字符，防止nGQL注入
   * 在Nebula Graph的nGQL中，数字值不应该加引号，而字符串值需要加引号。
   */
  static escapeProperties(properties: Record<string, any>): Record<string, any> {
    const escaped: Record<string, any> = {};
    for (const [key, value] of Object.entries(properties)) {
      // 对于数字类型或可以转换为数字的字符串，我们只转换为字符串而不添加引号
      if (typeof value === 'number') {
        escaped[key] = String(value);
      } else if (typeof value === 'string' && !isNaN(Number(value)) && !isNaN(parseFloat(value))) {
        // 如果是纯数字字符串，不加引号
        escaped[key] = value;
      } else {
        escaped[key] = NebulaQueryUtils.escapeValue(value);
      }
    }
    return escaped;
  }

  /**
   * 验证查询是否安全
   */
  static validateQuery(nGQL: string): boolean {
    if (!nGQL || typeof nGQL !== 'string' || nGQL.trim() === '') {
      return false;
    }

    // 检查是否包含潜在的危险操作
    const dangerousPatterns = [
      /DROP\s+SPACE/i,
      /DELETE\s+FROM/i,
      /TRUNCATE/i,
      /SYSTEM/i
    ];

    return !dangerousPatterns.some(pattern => pattern.test(nGQL));
  }

  /**
   * 检测查询类型
   */
  static detectQueryType(nGQL: string): QueryType {
    const trimmedQuery = nGQL.trim().toUpperCase();

    if (trimmedQuery.startsWith('CREATE') || trimmedQuery.startsWith('DROP') || trimmedQuery.startsWith('ALTER')) {
      return QueryType.DDL;
    } else if (trimmedQuery.startsWith('INSERT') || trimmedQuery.startsWith('UPDATE') || trimmedQuery.startsWith('DELETE')) {
      return QueryType.DML;
    } else if (trimmedQuery.startsWith('MATCH') || trimmedQuery.startsWith('FETCH') || trimmedQuery.startsWith('GO')) {
      return QueryType.QUERY;
    } else if (trimmedQuery.startsWith('SHOW') || trimmedQuery.startsWith('DESCRIBE')) {
      return QueryType.ADMIN;
    } else {
      return QueryType.OTHER;
    }
  }

  // 实现接口方法
  interpolateParameters(nGQL: string, parameters: Record<string, any>): string {
    return NebulaQueryUtils.interpolateParameters(nGQL, parameters);
  }

  escapeValue(value: any): string {
    return NebulaQueryUtils.escapeValue(value);
  }

  escapeProperties(properties: Record<string, any>): Record<string, any> {
    return NebulaQueryUtils.escapeProperties(properties);
  }

  validateQuery(nGQL: string): boolean {
    return NebulaQueryUtils.validateQuery(nGQL);
  }

  detectQueryType(nGQL: string): QueryType {
    return NebulaQueryUtils.detectQueryType(nGQL);
  }
}