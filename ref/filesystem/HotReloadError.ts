import { ErrorReport } from '../../utils/ErrorHandlerService';

export enum HotReloadErrorCode {
  FILE_WATCH_FAILED = 'FILE_WATCH_FAILED',
  CHANGE_DETECTION_FAILED = 'CHANGE_DETECTION_FAILED',
  INDEX_UPDATE_FAILED = 'INDEX_UPDATE_FAILED',
  PERMISSION_DENIED = 'PERMISSION_DENIED',
  FILE_TOO_LARGE = 'FILE_TOO_LARGE',
  HOT_RELOAD_DISABLED = 'HOT_RELOAD_DISABLED',
  PROJECT_NOT_FOUND = 'PROJECT_NOT_FOUND',
  INVALID_CONFIG = 'INVALID_CONFIG'
}

export class HotReloadError extends Error {
  constructor(
    public code: HotReloadErrorCode,
    message: string,
    public context?: Record<string, any>
  ) {
    super(message);
    this.name = 'HotReloadError';
  }

  toJSON() {
    return {
      code: this.code,
      message: this.message,
      name: this.name,
      context: this.context,
      stack: this.stack
    };
  }
}

// 扩展ErrorReport接口以包含热更新特定字段
declare module '../../utils/ErrorHandlerService' {
  interface ErrorReport {
    errorCode?: string; // 新增错误代码字段
    retryCount?: number; // 重试次数
    autoRecovered?: boolean; // 是否自动恢复
  }
}