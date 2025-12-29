import { injectable } from 'inversify';
import * as Joi from 'joi';
import { BaseConfigService } from './BaseConfigService';

export interface ProjectConfig {
  statePath: string;
  mappingPath?: string;
  allowReindex?: boolean;
}

@injectable()
export class ProjectConfigService extends BaseConfigService<ProjectConfig> {
  loadConfig(): ProjectConfig {
    const rawConfig = {
      statePath: process.env.PROJECT_STATE_PATH || './data/project-states.json',
      mappingPath: process.env.PROJECT_MAPPING_PATH || './data/project-mapping.json',
      allowReindex: process.env.ALLOW_REINDEX !== 'false',
    };

    return this.validateConfig(rawConfig);
  }

  validateConfig(config: any): ProjectConfig {
    const schema = Joi.object({
      statePath: Joi.string().default('./data/project-states.json'),
      mappingPath: Joi.string().default('./data/project-mapping.json'),
      allowReindex: Joi.boolean().default(true),
    });

    const { error, value } = schema.validate(config);
    if (error) {
      throw new Error(`Project config validation error: ${error.message}`);
    }

    return value;
  }

  getDefaultConfig(): ProjectConfig {
    return {
      statePath: './data/project-states.json',
      mappingPath: './data/project-mapping.json',
      allowReindex: true,
    };
  }
}