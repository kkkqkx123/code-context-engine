// 热更新相关的类型定义

export interface HotReloadConfig {
  enabled: boolean;
  debounceInterval: number;
  watchPatterns: string[];
  ignorePatterns: string[];
  maxFileSize: number;
  enableHashComparison: boolean;
 trackFileHistory: boolean;
  historySize: number;
  errorHandling: {
    maxRetries: number;
    alertThreshold: number;
    autoRecovery: boolean;
  };
}

export interface HotReloadStatus {
  enabled: boolean;
  isWatching: boolean;
  watchedPaths: string[];
  lastChange: Date | null;
  changesCount: number;
  errorsCount: number;
  lastError: Date | null;
}

export interface HotReloadMetrics {
  filesProcessed: number;
 changesDetected: number;
  averageProcessingTime: number;
  lastUpdated: Date;
  errorCount: number;
  errorBreakdown: Record<string, number>; // 按错误类型分类
  recoveryStats: {
    autoRecovered: number;
    manualIntervention: number;
    failedRecoveries: number;
  };
}

export interface HotReloadEvent {
  type: 'fileCreated' | 'fileModified' | 'fileDeleted';
  filePath: string;
  timestamp: Date;
  projectId?: string;
  processingTime?: number;
}

export interface HotReloadErrorReport {
  id: string;
  timestamp: Date;
 projectId: string;
  filePath?: string;
  errorCode: string;
  message: string;
  stack?: string;
  context?: Record<string, any>;
  retryCount?: number;
  resolved: boolean;
}