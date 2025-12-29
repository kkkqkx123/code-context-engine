import { injectable } from 'inversify';
import * as Joi from 'joi';
import { BaseConfigService } from './BaseConfigService';
import { EnvironmentUtils } from '../utils/EnvironmentUtils';
import { ValidationUtils } from '../utils/ValidationUtils';

export interface EnvironmentConfig {
  nodeEnv: string;
  port: number;
  logLevel: string;
  debug: boolean;
  
  // 合并日志配置
  logging?: {
    level: string;
    format: string;
  };
  
  // 合并监控配置
  monitoring?: {
    enabled: boolean;
    port: number;
    prometheusTargetDir: string;
  };
}

@injectable()
export class EnvironmentConfigService extends BaseConfigService<EnvironmentConfig> {
  loadConfig(): EnvironmentConfig {
    const rawConfig = {
      nodeEnv: EnvironmentUtils.parseString('NODE_ENV', 'development'),
      port: EnvironmentUtils.parsePort('PORT', 3000),
      logLevel: EnvironmentUtils.parseString('LOG_LEVEL', 'info'),
      debug: EnvironmentUtils.parseBoolean('DEBUG', false),
      
      // 合并日志配置
      logging: {
        level: EnvironmentUtils.parseString('LOG_LEVEL', 'info'),
        format: EnvironmentUtils.parseString('LOG_FORMAT', 'json'),
      },
      
      // 合并监控配置
      monitoring: {
        enabled: process.env.ENABLE_METRICS === 'true',
        port: parseInt(process.env.METRICS_PORT || '9090'),
        prometheusTargetDir: process.env.PROMETHEUS_TARGET_DIR || './etc/prometheus',
      },
    };

    return this.validateConfig(rawConfig);
  }

  validateConfig(config: any): EnvironmentConfig {
    const schema = Joi.object({
      nodeEnv: ValidationUtils.enumSchema(['development', 'production', 'test'], 'development'),
      port: ValidationUtils.portSchema(3000),
      logLevel: ValidationUtils.enumSchema(['error', 'warn', 'info', 'debug'], 'info'),
      debug: ValidationUtils.booleanSchema(false),
      
      // 日志配置验证
      logging: ValidationUtils.optionalObjectSchema({
        level: ValidationUtils.enumSchema(['error', 'warn', 'info', 'debug'], 'info'),
        format: ValidationUtils.enumSchema(['json', 'text'], 'json'),
      }),
      
      // 监控配置验证
      monitoring: ValidationUtils.optionalObjectSchema({
        enabled: Joi.boolean().default(true),
        port: Joi.number().port().default(9090),
        prometheusTargetDir: Joi.string().default('./etc/prometheus'),
      }),
    });

    return ValidationUtils.validateConfig(config, schema);
  }

  getDefaultConfig(): EnvironmentConfig {
    return {
      nodeEnv: 'development',
      port: 3000,
      logLevel: 'info',
      debug: false,
      
      logging: {
        level: 'info',
        format: 'json',
      },
      
      monitoring: {
        enabled: true,
        port: 9090,
        prometheusTargetDir: './etc/prometheus',
      },
    };
  }
}