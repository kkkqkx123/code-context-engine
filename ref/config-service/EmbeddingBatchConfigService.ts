import { injectable } from 'inversify';
import * as Joi from 'joi';
import { BaseConfigService } from './BaseConfigService';
import { EnvironmentUtils } from '../utils/EnvironmentUtils';
import { ValidationUtils } from '../utils/ValidationUtils';

/**
 * 嵌入器批处理配置接口
 */
export interface EmbeddingBatchConfig {
  defaultBatchSize: number;
  providerBatchLimits: {
    openai: number;
    siliconflow: number;
    ollama: number;
    gemini: number;
    mistral: number;
    custom1: number;
    custom2: number;
    custom3: number;
    similarity: number;
  };
}

/**
 * 嵌入器批处理配置服务
 * 用于管理不同嵌入器提供商的批处理限制
 */
@injectable()
export class EmbeddingBatchConfigService extends BaseConfigService<EmbeddingBatchConfig> {
  loadConfig(): EmbeddingBatchConfig {
    // 从环境变量获取默认批处理大小
    const defaultBatchSize = EnvironmentUtils.parseNumber('EMBEDDING_BATCH_SIZE', 50);

    // 从环境变量获取各提供商的批处理限制
    const providerBatchLimits = {
      openai: EnvironmentUtils.parseNumber('EMBEDDING_PROVIDER_OPENAI_BATCH_SIZE', 2048),
      siliconflow: EnvironmentUtils.parseNumber('EMBEDDING_PROVIDER_SILICONFLOW_BATCH_SIZE', 64),
      ollama: EnvironmentUtils.parseNumber('EMBEDDING_PROVIDER_OLLAMA_BATCH_SIZE', 128),
      gemini: EnvironmentUtils.parseNumber('EMBEDDING_PROVIDER_GEMINI_BATCH_SIZE', 100),
      mistral: EnvironmentUtils.parseNumber('EMBEDDING_PROVIDER_MISTRAL_BATCH_SIZE', 512),
      custom1: EnvironmentUtils.parseNumber('EMBEDDING_PROVIDER_CUSTOM1_BATCH_SIZE', 100),
      custom2: EnvironmentUtils.parseNumber('EMBEDDING_PROVIDER_CUSTOM2_BATCH_SIZE', 100),
      custom3: EnvironmentUtils.parseNumber('EMBEDDING_PROVIDER_CUSTOM3_BATCH_SIZE', 100),
      similarity: EnvironmentUtils.parseNumber('EMBEDDING_PROVIDER_SIMILARITY_BATCH_SIZE', 64),
    };

    const rawConfig = {
      defaultBatchSize,
      providerBatchLimits,
    };

    return this.validateConfig(rawConfig);
  }

  validateConfig(config: any): EmbeddingBatchConfig {
    const schema = Joi.object({
      defaultBatchSize: ValidationUtils.rangeNumberSchema(1, 10000, 50),
      providerBatchLimits: Joi.object({
        openai: ValidationUtils.rangeNumberSchema(1, 10000, 2048),
        siliconflow: ValidationUtils.rangeNumberSchema(1, 10000, 64),
        ollama: ValidationUtils.rangeNumberSchema(1, 10000, 128),
        gemini: ValidationUtils.rangeNumberSchema(1, 10000, 100),
        mistral: ValidationUtils.rangeNumberSchema(1, 10000, 512),
        custom1: ValidationUtils.rangeNumberSchema(1, 10000, 100),
        custom2: ValidationUtils.rangeNumberSchema(1, 10000, 100),
        custom3: ValidationUtils.rangeNumberSchema(1, 10000, 100),
        similarity: ValidationUtils.rangeNumberSchema(1, 10000, 64),
      }).required(),
    });

    return ValidationUtils.validateConfig(config, schema);
  }

  getDefaultConfig(): EmbeddingBatchConfig {
    return {
      defaultBatchSize: 50,
      providerBatchLimits: {
        openai: 2048,
        siliconflow: 64,
        ollama: 128,
        gemini: 100,
        mistral: 512,
        custom1: 100,
        custom2: 100,
        custom3: 100,
        similarity: 64,
      },
    };
  }
}