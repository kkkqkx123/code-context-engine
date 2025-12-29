import { injectable } from 'inversify';
import { ValidationUtils } from '../utils/ValidationUtils';

export interface ConfigServiceInterface<T> {
  loadConfig(): T;
  validateConfig(config: any): T;
  getDefaultConfig(): T;
}

@injectable()
export abstract class BaseConfigService<T> implements ConfigServiceInterface<T> {
  private _config: T | null = null;
  private _isInitialized = false;

  constructor() {
    // 不在构造函数中初始化配置，而是使用懒加载
  }

  abstract loadConfig(): T;
  abstract validateConfig(config: any): T;
  abstract getDefaultConfig(): T;

  getConfig(): T {
    if (!this._isInitialized) {
      this._config = this.loadConfig();
      this._isInitialized = true;
    }
    return this._config!;
  }

  updateConfig(newConfig: Partial<T>): void {
    if (!this._isInitialized) {
      this._config = this.loadConfig();
      this._isInitialized = true;
    }
    this._config = { ...this._config!, ...newConfig };
  }

  /**
   * 验证配置对象的通用方法
   * @param config 配置对象
   * @param schema Joi验证模式
   * @returns 验证后的配置对象
   */
  protected validateWithSchema(config: any, schema: any): T {
    return ValidationUtils.validateConfig(config, schema);
  }
}