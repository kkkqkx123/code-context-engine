import * as fs from 'fs/promises';
import * as path from 'path';
import { ProcessEventManager } from './ProcessEventManager';

export class Logger {
  private static instance: Logger;
  private logFilePath: string;
  private isNormalExit: boolean = false;
  private hasLogged: boolean = false; // 标记是否已写入日志
  private logLevel: string;
  private eventManager: ProcessEventManager;
  private serviceName: string;

  private constructor(serviceName: string, logLevel?: string) {
    this.serviceName = serviceName;
    // 设置日志级别，默认为 'info'
    this.logLevel = logLevel?.toUpperCase() || 'INFO';

    // 使用启动时的时间戳创建日志文件，后续运行期间一直使用该文件
    const timestamp = Logger.getStartTimeString().replace(/[:]/g, '-').replace(/\+08-00$/, ''); // 替换冒号为连字符并移除时区后缀
    const logFileName = `${serviceName}-${timestamp}.log`;
    this.logFilePath = path.join(process.cwd(), 'logs', logFileName);

    // 获取事件管理器实例
    this.eventManager = ProcessEventManager.getInstance();

    // 检查是否在测试环境中
    if (process.env.NODE_ENV === 'test') {
      // 测试环境中不创建日志文件，只输出到控制台
      this.isNormalExit = true; // 标记为正常退出，避免不必要的清理
    } else {
      // 非测试环境中初始化日志目录
      this.ensureLogDirectory().catch(error => {
        console.error('Failed to create log directory:', error);
      });

      // 设置进程退出处理 - 只在第一个实例中设置
      if (!Logger.instance) {
        this.setupExitHandlers();
      }
    }
  }

  /**
   * 获取 Logger 单例实例
   */
  public static getInstance(serviceName: string = 'code-search-helper', logLevel?: string): Logger {
    if (!Logger.instance) {
      Logger.instance = new Logger(serviceName, logLevel);
    }
    return Logger.instance;
  }

  // 静态属性用于存储启动时间，确保整个应用运行期间使用相同的时间戳
  private static startTime: Date = new Date();

  /**
   * 获取启动时的时间字符串（在整个应用运行期间保持不变）
   */
  private static getStartTimeString(): string {
    const now = this.startTime;
    const year = now.getFullYear();
    const month = String(now.getMonth() + 1).padStart(2, '0');
    const day = String(now.getDate()).padStart(2, '0');
    const hours = String(now.getHours()).padStart(2, '0');
    const minutes = String(now.getMinutes()).padStart(2, '0');
    const seconds = String(now.getSeconds()).padStart(2, '0');

    // 返回格式为 YYYY-MM-DDTHH:MM:SS+08:00 的时间字符串（不含毫秒）
    return `${year}-${month}-${day}T${hours}:${minutes}:${seconds}+08:00`;
  }

  /**
   * 获取当前时间的字符串（用于日志条目的时间戳）
   */
  private getChinaTimeString(): string {
    const now = new Date();
    // 使用标准的toLocaleString方法获取+8时区时间
    const year = now.getFullYear();
    const month = String(now.getMonth() + 1).padStart(2, '0');
    const day = String(now.getDate()).padStart(2, '0');
    const hours = String(now.getHours()).padStart(2, '0');
    const minutes = String(now.getMinutes()).padStart(2, '0');
    const seconds = String(now.getSeconds()).padStart(2, '0');
    const milliseconds = String(now.getMilliseconds()).padStart(3, '0');

    // 返回格式为 YYYY-MM-DDTHH:MM:SS.sss+08:00 的时间字符串
    return `${year}-${month}-${day}T${hours}:${minutes}:${seconds}.${milliseconds}+08:00`;
  }

  private async ensureLogDirectory(): Promise<void> {
    try {
      const logDir = path.join(process.cwd(), 'logs');
      await fs.mkdir(logDir, { recursive: true });
    } catch (error) {
      console.error('Failed to create log directory:', error);
    }
  }

  private setupExitHandlers(): void {
    // 正常退出处理
    const exitHandler = (code: number) => {
      // 尽可能释放资源，但不执行异步操作
      if (this.isNormalExit && this.hasLogged) {
        // 在同步上下文中，我们不能执行异步操作，所以只记录状态
      }
    };

    // 异常退出处理
    const sigintHandler = async () => {
      this.isNormalExit = true;
      await this.cleanup(true);
      process.exit(0);
    };

    const sigtermHandler = async () => {
      this.isNormalExit = true;
      await this.cleanup(true);
      process.exit(0);
    };

    const uncaughtExceptionHandler = async (error: Error) => {
      await this.error('Uncaught Exception:', error);
      this.isNormalExit = false;
      await this.cleanup(false);
      process.exit(1);
    };

    const unhandledRejectionHandler = async (reason: any, promise: Promise<any>) => {
      await this.error('Unhandled Rejection at:', promise, 'reason:', reason);
      this.isNormalExit = false;
      await this.cleanup(false);
      process.exit(1);
    };

    // 使用事件管理器注册监听器，避免重复注册
    this.eventManager.addListener('exit', exitHandler);
    this.eventManager.addListener('SIGINT', sigintHandler);
    this.eventManager.addListener('SIGTERM', sigtermHandler);
    this.eventManager.addListener('uncaughtException', uncaughtExceptionHandler);
    this.eventManager.addListener('unhandledRejection', unhandledRejectionHandler);
  }

  private async cleanup(isNormalExit: boolean): Promise<void> {
    this.isNormalExit = isNormalExit;

    // 如果是正常退出且已写入日志，删除日志文件
    if (isNormalExit && this.hasLogged) {
      try {
        await fs.unlink(this.logFilePath);
      } catch (error) {
        // 忽略删除文件时的错误
      }
    }
  }

  private shouldLog(level: string): boolean {
    const levels: { [key: string]: number } = {
      'DEBUG': 0,
      'INFO': 1,
      'WARN': 2,
      'ERROR': 3
    };

    const currentLevel = levels[this.logLevel] ?? levels['INFO'];
    const messageLevel = levels[level] ?? levels['INFO'];

    return messageLevel >= currentLevel;
  }
  private async writeLog(level: string, ...args: any[]): Promise<void> {
    // 检查是否应该记录此级别的日志
    if (!this.shouldLog(level)) {
      return;
    }

    const timestamp = this.getChinaTimeString();
    const message = args.map(arg =>
      typeof arg === 'object' ? JSON.stringify(arg, null, 2) : String(arg)
    ).join(' ');

    // 尝试写入文件
    const logLine = `[${timestamp}] [${level}] ${message}\n`;

    // 如果已经标记为正常退出，则不再写入文件，只输出到控制台
    if (this.isNormalExit) {
      // 在正常退出过程中，只输出到控制台，不写入文件
      console.log(`[${timestamp}] [${level}] ${message}`);
      return;
    }

    // 使用 fs.appendFile 替代 FileHandle，避免文件描述符泄漏
    try {
      await fs.appendFile(this.logFilePath, logLine);
      this.hasLogged = true; // 标记已经写入日志
    } catch (error) {
      // 文件写入失败时，只输出到控制台
      console.warn('Failed to write to log file:', error);
    }

    // 同时输出到控制台
    console.log(`[${timestamp}] [${level}] ${message}`);
  }

  async info(...args: any[]): Promise<void> {
    await this.writeLog('INFO', ...args);
  }

  async error(...args: any[]): Promise<void> {
    await this.writeLog('ERROR', ...args);
  }

  async warn(...args: any[]): Promise<void> {
    await this.writeLog('WARN', ...args);
  }

  async debug(...args: any[]): Promise<void> {
    await this.writeLog('DEBUG', ...args);
  }

  getLogFilePath(): string {
    return this.logFilePath;
  }

  /**
   * 更新日志级别
   */
  updateLogLevel(level: string): void {
    this.logLevel = level.toUpperCase();
  }

  async markAsNormalExit(): Promise<void> {
    this.isNormalExit = true;
    await this.cleanup(true);
  }
}