import { injectable } from 'inversify';
import * as Joi from 'joi';
import { BaseConfigService } from './BaseConfigService';

export interface IndexingConfig {
  batchSize: number;
  maxConcurrency: number;
  enableParallelIndexing: boolean;
  timeoutMs: number;
  retryAttempts: number;
  enableIncrementalIndexing: boolean;
  indexStrategy: 'full' | 'incremental' | 'smart';
  enableAutoOptimization: boolean;
  optimizationThreshold: number;
}

@injectable()
export class IndexingConfigService extends BaseConfigService<IndexingConfig> {
  loadConfig(): IndexingConfig {
    const rawConfig = {
      batchSize: parseInt(process.env.INDEXING_BATCH_SIZE || '50'),
      maxConcurrency: parseInt(process.env.INDEXING_MAX_CONCURRENCY || '3'),
      enableParallelIndexing: process.env.INDEXING_ENABLE_PARALLEL !== 'false',
      timeoutMs: parseInt(process.env.INDEXING_TIMEOUT_MS || '300000'),
      retryAttempts: parseInt(process.env.INDEXING_RETRY_ATTEMPTS || '3'),
      enableIncrementalIndexing: process.env.INDEXING_ENABLE_INCREMENTAL !== 'false',
      indexStrategy: (process.env.INDEXING_STRATEGY as 'full' | 'incremental' | 'smart') || 'smart',
      enableAutoOptimization: process.env.INDEXING_ENABLE_AUTO_OPTIMIZATION !== 'false',
      optimizationThreshold: parseInt(process.env.INDEXING_OPTIMIZATION_THRESHOLD || '1000'),
    };

    return this.validateConfig(rawConfig);
  }

  validateConfig(config: any): IndexingConfig {
    const schema = Joi.object({
      batchSize: Joi.number().positive().default(50),
      maxConcurrency: Joi.number().positive().default(3),
      enableParallelIndexing: Joi.boolean().default(true),
      timeoutMs: Joi.number().positive().default(300000),
      retryAttempts: Joi.number().positive().integer().default(3),
      enableIncrementalIndexing: Joi.boolean().default(true),
      indexStrategy: Joi.string().valid('full', 'incremental', 'smart').default('smart'),
      enableAutoOptimization: Joi.boolean().default(true),
      optimizationThreshold: Joi.number().positive().default(1000),
    });

    const { error, value } = schema.validate(config);
    if (error) {
      throw new Error(`Indexing config validation error: ${error.message}`);
    }

    return value;
  }

  getDefaultConfig(): IndexingConfig {
    return {
      batchSize: 50,
      maxConcurrency: 3,
      enableParallelIndexing: true,
      timeoutMs: 300000,
      retryAttempts: 3,
      enableIncrementalIndexing: true,
      indexStrategy: 'smart',
      enableAutoOptimization: true,
      optimizationThreshold: 1000,
    };
  }
}