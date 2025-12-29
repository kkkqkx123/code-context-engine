import { injectable } from 'inversify';
import * as Joi from 'joi';
import { BaseConfigService } from './BaseConfigService';

export interface FileProcessingConfig {
  maxFileSize: number;
  supportedExtensions: string;
  indexBatchSize: number;
  chunkSize: number;
  overlapSize: number;
}

@injectable()
export class FileProcessingConfigService extends BaseConfigService<FileProcessingConfig> {
  loadConfig(): FileProcessingConfig {
    const rawConfig = {
      maxFileSize: parseInt(process.env.MAX_FILE_SIZE || '512000'),
      supportedExtensions: process.env.SUPPORTED_EXTENSIONS || '.ts,.js,.py,.java,.go,.rs,.cpp,.c,.h',
      indexBatchSize: parseInt(process.env.INDEX_BATCH_SIZE || '100'),
      chunkSize: parseInt(process.env.CHUNK_SIZE || '1000'),
      overlapSize: parseInt(process.env.OVERLAP_SIZE || '200'),
    };

    return this.validateConfig(rawConfig);
  }

  validateConfig(config: any): FileProcessingConfig {
    const schema = Joi.object({
      maxFileSize: Joi.number().positive().default(512000),
      supportedExtensions: Joi.string().default('.ts,.js,.py,.java,.go,.rs,.cpp,.c,.h'),
      indexBatchSize: Joi.number().positive().default(100),
      chunkSize: Joi.number().positive().default(1000),
      overlapSize: Joi.number().positive().default(200),
    });

    const { error, value } = schema.validate(config);
    if (error) {
      throw new Error(`File processing config validation error: ${error.message}`);
    }

    return value;
  }

  getDefaultConfig(): FileProcessingConfig {
    return {
      maxFileSize: 512000,
      supportedExtensions: '.ts,.js,.py,.java,.go,.rs,.cpp,.c,.h',
      indexBatchSize: 100,
      chunkSize: 1000,
      overlapSize: 200,
    };
  }
}