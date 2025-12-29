import { injectable } from 'inversify';
import * as Joi from 'joi';
import { BaseConfigService } from './BaseConfigService';
import { IMemoryMonitorConfig } from '../../service/memory/interfaces/IMemoryStatus';

// 使用统一的接口定义
export type MemoryMonitorConfig = IMemoryMonitorConfig;

@injectable()
export class MemoryMonitorConfigService extends BaseConfigService<MemoryMonitorConfig> {
  loadConfig(): MemoryMonitorConfig {
    const rawConfig = {
      enabled: process.env.MEMORY_MONITORING_ENABLED !== 'false',
      warningThreshold: parseFloat(process.env.MEMORY_WARNING_THRESHOLD || '0.90'),
      criticalThreshold: parseFloat(process.env.MEMORY_CRITICAL_THRESHOLD || '0.94'),
      emergencyThreshold: parseFloat(process.env.MEMORY_EMERGENCY_THRESHOLD || '0.98'),
      checkInterval: parseInt(process.env.MEMORY_CHECK_INTERVAL || '30000'),
      cleanupCooldown: parseInt(process.env.MEMORY_CLEANUP_COOLDOWN || '30000'),
      maxHistorySize: parseInt(process.env.MEMORY_HISTORY_SIZE || '100'),
    };

    return this.validateConfig(rawConfig);
  }

  validateConfig(config: any): MemoryMonitorConfig {
    const schema = Joi.object({
      enabled: Joi.boolean().default(true),
      warningThreshold: Joi.number().min(0).max(1).default(0.90),
      criticalThreshold: Joi.number().min(0).max(1).default(0.94),
      emergencyThreshold: Joi.number().min(0).max(1).default(0.98),
      checkInterval: Joi.number().min(1000).default(30000),
      cleanupCooldown: Joi.number().min(1000).default(30000),
      maxHistorySize: Joi.number().min(10).default(100),
    });

    const { error, value } = schema.validate(config);
    if (error) {
      throw new Error(`Memory monitor config validation error: ${error.message}`);
    }

    // 确保阈值逻辑正确：warning < critical < emergency
    if (value.enabled) {
      if (value.warningThreshold >= value.criticalThreshold) {
        throw new Error('Warning threshold must be less than critical threshold');
      }
      if (value.criticalThreshold >= value.emergencyThreshold) {
        throw new Error('Critical threshold must be less than emergency threshold');
      }
    }

    return value as MemoryMonitorConfig;
  }

  getDefaultConfig(): MemoryMonitorConfig {
    return {
      enabled: true,
      warningThreshold: 0.90,
      criticalThreshold: 0.94,
      emergencyThreshold: 0.98,
      checkInterval: 30000,
      cleanupCooldown: 30000,
      maxHistorySize: 100,
    } as MemoryMonitorConfig;
  }
}