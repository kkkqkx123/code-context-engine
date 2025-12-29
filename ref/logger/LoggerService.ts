import { injectable } from 'inversify';
import { Logger } from './logger';

/**
 * 日志服务类
 * 提供结构化的日志记录功能
 */
@injectable()
export class LoggerService {
  private logger: Logger;

  constructor() {
    const serviceName = 'code-search-helper';
    // 从环境变量获取日志级别，如果没有设置则使用默认级别
    const logLevel = process.env.LOG_LEVEL || 'info';
    // 使用 Logger 单例
    this.logger = Logger.getInstance(serviceName, logLevel);
  }

  /**
   * 记录信息级别日志
   */
  async info(message: string, meta?: any): Promise<void> {
    await this.logger.info(message, meta || {});
  }

  /**
   * 记录错误级别日志
   */
  async error(message: string, error?: any): Promise<void> {
    await this.logger.error(message, error || {});
 }

  /**
   * 记录警告级别日志
   */
  async warn(message: string, meta?: any): Promise<void> {
    await this.logger.warn(message, meta || {});
  }

  /**
   * 记录调试级别日志
   */
  async debug(message: string, meta?: any): Promise<void> {
    await this.logger.debug(message, meta || {});
 }

  /**
   * 获取日志文件路径
   */
  getLogFilePath(): string {
    return this.logger.getLogFilePath();
  }

  /**
   * 更新日志级别
   */
  updateLogLevel(level: string): void {
    this.logger.updateLogLevel(level);
  }

  /**
   * 标记为正常退出
   */
  async markAsNormalExit(): Promise<void> {
    await this.logger.markAsNormalExit();
  }
}