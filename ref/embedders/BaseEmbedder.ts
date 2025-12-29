import { LoggerService } from '../utils/LoggerService';
import { ErrorHandlerService } from '../utils/ErrorHandlerService';
import { EnvironmentUtils } from '../config/utils/EnvironmentUtils';

export interface EmbeddingInput {
  text: string;
  metadata?: Record<string, any>;
}

export interface EmbeddingResult {
  vector: number[];
  dimensions: number;
  model: string;
  processingTime: number;
}

export interface Embedder {
  embed(input: EmbeddingInput | EmbeddingInput[]): Promise<EmbeddingResult | EmbeddingResult[]>;
  getDimensions(): number;
  getModelName(): string;
  isAvailable(): Promise<boolean>;
}

export abstract class BaseEmbedder implements Embedder {
  protected logger: LoggerService;
  protected errorHandler: ErrorHandlerService;
  protected timeout: number;
  protected maxConcurrent: number;
  private activeRequests: number = 0;
  private requestQueue: Array<() => void> = [];

  constructor(
    logger: LoggerService,
    errorHandler: ErrorHandlerService
  ) {
    this.logger = logger;
    this.errorHandler = errorHandler;

    // 简化配置获取
    this.timeout = parseInt(process.env.EMBEDDING_TIMEOUT || '300000'); // Default to 5 minutes
    this.maxConcurrent = parseInt(process.env.EMBEDDING_MAX_CONCURRENT || '5');
  }

  abstract embed(
    input: EmbeddingInput | EmbeddingInput[]
  ): Promise<EmbeddingResult | EmbeddingResult[]>;
  abstract getDimensions(): number;
  abstract getModelName(): string;
  abstract isAvailable(): Promise<boolean>;

  /**
   * 检查提供者是否被禁用
   * @param providerName 提供者名称
   * @returns 如果提供者被禁用则返回true，否则返回false
   */
  protected isProviderDisabled(providerName: string): boolean {
    return EnvironmentUtils.isEmbeddingProviderDisabled(providerName);
  }

  /**
    * Common embedding logic with cache checking and result combination
    */
  protected async embedWithCache(
    input: EmbeddingInput | EmbeddingInput[],
    processEmbeddings: (inputs: EmbeddingInput[]) => Promise<EmbeddingResult[]>,
    cacheService?: any
  ): Promise<EmbeddingResult | EmbeddingResult[]> {
    const inputs = Array.isArray(input) ? input : [input];

    // Check cache for existing embeddings if cache service is available
    const cachedResults: EmbeddingResult[] = [];
    const uncachedInputs: EmbeddingInput[] = [];

    if (cacheService) {
      for (const inp of inputs) {
        const cached = await cacheService.get(inp.text, this.getModelName());
        if (cached) {
          cachedResults.push(cached);
        } else {
          uncachedInputs.push(inp);
        }
      }
    } else {
      // If no cache service, all inputs need to be processed
      uncachedInputs.push(...inputs);
    }

    // If all inputs are cached, return cached results
    if (uncachedInputs.length === 0) {
      this.logger.debug('All embeddings found in cache', { count: cachedResults.length });
      return Array.isArray(input) ? cachedResults : cachedResults[0];
    }

    try {
      // Wait for available request slot
      await this.waitForAvailableSlot();

      const { result, time } = await this.executeWithTimeout(async () => {
        return await this.measureTime(async () => {
          return await processEmbeddings(uncachedInputs);
        });
      });

      // Release request slot
      this.releaseSlot();

      // Update processingTime with the actual measured time
      const apiResults = Array.isArray(result) ? result : [result];
      apiResults.forEach(embedding => {
        embedding.processingTime = time;
      });

      // Cache the new results if cache service is available
      if (cacheService) {
        for (let i = 0; i < apiResults.length; i++) {
          await cacheService.set(uncachedInputs[i].text, this.getModelName(), apiResults[i]);
        }
      }

      // Combine cached and new results
      const finalResult = [...cachedResults, ...apiResults];

      return Array.isArray(input) ? finalResult : finalResult[0];
    } catch (error) {
      // Release request slot in case of error
      this.releaseSlot();
      throw error;
    }
  }

  protected async measureTime<T>(
    operation: () => Promise<T>
  ): Promise<{ result: T; time: number }> {
    const startTime = Date.now();
    const result = await operation();
    const endTime = Date.now();
    return { result, time: endTime - startTime };
  }

  /**
   * Wait for available request slot based on concurrency limits
   */
  protected async waitForAvailableSlot(): Promise<void> {
    return new Promise(resolve => {
      if (this.activeRequests < this.maxConcurrent) {
        this.activeRequests++;
        resolve();
      } else {
        this.requestQueue.push(() => {
          this.activeRequests++;
          resolve();
        });
      }
    });
  }

  /**
   * Release a request slot
   */
  protected releaseSlot(): void {
    this.activeRequests--;

    // Process next request in queue if available
    if (this.requestQueue.length > 0) {
      const next = this.requestQueue.shift();
      if (next) {
        setTimeout(next, 0); // Schedule for next tick
      }
    }
  }

  /**
   * Execute an operation with timeout
   */
  protected async executeWithTimeout<T>(operation: () => Promise<T>): Promise<T> {
    return new Promise((resolve, reject) => {
      // Set up timeout
      const timeoutId = setTimeout(() => {
        reject(new Error(`Operation timed out after ${this.timeout}ms`));
      }, this.timeout);

      // Execute operation
      operation()
        .then(result => {
          clearTimeout(timeoutId);
          resolve(result);
        })
        .catch(error => {
          clearTimeout(timeoutId);
          reject(error);
        });
    });
  }
}

