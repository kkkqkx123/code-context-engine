import { injectable } from 'inversify';
import * as Joi from 'joi';
import { BaseConfigService } from './BaseConfigService';
import { EnvironmentUtils, ValidationUtils } from '../ConfigTypes';

/**
 * Base embedding provider configuration
 * Follows Interface Segregation Principle - focused, specific interface
 */
export interface BaseEmbeddingProviderConfig {
  model: string;
  dimensions: number;
}

/**
 * Provider configurations with specific authentication
 * Each provider only includes what it actually needs
 */
export interface OpenAIConfig extends BaseEmbeddingProviderConfig {
  apiKey?: string;
  baseUrl?: string;
}

export interface OllamaConfig extends BaseEmbeddingProviderConfig {
  baseUrl: string;
}

export interface GeminiConfig extends BaseEmbeddingProviderConfig {
  apiKey?: string;
  baseUrl?: string;
}

export interface MistralConfig extends BaseEmbeddingProviderConfig {
  apiKey?: string;
  baseUrl?: string;
}

export interface SiliconFlowConfig extends BaseEmbeddingProviderConfig {
  apiKey?: string;
  baseUrl?: string;
}

export interface Custom1ProviderConfig extends BaseEmbeddingProviderConfig {
  apiKey?: string;
  baseUrl?: string;
}

export interface Custom2ProviderConfig extends BaseEmbeddingProviderConfig {
  apiKey?: string;
  baseUrl?: string;
}

export interface Custom3ProviderConfig extends BaseEmbeddingProviderConfig {
  apiKey?: string;
  baseUrl?: string;
}

export interface SimilarityProviderConfig extends BaseEmbeddingProviderConfig {
  apiKey?: string;
  baseUrl: string;
}

/**
 * Simplified embedding configuration
 * Uses provider factory pattern instead of bloated interface
 * Follows KISS and YAGNI principles
 */
export interface EmbeddingConfig {
  provider: string;
  weights?: {
    quality?: number;
    performance?: number;
  };
  // Only include provider configuration that's actually used
  providerConfig?: BaseEmbeddingProviderConfig | OpenAIConfig | OllamaConfig | GeminiConfig | MistralConfig | SiliconFlowConfig | Custom1ProviderConfig | Custom2ProviderConfig | Custom3ProviderConfig | SimilarityProviderConfig;
}

/**
 * Provider factory for creating provider-specific configurations
 * Follows Open/Closed Principle - easy to add new providers
 */
export class EmbeddingProviderFactory {
  /**
   * Create provider-specific configuration from environment variables
   */
  static createProviderConfig(provider: string): BaseEmbeddingProviderConfig | OpenAIConfig | OllamaConfig | GeminiConfig | MistralConfig | SiliconFlowConfig | Custom1ProviderConfig | Custom2ProviderConfig | Custom3ProviderConfig | SimilarityProviderConfig {
    switch (provider) {
      case 'openai':
        return {
          apiKey: EnvironmentUtils.parseOptionalString('OPENAI_API_KEY'),
          baseUrl: EnvironmentUtils.parseOptionalString('OPENAI_BASE_URL'),
          model: EnvironmentUtils.parseString('OPENAI_MODEL', 'text-embedding-ada-002'),
          dimensions: EnvironmentUtils.parseNumber('OPENAI_DIMENSIONS', 1536),
        } as OpenAIConfig;

      case 'ollama':
        return {
          baseUrl: EnvironmentUtils.parseString('OLLAMA_BASE_URL', 'http://localhost:11434'),
          model: EnvironmentUtils.parseString('OLLAMA_MODEL', 'nomic-embed-text'),
          dimensions: EnvironmentUtils.parseNumber('OLLAMA_DIMENSIONS', 768),
        } as OllamaConfig;

      case 'gemini':
        return {
          apiKey: EnvironmentUtils.parseOptionalString('GEMINI_API_KEY'),
          baseUrl: EnvironmentUtils.parseOptionalString('GEMINI_BASE_URL'),
          model: EnvironmentUtils.parseString('GEMINI_MODEL', 'embedding-001'),
          dimensions: EnvironmentUtils.parseNumber('GEMINI_DIMENSIONS', 768),
        } as GeminiConfig;

      case 'mistral':
        return {
          apiKey: EnvironmentUtils.parseOptionalString('MISTRAL_API_KEY'),
          baseUrl: EnvironmentUtils.parseOptionalString('MISTRAL_BASE_URL'),
          model: EnvironmentUtils.parseString('MISTRAL_MODEL', 'mistral-embed'),
          dimensions: EnvironmentUtils.parseNumber('MISTRAL_DIMENSIONS', 1024),
        } as MistralConfig;

      case 'siliconflow':
        return {
          apiKey: EnvironmentUtils.parseOptionalString('SILICONFLOW_API_KEY'),
          baseUrl: EnvironmentUtils.parseOptionalString('SILICONFLOW_BASE_URL'),
          model: EnvironmentUtils.parseString('SILICONFLOW_MODEL', 'BAAI/bge-large-en-v1.5'),
          dimensions: EnvironmentUtils.parseNumber('SILICONFLOW_DIMENSIONS', 1024),
        } as SiliconFlowConfig;

      case 'custom1':
      return {
           apiKey: EnvironmentUtils.parseOptionalString('CUSTOM_CUSTOM1_API_KEY'),
           baseUrl: EnvironmentUtils.parseOptionalString('CUSTOM_CUSTOM1_BASE_URL'),
           model: EnvironmentUtils.parseString('CUSTOM_CUSTOM1_MODEL', 'custom1-embed'),
           dimensions: EnvironmentUtils.parseNumber('CUSTOM_CUSTOM1_DIMENSIONS', 768),
         } as Custom1ProviderConfig;

       case 'custom2':
         return {
           apiKey: EnvironmentUtils.parseOptionalString('CUSTOM_CUSTOM2_API_KEY'),
           baseUrl: EnvironmentUtils.parseOptionalString('CUSTOM_CUSTOM2_BASE_URL'),
           model: EnvironmentUtils.parseString('CUSTOM_CUSTOM2_MODEL', 'custom2-embed'),
           dimensions: EnvironmentUtils.parseNumber('CUSTOM_CUSTOM2_DIMENSIONS', 768),
         } as Custom2ProviderConfig;

       case 'custom3':
         return {
           apiKey: EnvironmentUtils.parseOptionalString('CUSTOM_CUSTOM3_API_KEY'),
           baseUrl: EnvironmentUtils.parseOptionalString('CUSTOM_CUSTOM3_BASE_URL'),
           model: EnvironmentUtils.parseString('CUSTOM_CUSTOM3_MODEL', 'custom3-embed'),
           dimensions: EnvironmentUtils.parseNumber('CUSTOM_CUSTOM3_DIMENSIONS', 768),
         } as Custom3ProviderConfig;

       case 'similarity':
         return {
           apiKey: EnvironmentUtils.parseOptionalString('SIMILARITY_API_KEY'),
           baseUrl: EnvironmentUtils.parseString('SIMILARITY_BASE_URL', 'http://localhost:9000'),
           model: EnvironmentUtils.parseString('SIMILARITY_MODEL', 'sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2'),
           dimensions: EnvironmentUtils.parseNumber('SIMILARITY_DIMENSIONS', 384),
         } as SimilarityProviderConfig;

       default:
         throw new Error(`Unsupported embedding provider: ${provider}. Supported providers: openai, ollama, gemini, mistral, siliconflow, custom1, custom2, custom3, similarity`);
    }
  }

  /**
   * Get validation schema for specific provider
   */
  static getProviderValidationSchema(provider: string): Joi.ObjectSchema {
    const baseSchema = Joi.object({
      model: Joi.string().required(),
      dimensions: Joi.number().positive().required(),
    });

    switch (provider) {
      case 'openai':
        return baseSchema.keys({
          apiKey: Joi.string().required(),
          baseUrl: Joi.string().uri().optional(),
        });

      case 'ollama':
        return baseSchema.keys({
          baseUrl: Joi.string().uri().required(),
        });

      case 'gemini':
        return baseSchema.keys({
          apiKey: Joi.string().required(),
          baseUrl: Joi.string().uri().optional(),
        });

      case 'mistral':
        return baseSchema.keys({
          apiKey: Joi.string().required(),
          baseUrl: Joi.string().uri().optional(),
        });

      case 'siliconflow':
        return baseSchema.keys({
          apiKey: Joi.string().required(),
          baseUrl: Joi.string().uri().optional(),
        });

      case 'custom1':
      return baseSchema.keys({
           apiKey: Joi.string().optional(),
           baseUrl: Joi.string().uri().optional(),
         });

       case 'custom2':
         return baseSchema.keys({
           apiKey: Joi.string().optional(),
           baseUrl: Joi.string().uri().optional(),
         });

       case 'custom3':
         return baseSchema.keys({
           apiKey: Joi.string().optional(),
           baseUrl: Joi.string().uri().optional(),
         });

       case 'similarity':
         return baseSchema.keys({
           apiKey: Joi.string().optional(),
           baseUrl: Joi.string().uri().required(),
         });

       default:
         throw new Error(`Unsupported provider for validation: ${provider}`);
    }
  }

  /**
   * Validate provider-specific configuration
   */
  static validateProviderConfig(provider: string, config: any): string[] {
    try {
      const schema = this.getProviderValidationSchema(provider);
      const { error } = schema.validate(config);
      return error ? [error.details.map(d => d.message).join(', ')] : [];
    } catch (err) {
      return [err instanceof Error ? err.message : 'Unknown validation error'];
    }
  }
}

@injectable()
export class EmbeddingConfigService extends BaseConfigService<EmbeddingConfig> {
  loadConfig(): EmbeddingConfig {
    const provider = EnvironmentUtils.parseString('EMBEDDING_PROVIDER', 'openai');

    // 检查提供者是否被禁用
    if (EnvironmentUtils.isEmbeddingProviderDisabled(provider)) {
      throw new Error(`Embedding provider ${provider} is disabled`);
    }

    try {
      const providerConfig = EmbeddingProviderFactory.createProviderConfig(provider);

      const rawConfig = {
        provider,
        providerConfig,
        weights: {
          quality: EnvironmentUtils.parseOptionalNumber('QUALITY_WEIGHT'),
          performance: EnvironmentUtils.parseOptionalNumber('PERFORMANCE_WEIGHT'),
        },
      };

      return this.validateConfig(rawConfig);
    } catch (error) {
      throw new Error(`Failed to load embedding config for provider ${provider}: ${error instanceof Error ? error.message : 'Unknown error'}`);
    }
  }

  validateConfig(config: any): EmbeddingConfig {
    const schema = Joi.object({
      provider: ValidationUtils.enumSchema(['openai', 'ollama', 'gemini', 'mistral', 'siliconflow', 'custom1', 'custom2', 'custom3', 'similarity'], 'openai'),
      providerConfig: Joi.when('provider', {
        is: Joi.exist(),
        then: Joi.custom((value, helpers) => {
          const provider = helpers.state.ancestors[0].provider;
          const schema = EmbeddingProviderFactory.getProviderValidationSchema(provider);
          const { error } = schema.validate(value);
          if (error) {
            throw new Error(`Provider config validation failed: ${error.message}`);
          }
          return value;
        }),
        otherwise: Joi.forbidden()
      }),
      weights: ValidationUtils.optionalObjectSchema({
        quality: ValidationUtils.rangeNumberSchema(0, 1, 0.7),
        performance: ValidationUtils.rangeNumberSchema(0, 1, 0.3),
      }),
    });

    return ValidationUtils.validateConfig(config, schema);
  }

  getDefaultConfig(): EmbeddingConfig {
    const provider = 'openai';
    const providerConfig = EmbeddingProviderFactory.createProviderConfig(provider);

    return {
      provider,
      providerConfig,
      weights: {
        quality: 0.7,
        performance: 0.3,
      },
    };
  }

  /**
   * Get configuration for specific provider
   * Type-safe method to get provider-specific configuration
   */
  getProviderConfig<T extends BaseEmbeddingProviderConfig>(): T {
    const config = this.getConfig();
    return config.providerConfig as T;
  }

  /**
   * Validate current provider configuration
   */
  validateCurrentProvider(): string[] {
    const config = this.getConfig();
    return EmbeddingProviderFactory.validateProviderConfig(config.provider, config.providerConfig);
  }

  /**
   * Get supported providers list
   */
  getSupportedProviders(): string[] {
    return ['openai', 'ollama', 'gemini', 'mistral', 'siliconflow', 'custom1', 'custom2', 'custom3', 'similarity'];
  }
}