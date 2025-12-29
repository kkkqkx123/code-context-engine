import { injectable, inject } from 'inversify';
import { EventEmitter } from 'events';
import { TYPES } from '../../../types';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { PerformanceMonitor } from '../../../infrastructure/monitoring/PerformanceMonitor';
import { NebulaQueryResult } from '../NebulaTypes';

// 管道阶段
export enum PipelineStage {
  VALIDATION = 'validation',
  OPTIMIZATION = 'optimization',
  EXECUTION = 'execution',
  TRANSFORMATION = 'transformation',
  CACHING = 'caching'
}

// 管道阶段处理器
export interface PipelineStageHandler {
  stage: PipelineStage;
  process(context: QueryPipelineContext): Promise<QueryPipelineContext>;
}

// 查询管道上下文
export interface QueryPipelineContext {
  query: string;
  params?: Record<string, any>;
  options?: any;
  result?: NebulaQueryResult;
  metadata: Record<string, any>;
  errors: Error[];
  startTime: number;
  stageTimings: Record<PipelineStage, number>;
}

// 管道配置
export interface QueryPipelineConfig {
  enabled: boolean;
  parallelStages: boolean;
  timeout: number;
  enableMetrics: boolean;
  maxConcurrentPipelines: number;
}

// 默认管道配置
const DEFAULT_PIPELINE_CONFIG: QueryPipelineConfig = {
  enabled: true,
  parallelStages: false,
  timeout: 30000,
  enableMetrics: true,
  maxConcurrentPipelines: 10
};

/**
 * 查询管道处理器
 * 实现查询处理的管道化，提高查询执行效率
 */
@injectable()
export class QueryPipeline extends EventEmitter {
  private logger: LoggerService;
  private errorHandler: ErrorHandlerService;
  private performanceMonitor: PerformanceMonitor;
  private config: QueryPipelineConfig;
  private stageHandlers: Map<PipelineStage, PipelineStageHandler> = new Map();
  private activePipelines: number = 0;

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.PerformanceMonitor) performanceMonitor: PerformanceMonitor
  ) {
    super();
    this.logger = logger;
    this.errorHandler = errorHandler;
    this.performanceMonitor = performanceMonitor;
    this.config = { ...DEFAULT_PIPELINE_CONFIG };
    
    this.initializeDefaultHandlers();
  }

  /**
   * 更新管道配置
   */
  updateConfig(config: Partial<QueryPipelineConfig>): void {
    this.config = { ...this.config, ...config };
    this.logger.info('Query pipeline config updated', { config: this.config });
  }

  /**
   * 注册管道阶段处理器
   */
  registerStageHandler(handler: PipelineStageHandler): void {
    this.stageHandlers.set(handler.stage, handler);
    this.logger.debug('Registered pipeline stage handler', { stage: handler.stage });
  }

  /**
   * 执行查询管道
   */
  async execute(query: string, params?: Record<string, any>, options?: any): Promise<NebulaQueryResult> {
    if (!this.config.enabled) {
      throw new Error('Query pipeline is disabled');
    }

    if (this.activePipelines >= this.config.maxConcurrentPipelines) {
      throw new Error('Maximum concurrent pipelines reached');
    }

    this.activePipelines++;
    const startTime = Date.now();

    try {
      const context: QueryPipelineContext = {
        query,
        params,
        options,
        metadata: {},
        errors: [],
        startTime,
        stageTimings: {
          [PipelineStage.VALIDATION]: 0,
          [PipelineStage.OPTIMIZATION]: 0,
          [PipelineStage.EXECUTION]: 0,
          [PipelineStage.TRANSFORMATION]: 0,
          [PipelineStage.CACHING]: 0
        }
      };

      this.emit('pipelineStarted', { queryId: this.generateQueryId(query) });

      // 执行管道阶段
      if (this.config.parallelStages) {
        await this.executeParallelStages(context);
      } else {
        await this.executeSequentialStages(context);
      }

      // 检查是否有错误
      if (context.errors.length > 0) {
        throw new Error(`Pipeline execution failed: ${context.errors.map(e => e.message).join(', ')}`);
      }

      if (!context.result) {
        throw new Error('Pipeline execution completed but no result was produced');
      }

      const totalTime = Date.now() - startTime;
      
      if (this.config.enableMetrics) {
        this.recordPipelineMetrics(context, totalTime);
      }

      this.emit('pipelineCompleted', {
        queryId: this.generateQueryId(query),
        totalTime,
        stageTimings: context.stageTimings
      });

      return context.result;
    } catch (error) {
      const totalTime = Date.now() - startTime;
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Pipeline execution failed'),
        { component: 'QueryPipeline', operation: 'execute', query, totalTime }
      );
      
      this.emit('pipelineError', {
        queryId: this.generateQueryId(query),
        error: error instanceof Error ? error.message : String(error),
        totalTime
      });
      
      throw error;
    } finally {
      this.activePipelines--;
    }
  }

  /**
   * 批量执行查询管道
   */
  async executeBatch(queries: Array<{ query: string; params?: Record<string, any>; options?: any }>): Promise<NebulaQueryResult[]> {
    const promises = queries.map(({ query, params, options }) => this.execute(query, params, options));
    return Promise.all(promises);
  }

  /**
   * 初始化默认处理器
   */
  private initializeDefaultHandlers(): void {
    // 验证阶段处理器
    this.registerStageHandler({
      stage: PipelineStage.VALIDATION,
      process: async (context) => {
        const stageStart = Date.now();
        
        try {
          // 基本查询验证
          if (!context.query || context.query.trim().length === 0) {
            throw new Error('Query cannot be empty');
          }

          // 检查查询长度
          if (context.query.length > 10000) {
            context.metadata.warning = 'Query is very long, consider optimization';
          }

          // 参数验证
          if (context.params) {
            for (const [key, value] of Object.entries(context.params)) {
              if (value === null || value === undefined) {
                context.metadata[`null_param_${key}`] = true;
              }
            }
          }

          context.stageTimings[PipelineStage.VALIDATION] = Date.now() - stageStart;
          return context;
        } catch (error) {
          context.errors.push(error instanceof Error ? error : new Error('Validation failed'));
          context.stageTimings[PipelineStage.VALIDATION] = Date.now() - stageStart;
          return context;
        }
      }
    });

    // 优化阶段处理器
    this.registerStageHandler({
      stage: PipelineStage.OPTIMIZATION,
      process: async (context) => {
        const stageStart = Date.now();
        
        try {
          let optimizedQuery = context.query;

          // 简单的查询优化
          optimizedQuery = optimizedQuery.replace(/\s+/g, ' ').trim();
          
          // 移除不必要的分号
          if (optimizedQuery.endsWith(';')) {
            optimizedQuery = optimizedQuery.slice(0, -1);
          }

          // 记录优化信息
          if (optimizedQuery !== context.query) {
            context.metadata.queryOptimized = true;
            context.metadata.originalQueryLength = context.query.length;
            context.metadata.optimizedQueryLength = optimizedQuery.length;
          }

          context.query = optimizedQuery;
          context.stageTimings[PipelineStage.OPTIMIZATION] = Date.now() - stageStart;
          return context;
        } catch (error) {
          context.errors.push(error instanceof Error ? error : new Error('Optimization failed'));
          context.stageTimings[PipelineStage.OPTIMIZATION] = Date.now() - stageStart;
          return context;
        }
      }
    });

    // 转换阶段处理器
    this.registerStageHandler({
      stage: PipelineStage.TRANSFORMATION,
      process: async (context) => {
        const stageStart = Date.now();
        
        try {
          if (context.result) {
            // 结果转换和优化
            context.metadata.resultSize = JSON.stringify(context.result).length;
            
            // 添加执行时间信息
            if (!context.result.metadata) {
              context.result.metadata = {};
            }
            context.result.metadata.pipelineExecutionTime = Date.now() - context.startTime;
            context.result.metadata.stageTimings = context.stageTimings;
          }

          context.stageTimings[PipelineStage.TRANSFORMATION] = Date.now() - stageStart;
          return context;
        } catch (error) {
          context.errors.push(error instanceof Error ? error : new Error('Transformation failed'));
          context.stageTimings[PipelineStage.TRANSFORMATION] = Date.now() - stageStart;
          return context;
        }
      }
    });
  }

  /**
   * 顺序执行管道阶段
   */
  private async executeSequentialStages(context: QueryPipelineContext): Promise<void> {
    const stages = [
      PipelineStage.VALIDATION,
      PipelineStage.OPTIMIZATION,
      PipelineStage.EXECUTION,
      PipelineStage.TRANSFORMATION,
      PipelineStage.CACHING
    ];

    for (const stage of stages) {
      const handler = this.stageHandlers.get(stage);
      if (handler) {
        await handler.process(context);
      }
    }
  }

  /**
   * 并行执行管道阶段（适用于独立阶段）
   */
  private async executeParallelStages(context: QueryPipelineContext): Promise<void> {
    // 依赖阶段必须顺序执行
    const sequentialStages = [PipelineStage.VALIDATION, PipelineStage.OPTIMIZATION, PipelineStage.EXECUTION];
    
    for (const stage of sequentialStages) {
      const handler = this.stageHandlers.get(stage);
      if (handler) {
        await handler.process(context);
      }
    }

    // 独立阶段可以并行执行
    const parallelStages = [PipelineStage.TRANSFORMATION, PipelineStage.CACHING];
    const parallelPromises = parallelStages.map(stage => {
      const handler = this.stageHandlers.get(stage);
      return handler ? handler.process(context) : Promise.resolve(context);
    });

    await Promise.all(parallelPromises);
  }

  /**
   * 记录管道性能指标
   */
  private recordPipelineMetrics(context: QueryPipelineContext, totalTime: number): void {
    const operationId = this.performanceMonitor.startOperation('query_pipeline', {
      queryLength: context.query.length,
      hasParams: !!context.params,
      stageCount: Object.keys(context.stageTimings).length,
      totalStagesTime: Object.values(context.stageTimings).reduce((sum, time) => sum + time, 0)
    });

    this.performanceMonitor.endOperation(operationId, {
      success: context.errors.length === 0,
      duration: totalTime,
      metadata: {
        stageTimings: context.stageTimings,
        errors: context.errors.length,
        metadataKeys: Object.keys(context.metadata)
      }
    });
  }

  /**
   * 生成查询ID
   */
  private generateQueryId(query: string): string {
    return `query_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  /**
   * 获取管道统计信息
   */
  getStats(): any {
    return {
      activePipelines: this.activePipelines,
      maxConcurrentPipelines: this.config.maxConcurrentPipelines,
      registeredHandlers: Array.from(this.stageHandlers.keys()),
      config: this.config
    };
  }
}