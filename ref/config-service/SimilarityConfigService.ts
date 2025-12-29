import { injectable } from 'inversify';
import * as Joi from 'joi';
import { BaseConfigService } from './BaseConfigService';
import { EnvironmentUtils, ValidationUtils, SimilarityConfig } from '../ConfigTypes';

@injectable()
export class SimilarityConfigService extends BaseConfigService<SimilarityConfig> {
  loadConfig(): SimilarityConfig {
    // 从SIMILARITY_PROVIDER环境变量读取提供者配置
    const similarityProvider = EnvironmentUtils.parseString('SIMILARITY_PROVIDER', 'similarity');
    
    // 根据提供者类型加载配置
    const rawConfig = {
      provider: similarityProvider,
      apiKey: EnvironmentUtils.parseString('SIMILARITY_API_KEY', ''),
      baseUrl: EnvironmentUtils.parseString('SIMILARITY_BASE_URL', 'http://localhost:9000'),
      model: EnvironmentUtils.parseString('SIMILARITY_MODEL', 'sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2'),
      dimensions: EnvironmentUtils.parseNumber('SIMILARITY_DIMENSIONS', 384),
    };

    return this.validateConfig(rawConfig);
  }

  validateConfig(config: any): SimilarityConfig {
    const schema = Joi.object({
      provider: Joi.string().default('similarity'),
      apiKey: Joi.string().optional().allow(''),
      baseUrl: Joi.string().uri().default('http://localhost:9000'),
      model: Joi.string().default('sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2'),
      dimensions: Joi.number().integer().positive().default(384),
    });

    return ValidationUtils.validateConfig(config, schema);
  }

  getDefaultConfig(): SimilarityConfig {
    return {
      provider: 'similarity',
      apiKey: '',
      baseUrl: 'http://localhost:9000',
      model: 'sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2',
      dimensions: 384,
    };
  }
}
