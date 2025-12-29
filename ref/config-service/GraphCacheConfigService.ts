/**
 * 图缓存配置服务
 * 提供图缓存模块的配置管理
 */

import { injectable } from 'inversify';
import * as Joi from 'joi';
import { BaseConfigService } from './BaseConfigService';
import { EnvironmentUtils } from '../utils/EnvironmentUtils';
import { ValidationUtils } from '../utils/ValidationUtils';
import { GraphCacheConfig } from '../ConfigTypes';

@injectable()
export class GraphCacheConfigService extends BaseConfigService<GraphCacheConfig> {
  loadConfig(): GraphCacheConfig {
    const rawConfig = {
      maxSize: EnvironmentUtils.parseNumber('GRAPH_CACHE_MAX_SIZE', 10000),
      defaultTTL: EnvironmentUtils.parseNumber('GRAPH_CACHE_DEFAULT_TTL', 300), // 5分钟
      maxMemory: EnvironmentUtils.parseNumber('GRAPH_CACHE_MAX_MEMORY', 100 * 1024 * 1024), // 100MB
      enableCompression: EnvironmentUtils.parseBoolean('GRAPH_CACHE_ENABLE_COMPRESSION', true),
      compressionThreshold: EnvironmentUtils.parseNumber('GRAPH_CACHE_COMPRESSION_THRESHOLD', 1024), // 1KB
      enableStats: EnvironmentUtils.parseBoolean('GRAPH_CACHE_ENABLE_STATS', true),
      compressionLevel: EnvironmentUtils.parseNumber('GRAPH_CACHE_COMPRESSION_LEVEL', 6),
    };

    return this.validateConfig(rawConfig);
  }

  validateConfig(config: any): GraphCacheConfig {
    const schema = Joi.object({
      maxSize: Joi.number().integer().min(100).max(1000000).required(),
      defaultTTL: Joi.number().integer().min(60).max(86400).required(), // 1分钟到24小时
      maxMemory: Joi.number().integer().min(10 * 1024 * 1024).max(2 * 1024 * 1024 * 1024).required(), // 10MB到2GB
      enableCompression: Joi.boolean().required(),
      compressionThreshold: Joi.number().integer().min(128).max(10240).required(), // 128字节到10KB
      enableStats: Joi.boolean().required(),
      compressionLevel: Joi.number().integer().min(1).max(9).required(),
    });

    return ValidationUtils.validateConfig(config, schema);
  }

  getDefaultConfig(): GraphCacheConfig {
    return {
      maxSize: 10000,
      defaultTTL: 300, // 5分钟
      maxMemory: 100 * 1024 * 1024, // 100MB
      enableCompression: true,
      compressionThreshold: 1024, // 1KB
      enableStats: true,
      compressionLevel: 6,
    };
  }
}