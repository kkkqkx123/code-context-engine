/**
 * SQLite数据迁移类型定义
 */

export interface MigrationResult {
  success: boolean;
  migratedCount: number;
  errorCount: number;
  errors: string[];
  duration: number;
}

export interface MigrationSummary {
  projectMappings: MigrationResult;
  projectStates: MigrationResult;
  totalMigrated: number;
  totalErrors: number;
  overallSuccess: boolean;
}

export interface ValidationResult {
  isValid: boolean;
  issues: string[];
  dataConsistency: {
    projects: number;
    fileStates: number;
    projectStatus: number;
    changeHistory: number;
  };
}

export interface RollbackResult {
  success: boolean;
  restoredFiles: string[];
  errors: string[];
}

export interface MigrationStatus {
  enabled: boolean;
  lastMigration: Date | null;
  dataSource: 'sqlite' | 'json';
}

export interface MigrationConfig {
  backupEnabled: boolean;
  backupPath: string;
  validateBeforeMigration: boolean;
  validateAfterMigration: boolean;
  rollbackOnError: boolean;
  maxRetries: number;
  retryDelay: number;
}

export interface MigrationOptions {
  config?: Partial<MigrationConfig>;
  onProgress?: (progress: MigrationProgress) => void;
  onError?: (error: MigrationError) => void;
}

export interface MigrationProgress {
  stage: string;
  current: number;
  total: number;
  percentage: number;
  message: string;
}

export interface MigrationError {
  stage: string;
  error: string;
  context?: any;
  recoverable: boolean;
}

export interface DataConsistencyCheck {
  tableName: string;
  expectedCount: number;
  actualCount: number;
  isConsistent: boolean;
  issues: string[];
}

export interface MigrationReport {
  summary: MigrationSummary;
  validation: ValidationResult;
  consistencyChecks: DataConsistencyCheck[];
  performanceMetrics: {
    totalDuration: number;
    backupDuration: number;
    migrationDuration: number;
    validationDuration: number;
  };
  recommendations: string[];
}