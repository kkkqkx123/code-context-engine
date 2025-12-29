import { injectable, inject } from 'inversify';
import { EventEmitter } from 'events';
import { LoggerService } from '../../utils/LoggerService';
import { ErrorHandlerService } from '../../utils/ErrorHandlerService';
import { BatchProcessingService } from '../../infrastructure/batching/BatchProcessingService';
import { FileChangeEvent } from '../filesystem/ChangeDetectionService';
import { TYPES } from '../../types';

export interface BatchScheduler {
  scheduleBatch(projectId: string, changes: FileChangeEvent[]): Promise<void>;
  getPendingCount(projectId: string): number;
  clearPending(projectId: string): void;
}

export interface ChangeGroups {
  fileChanges: FileChangeEvent[];
  indexChanges: FileChangeEvent[];
  vectorChanges: FileChangeEvent[];
  graphChanges: FileChangeEvent[];
}

export interface BatchConfig {
  maxBatchSize: number;
  mediumBatchSize: number;
  minDelay: number;
  mediumDelay: number;
  maxDelay: number;
  maxConcurrency: number;
}

/**
 * 变更协调器 - 集中管理文件变更的批量处理和调度
 */
@injectable()
export class ChangeCoordinator {
  private logger: LoggerService;
  private errorHandler: ErrorHandlerService;
  private batchProcessor: BatchProcessingService;
  private changeAccumulator: Map<string, FileChangeEvent[]> = new Map();
  private batchSchedulers: Map<string, BatchScheduler> = new Map();
  private batchConfig: Map<string, BatchConfig> = new Map();
  private isProcessing: Map<string, boolean> = new Map();
  private eventEmitter: EventEmitter = new EventEmitter();

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.BatchProcessingService) batchProcessor: BatchProcessingService
  ) {
    this.logger = logger;
    this.errorHandler = errorHandler;
    this.batchProcessor = batchProcessor;
  }

  /**
   * 协调变更处理
   */
  async coordinateChanges(projectId: string, changes: FileChangeEvent[]): Promise<void> {
    try {
      this.logger.debug(`Coordinating changes for project ${projectId}`, {
        changeCount: changes.length
      });

      // 累积变更
      this.accumulateChanges(projectId, changes);

      // 获取或创建批处理调度器
      const scheduler = this.getOrCreateScheduler(projectId);

      // 调度批量处理
      await scheduler.scheduleBatch(projectId, this.changeAccumulator.get(projectId) || []);

    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error(String(error)),
        {
          component: 'ChangeCoordinator',
          operation: 'coordinateChanges',
          metadata: { projectId, changeCount: changes.length }
        }
      );
      throw error;
    }
  }

  /**
   * 累积变更
   */
  private accumulateChanges(projectId: string, changes: FileChangeEvent[]): void {
    if (!this.changeAccumulator.has(projectId)) {
      this.changeAccumulator.set(projectId, []);
    }
    this.changeAccumulator.get(projectId)!.push(...changes);
  }

  /**
   * 获取或创建批处理调度器
   */
  private getOrCreateScheduler(projectId: string): BatchScheduler {
    if (!this.batchSchedulers.has(projectId)) {
      this.batchSchedulers.set(projectId, new DefaultBatchScheduler(
        this.logger,
        this.batchProcessor,
        this.getProjectBatchConfig(projectId)
      ));
    }
    return this.batchSchedulers.get(projectId)!;
  }

  /**
   * 获取项目批处理配置
   */
  private getProjectBatchConfig(projectId: string): BatchConfig {
    if (!this.batchConfig.has(projectId)) {
      this.batchConfig.set(projectId, {
        maxBatchSize: 50,
        mediumBatchSize: 20,
        minDelay: 100,
        mediumDelay: 1000,
        maxDelay: 5000,
        maxConcurrency: 3
      });
    }
    return this.batchConfig.get(projectId)!;
  }

  /**
    * 智能变更分组
    */
   private groupChangesByTarget(changes: FileChangeEvent[]): ChangeGroups {
     const initialGroups: ChangeGroups = {
       fileChanges: [],
       indexChanges: [],
       vectorChanges: [],
       graphChanges: []
     };

     return changes.reduce((groups: ChangeGroups, change: FileChangeEvent) => {
       // 基于文件扩展名和变更类型进行分组
       const extension = change.path.split('.').pop()?.toLowerCase();
       
       // 向量索引相关文件
       if (this.isVectorRelevantFile(extension)) {
         groups.vectorChanges.push(change);
       }
       
       // 图索引相关文件
       if (this.isGraphRelevantFile(extension)) {
         groups.graphChanges.push(change);
       }
       
       // 索引更新相关
       if (this.requiresIndexing(change)) {
         groups.indexChanges.push(change);
       } else {
         groups.fileChanges.push(change);
       }
       
       return groups;
     }, initialGroups);
   }

  /**
   * 判断文件是否与向量索引相关
   */
  private isVectorRelevantFile(extension?: string): boolean {
    const vectorExtensions = ['ts', 'js', 'tsx', 'jsx', 'py', 'java', 'cpp', 'c', 'h', 'hpp', 'md', 'txt'];
    return vectorExtensions.includes(extension || '');
  }

  /**
   * 判断文件是否与图索引相关
   */
  private isGraphRelevantFile(extension?: string): boolean {
    const graphExtensions = ['ts', 'js', 'tsx', 'jsx', 'py', 'java', 'cpp', 'c', 'h', 'hpp'];
    return graphExtensions.includes(extension || '');
  }

  /**
   * 判断变更是否需要索引更新
   */
  private requiresIndexing(change: FileChangeEvent): boolean {
    const indexedExtensions = ['ts', 'js', 'tsx', 'jsx', 'py', 'java', 'cpp', 'c', 'h', 'hpp'];
    const extension = change.path.split('.').pop()?.toLowerCase();
    return indexedExtensions.includes(extension || '');
  }

  /**
   * 优先级调度
   */
  private async scheduleByPriority(groups: ChangeGroups): Promise<void> {
    // 按优先级处理：高优先级（删除）-> 中优先级（修改）-> 低优先级（创建）
    const priorityGroups = [
      { changes: groups.indexChanges.filter(c => c.type === 'deleted'), priority: 'high' },
      { changes: groups.indexChanges.filter(c => c.type === 'modified'), priority: 'medium' },
      { changes: groups.indexChanges.filter(c => c.type === 'created'), priority: 'low' }
    ];

    for (const group of priorityGroups) {
      if (group.changes.length > 0) {
        await this.processChangeGroup(group.changes, group.priority);
      }
    }
  }

  /**
   * 处理变更组
   */
  private async processChangeGroup(changes: FileChangeEvent[], priority: string): Promise<void> {
    // 这里可以添加具体的处理逻辑
    this.logger.debug(`Processing change group with priority ${priority}`, {
      changeCount: changes.length
    });

    // 发出事件供其他服务处理
    this.eventEmitter.emit('processChangeGroup', { changes, priority });
  }

  /**
   * 清理项目状态
   */
  clearProjectState(projectId: string): void {
    this.changeAccumulator.delete(projectId);
    this.batchSchedulers.delete(projectId);
    this.batchConfig.delete(projectId);
    this.isProcessing.delete(projectId);
    
    this.logger.debug(`Cleared state for project ${projectId}`);
  }

  /**
   * 获取待处理变更数量
   */
  getPendingChangeCount(projectId: string): number {
    return this.changeAccumulator.get(projectId)?.length || 0;
  }

  /**
   * 获取项目统计信息
   */
  getProjectStats(projectId: string): {
    pendingChanges: number;
    isProcessing: boolean;
    hasScheduler: boolean;
  } {
    return {
      pendingChanges: this.getPendingChangeCount(projectId),
      isProcessing: this.isProcessing.get(projectId) || false,
      hasScheduler: this.batchSchedulers.has(projectId)
    };
  }

  /**
   * 获取所有项目统计信息
   */
  getAllProjectStats(): Map<string, {
    pendingChanges: number;
    isProcessing: boolean;
    hasScheduler: boolean;
  }> {
    const stats = new Map();
    
    for (const projectId of this.changeAccumulator.keys()) {
      stats.set(projectId, this.getProjectStats(projectId));
    }
    
    return stats;
  }
}

/**
 * 默认批处理调度器实现
 */
class DefaultBatchScheduler implements BatchScheduler {
  private logger: LoggerService;
  private batchProcessor: BatchProcessingService;
  private config: BatchConfig;
  private pendingBatches: Map<string, NodeJS.Timeout> = new Map();

  constructor(
    logger: LoggerService,
    batchProcessor: BatchProcessingService,
    config: BatchConfig
  ) {
    this.logger = logger;
    this.batchProcessor = batchProcessor;
    this.config = config;
  }

  async scheduleBatch(projectId: string, changes: FileChangeEvent[]): Promise<void> {
    // 清除现有的批处理定时器
    if (this.pendingBatches.has(projectId)) {
      clearTimeout(this.pendingBatches.get(projectId)!);
    }

    const delay = this.calculateDelay(changes.length);
    
    const timer = setTimeout(async () => {
      await this.processBatch(projectId, changes);
    }, delay);

    this.pendingBatches.set(projectId, timer);
  }

  getPendingCount(projectId: string): number {
    return this.pendingBatches.has(projectId) ? 1 : 0;
  }

  clearPending(projectId: string): void {
    if (this.pendingBatches.has(projectId)) {
      clearTimeout(this.pendingBatches.get(projectId)!);
      this.pendingBatches.delete(projectId);
    }
  }

  private calculateDelay(changeCount: number): number {
    if (changeCount >= this.config.maxBatchSize) {
      return this.config.minDelay;
    } else if (changeCount >= this.config.mediumBatchSize) {
      return this.config.mediumDelay;
    } else {
      return this.config.maxDelay;
    }
  }

  private async processBatch(projectId: string, changes: FileChangeEvent[]): Promise<void> {
    try {
      this.logger.debug(`Processing batch for project ${projectId}`, {
        changeCount: changes.length
      });

      const result = await this.batchProcessor.executeHotReloadBatch(
        projectId,
        changes,
        {
          maxConcurrency: this.config.maxConcurrency,
          batchSize: Math.min(20, changes.length)
        }
      );

      this.logger.debug(`Batch processing completed for project ${projectId}`, {
        result
      });

      this.clearPending(projectId);
    } catch (error) {
      this.logger.error(`Batch processing failed for project ${projectId}:`, error);
      this.clearPending(projectId);
    }
  }
}