import { injectable } from 'inversify';
import * as Joi from 'joi';
import { BaseConfigService } from './BaseConfigService';
import { ProjectIdManager } from '../../database/ProjectIdManager';

export interface ProjectNamingConfig {
  qdrant: {
    defaultCollection: string;
    namingPattern: string;
  };
  nebula: {
    defaultSpace: string;
    namingPattern: string;
  };
}

@injectable()
export class ProjectNamingConfigService extends BaseConfigService<ProjectNamingConfig> {

  constructor() {
    super();
    // BaseConfigService uses lazy loading, no need to initialize config here
  }

  loadConfig(): ProjectNamingConfig {
    const rawConfig = {
      qdrant: {
        defaultCollection: process.env.PROJECT_QDRANT_DEFAULT_COLLECTION || 'default',
        namingPattern: process.env.PROJECT_QDRANT_NAMING_PATTERN || '{projectId}',
      },
      nebula: {
        defaultSpace: process.env.PROJECT_NEBULA_DEFAULT_SPACE || 'default',
        namingPattern: process.env.PROJECT_NEBULA_NAMING_PATTERN || '{projectId}',
      },
    };

    return this.validateConfig(rawConfig);
  }

  validateConfig(config: any): ProjectNamingConfig {
    const schema = Joi.object({
      qdrant: Joi.object({
        defaultCollection: Joi.string().pattern(/^[a-zA-Z0-9_-]+$/).default('default'),
        namingPattern: Joi.string().default('{projectId}'),
      }),
      nebula: Joi.object({
        defaultSpace: Joi.string().pattern(/^[a-zA-Z0-9_-]+$/).default('default'),
        namingPattern: Joi.string().default('{projectId}'),
      }),
    });

    const { error, value } = schema.validate(config);
    if (error) {
      throw new Error(`Project naming config validation error: ${error.message}`);
    }

    return value;
  }

  getDefaultConfig(): ProjectNamingConfig {
    return {
      qdrant: {
        defaultCollection: 'default',
        namingPattern: '{projectId}',
      },
      nebula: {
        defaultSpace: 'default',
        namingPattern: '{projectId}',
      },
    };
  }

  /**
   * 获取Qdrant集合名称，实现配置优先级逻辑
   * 1. 检查显式环境配置（保持向后兼容）
   * 2. 使用配置服务的默认集合
   * 3. 项目隔离的动态命名
   * 4. 默认回退
   */
  getQdrantCollectionName(projectId: string): string {
    // 1. 检查显式环境配置（向后兼容）
    const explicitName = process.env.QDRANT_COLLECTION;
    if (explicitName && explicitName !== 'code-snippets') {
      return explicitName;
    }

    // 2. 使用配置服务的默认集合（如果不是默认值）
    const config = this.getConfig();
    if (config.qdrant.defaultCollection !== 'default') {
      return config.qdrant.defaultCollection;
    }

    // 3. 使用配置服务中的命名模式
    const namingPattern = config.qdrant.namingPattern;
    if (namingPattern && namingPattern !== '{projectId}') {
      return namingPattern.replace('{projectId}', projectId);
    }

    // 4. 使用项目隔离的动态命名
    return `project-${projectId}`;
  }

  /**
   * 获取Nebula空间名称，实现配置优先级逻辑
   * 1. 检查显式环境配置（保持向后兼容）
   * 2. 使用配置服务的默认空间
   * 3. 项目隔离的动态命名
   * 4. 默认回退
   */
  getNebulaSpaceName(projectId: string): string {
    // 1. 检查显式环境配置（向后兼容）
    const explicitName = process.env.NEBULA_SPACE;
    if (explicitName && explicitName !== 'code_graphs') {
      return explicitName;
    }

    // 2. 使用配置服务的默认空间（如果不是默认值）
    const config = this.getConfig();
    if (config.nebula.defaultSpace !== 'default') {
      return config.nebula.defaultSpace;
    }

    // 3. 使用配置服务中的命名模式
    const namingPattern = config.nebula.namingPattern;
    if (namingPattern && namingPattern !== '{projectId}') {
      return namingPattern.replace('{projectId}', projectId);
    }

    // 4. 使用项目隔离的动态命名
    return `project-${projectId}`;
  }

  /**
   * 验证命名约定是否符合数据库约束
   * @param name 集合名或空间名
   * @returns 是否符合约束
   */
  validateNamingConvention(name: string): boolean {
    return ProjectIdManager.validateNamingConvention(name);
  }

}