import { injectable, inject } from 'inversify';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { IQueryRunner } from '../query/QueryRunner';
import { LRUCache } from '../../../utils/cache/LRUCache';
import { TYPES } from '../../../types';

// Space 验证结果
export interface SpaceValidationResult {
  isValid: boolean;
  exists: boolean;
  isAccessible: boolean;
  error?: SpaceValidationError;
  spaceInfo?: NebulaSpaceInfo;
}

// Space 验证错误类型
export enum SpaceValidationErrorType {
  INVALID_FORMAT = 'INVALID_FORMAT',
  NOT_EXISTS = 'NOT_EXISTS',
  NOT_ACCESSIBLE = 'NOT_ACCESSIBLE',
  PERMISSION_DENIED = 'PERMISSION_DENIED',
  UNKNOWN_ERROR = 'UNKNOWN_ERROR'
}

// Space 验证错误
export interface SpaceValidationError {
  type: SpaceValidationErrorType;
  message: string;
  details?: any;
  suggestions?: string[];
}

// Nebula Space 信息
export interface NebulaSpaceInfo {
  name: string;
  id?: number;
  partitionNum?: number;
  replicaFactor?: number;
  vidType?: string;
  charset?: string;
  collate?: string;
  comment?: string;
}

// Space 验证器配置
export interface SpaceValidatorConfig {
  enableCache: boolean;
  cacheSize: number;
  cacheTTL: number; // 缓存过期时间（毫秒）
  validationTimeout: number; // 验证超时时间（毫秒）
}

// 默认配置
const DEFAULT_SPACE_VALIDATOR_CONFIG: SpaceValidatorConfig = {
  enableCache: true,
  cacheSize: 100,
  cacheTTL: 300000, // 5分钟
  validationTimeout: 10000 // 10秒
};

// 缓存条目
interface CacheEntry {
  result: SpaceValidationResult;
  timestamp: number;
}

/**
 * Space 验证器
 * 负责验证 Nebula Graph space 的有效性和可访问性
 */
@injectable()
export class SpaceValidator {
  private logger: LoggerService;
  private errorHandler: ErrorHandlerService;
  private queryRunner: IQueryRunner;
  private config: SpaceValidatorConfig;
  private cache: LRUCache<string, CacheEntry>;

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.IQueryRunner) queryRunner: IQueryRunner,
    config: Partial<SpaceValidatorConfig> = {}
  ) {
    this.logger = logger;
    this.errorHandler = errorHandler;
    this.queryRunner = queryRunner;
    this.config = { ...DEFAULT_SPACE_VALIDATOR_CONFIG, ...config };

    // 初始化缓存
    this.cache = new LRUCache<string, CacheEntry>(this.config.cacheSize, {
      enableStats: true
    });

    this.logger.info('SpaceValidator initialized', { config: this.config });
  }

  /**
   * 验证 space
   * @param spaceName space 名称
   * @param forceValidate 是否强制验证（忽略缓存）
   * @returns 验证结果
   */
  async validateSpace(spaceName: string, forceValidate: boolean = false): Promise<SpaceValidationResult> {
    if (!spaceName) {
      return {
        isValid: false,
        exists: false,
        isAccessible: false,
        error: {
          type: SpaceValidationErrorType.INVALID_FORMAT,
          message: 'Space name cannot be empty',
          suggestions: ['Please provide a valid space name']
        }
      };
    }

    // 检查缓存
    if (!forceValidate && this.config.enableCache) {
      const cachedResult = this.getCachedResult(spaceName);
      if (cachedResult) {
        this.logger.debug('Space validation result retrieved from cache', { spaceName });
        return cachedResult;
      }
    }

    try {
      this.logger.debug('Validating space', { spaceName });

      // 1. 格式验证
      const formatValidation = this.validateSpaceFormat(spaceName);
      if (!formatValidation.isValid) {
        const result = {
          isValid: false,
          exists: false,
          isAccessible: false,
          error: formatValidation.error
        } as SpaceValidationResult;

        this.setCachedResult(spaceName, result);
        return result;
      }

      // 2. 存在性验证
      const existenceResult = await this.checkSpaceExistence(spaceName);
      if (!existenceResult.exists) {
        const result = {
          isValid: false,
          exists: false,
          isAccessible: false,
          error: existenceResult.error
        } as SpaceValidationResult;

        this.setCachedResult(spaceName, result);
        return result;
      }

      // 3. 可访问性验证
      const accessibilityResult = await this.checkSpaceAccessibility(spaceName);
      if (!accessibilityResult.isAccessible) {
        const result = {
          isValid: false,
          exists: true,
          isAccessible: false,
          error: accessibilityResult.error
        } as SpaceValidationResult;

        this.setCachedResult(spaceName, result);
        return result;
      }

      // 验证成功
      const result: SpaceValidationResult = {
        isValid: true,
        exists: true,
        isAccessible: true,
        spaceInfo: existenceResult.spaceInfo
      };

      this.setCachedResult(spaceName, result);
      this.logger.info('Space validation successful', { spaceName });

      return result;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.logger.error('Space validation failed', { spaceName, error: errorMessage });

      const result: SpaceValidationResult = {
        isValid: false,
        exists: false,
        isAccessible: false,
        error: {
          type: SpaceValidationErrorType.UNKNOWN_ERROR,
          message: `Unknown error during space validation: ${errorMessage}`,
          details: error
        }
      };

      // 不缓存未知错误的结果
      return result;
    }
  }

  /**
   * 批量验证 space
   * @param spaceNames space 名称列表
   * @returns 验证结果映射
   */
  async validateSpaces(spaceNames: string[]): Promise<Map<string, SpaceValidationResult>> {
    const results = new Map<string, SpaceValidationResult>();

    // 获取所有可用 space（一次性查询）
    let allSpaces: string[] = [];
    try {
      allSpaces = await this.getAllSpaces();
    } catch (error) {
      this.logger.error('Failed to get all spaces for batch validation', { error });
    }

    // 并行验证所有 space
    const validationPromises = spaceNames.map(async (spaceName) => {
      const result = await this.validateSpace(spaceName);
      return { spaceName, result };
    });

    const validationResults = await Promise.allSettled(validationPromises);

    // 处理结果
    validationResults.forEach((promiseResult, index) => {
      if (promiseResult.status === 'fulfilled') {
        const { spaceName, result } = promiseResult.value;
        results.set(spaceName, result);
      } else {
        const spaceName = spaceNames[index];
        const error = promiseResult.reason;
        this.logger.error('Space validation failed in batch', { spaceName, error });

        results.set(spaceName, {
          isValid: false,
          exists: false,
          isAccessible: false,
          error: {
            type: SpaceValidationErrorType.UNKNOWN_ERROR,
            message: `Batch validation failed: ${error instanceof Error ? error.message : String(error)}`
          }
        });
      }
    });

    return results;
  }

  /**
   * 清除缓存
   * @param spaceName 可选，指定要清除的 space 名称，不指定则清除所有缓存
   */
  clearCache(spaceName?: string): void {
    if (spaceName) {
      this.cache.delete(spaceName);
      this.logger.debug('Space validation cache cleared for space', { spaceName });
    } else {
      this.cache.clear();
      this.logger.debug('All space validation cache cleared');
    }
  }

  /**
   * 获取缓存统计信息
   */
  getCacheStats(): any {
    return this.cache.getStats();
  }

  /**
   * 验证 space 名称格式
   * @param spaceName space 名称
   * @returns 格式验证结果
   */
  private validateSpaceFormat(spaceName: string): { isValid: boolean; error?: SpaceValidationError } {
    // 检查是否为空
    if (!spaceName || spaceName.trim() === '') {
      return {
        isValid: false,
        error: {
          type: SpaceValidationErrorType.INVALID_FORMAT,
          message: 'Space name cannot be empty',
          suggestions: ['Please provide a valid space name']
        }
      };
    }

    const trimmedName = spaceName.trim();

    // 检查长度（Nebula Graph 没有明确限制，但建议不超过 256 字符）
    if (trimmedName.length > 256) {
      return {
        isValid: false,
        error: {
          type: SpaceValidationErrorType.INVALID_FORMAT,
          message: 'Space name is too long (maximum 256 characters)',
          suggestions: ['Use a shorter space name']
        }
      };
    }

    // 检查是否以数字开头
    if (/^\d/.test(trimmedName)) {
      return {
        isValid: false,
        error: {
          type: SpaceValidationErrorType.INVALID_FORMAT,
          message: 'Space name cannot start with a number',
          suggestions: ['Start the space name with a letter or underscore']
        }
      };
    }

    // 检查字符限制（允许字母、数字、下划线、中文）
    if (!/^[a-zA-Z0-9_\u4e00-\u9fa5]+$/.test(trimmedName)) {
      return {
        isValid: false,
        error: {
          type: SpaceValidationErrorType.INVALID_FORMAT,
          message: 'Space name contains invalid characters',
          details: {
            allowedCharacters: 'Letters, numbers, underscores, and Chinese characters',
            actualName: trimmedName
          },
          suggestions: [
            'Use only letters, numbers, underscores, and Chinese characters',
            'Avoid special characters and spaces'
          ]
        }
      };
    }

    // 检查是否为保留关键字（简单检查）
    const reservedKeywords = ['space', 'tag', 'edge', 'index', 'user', 'role', 'graph'];
    if (reservedKeywords.includes(trimmedName.toLowerCase())) {
      return {
        isValid: false,
        error: {
          type: SpaceValidationErrorType.INVALID_FORMAT,
          message: 'Space name cannot be a reserved keyword',
          details: { reservedKeywords },
          suggestions: ['Choose a different name that is not a reserved keyword']
        }
      };
    }

    return { isValid: true };
  }

  /**
   * 检查 space 是否存在
   * @param spaceName space 名称
   * @returns 存在性检查结果
   */
  private async checkSpaceExistence(spaceName: string): Promise<{ exists: boolean; spaceInfo?: NebulaSpaceInfo; error?: SpaceValidationError }> {
    try {
      // 获取所有 space
      const allSpaces = await this.getAllSpaces();

      // 检查目标 space 是否存在
      if (!allSpaces.includes(spaceName)) {
        return {
          exists: false,
          error: {
            type: SpaceValidationErrorType.NOT_EXISTS,
            message: `Space '${spaceName}' does not exist`,
            details: { availableSpaces: allSpaces },
            suggestions: [
              'Create the space first using CREATE SPACE statement',
              'Check if the space name is spelled correctly',
              `Available spaces: ${allSpaces.join(', ')}`
            ]
          }
        };
      }

      // 获取 space 详细信息
      const spaceInfo = await this.getSpaceInfo(spaceName);

      return { exists: true, spaceInfo };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        exists: false,
        error: {
          type: SpaceValidationErrorType.UNKNOWN_ERROR,
          message: `Failed to check space existence: ${errorMessage}`,
          details: error
        }
      };
    }
  }

  /**
   * 检查 space 是否可访问
   * @param spaceName space 名称
   * @returns 可访问性检查结果
   */
  private async checkSpaceAccessibility(spaceName: string): Promise<{ isAccessible: boolean; error?: SpaceValidationError }> {
    try {
      // 尝试使用 space
      const useQuery = `USE \`${spaceName}\`;`;
      const result = await this.queryRunner.execute(useQuery);

      if (result.error) {
        // 检查是否是权限错误
        if (result.error.toLowerCase().includes('permission') ||
          result.error.toLowerCase().includes('access') ||
          result.error.toLowerCase().includes('auth')) {
          return {
            isAccessible: false,
            error: {
              type: SpaceValidationErrorType.PERMISSION_DENIED,
              message: `Permission denied when accessing space '${spaceName}'`,
              details: { originalError: result.error },
              suggestions: [
                'Check if the user has the required permissions',
                'Grant the necessary permissions to the user',
                'Contact the database administrator'
              ]
            }
          };
        }

        return {
          isAccessible: false,
          error: {
            type: SpaceValidationErrorType.NOT_ACCESSIBLE,
            message: `Cannot access space '${spaceName}'`,
            details: { originalError: result.error },
            suggestions: [
              'Check if the space is in a valid state',
              'Verify the space configuration',
              'Contact the database administrator'
            ]
          }
        };
      }

      return { isAccessible: true };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);

      // 检查是否是权限错误
      if (errorMessage.toLowerCase().includes('permission') ||
        errorMessage.toLowerCase().includes('access') ||
        errorMessage.toLowerCase().includes('auth')) {
        return {
          isAccessible: false,
          error: {
            type: SpaceValidationErrorType.PERMISSION_DENIED,
            message: `Permission denied when accessing space '${spaceName}'`,
            details: { originalError: errorMessage },
            suggestions: [
              'Check if the user has the required permissions',
              'Grant the necessary permissions to the user',
              'Contact the database administrator'
            ]
          }
        };
      }

      return {
        isAccessible: false,
        error: {
          type: SpaceValidationErrorType.NOT_ACCESSIBLE,
          message: `Cannot access space '${spaceName}'`,
          details: { originalError: errorMessage },
          suggestions: [
            'Check if the space is in a valid state',
            'Verify the space configuration',
            'Contact the database administrator'
          ]
        }
      };
    }
  }

  /**
   * 获取所有 space 名称
   * @returns space 名称列表
   */
  private async getAllSpaces(): Promise<string[]> {
    try {
      const result = await this.queryRunner.execute('SHOW SPACES;');

      if (result.error) {
        throw new Error(`Failed to get spaces: ${result.error}`);
      }

      // 解析结果
      const spaces: string[] = [];
      if (result.table && result.table.data) {
        for (const row of result.table.data) {
          if (row && row.length > 0) {
            spaces.push(String(row[0]));
          }
        }
      }

      return spaces;
    } catch (error) {
      this.logger.error('Failed to get all spaces', { error });
      throw error;
    }
  }

  /**
   * 获取 space 详细信息
   * @param spaceName space 名称
   * @returns space 信息
   */
  private async getSpaceInfo(spaceName: string): Promise<NebulaSpaceInfo | undefined> {
    try {
      const result = await this.queryRunner.execute(`DESCRIBE SPACE \`${spaceName}\`;`);

      if (result.error) {
        this.logger.warn('Failed to get space info', { spaceName, error: result.error });
        return undefined;
      }

      // 解析结果
      if (result.table && result.table.data && result.table.data.length > 0) {
        const row = result.table.data[0];
        if (row && row.length >= 8) {
          return {
            name: spaceName,
            id: parseInt(String(row[0])),
            partitionNum: parseInt(String(row[1])),
            replicaFactor: parseInt(String(row[2])),
            charset: String(row[3]),
            collate: String(row[4]),
            vidType: String(row[5]),
            comment: String(row[7])
          };
        }
      }

      return undefined;
    } catch (error) {
      this.logger.warn('Failed to get space info', { spaceName, error });
      return undefined;
    }
  }

  /**
   * 获取缓存的验证结果
   * @param spaceName space 名称
   * @returns 缓存的验证结果或 null
   */
  private getCachedResult(spaceName: string): SpaceValidationResult | null {
    if (!this.config.enableCache) {
      return null;
    }

    const cachedEntry = this.cache.get(spaceName);
    if (!cachedEntry) {
      return null;
    }

    // 检查缓存是否过期
    const now = Date.now();
    if (now - cachedEntry.timestamp > this.config.cacheTTL) {
      this.cache.delete(spaceName);
      return null;
    }

    return cachedEntry.result;
  }

  /**
   * 设置缓存的验证结果
   * @param spaceName space 名称
   * @param result 验证结果
   */
  private setCachedResult(spaceName: string, result: SpaceValidationResult): void {
    if (!this.config.enableCache) {
      return;
    }

    // 不缓存未知错误的结果
    if (result.error?.type === SpaceValidationErrorType.UNKNOWN_ERROR) {
      return;
    }

    const cacheEntry: CacheEntry = {
      result,
      timestamp: Date.now()
    };

    this.cache.set(spaceName, cacheEntry);
  }
}