import { injectable, inject } from 'inversify';
import { LoggerService } from '../utils/LoggerService';
import { ErrorHandlerService } from '../utils/ErrorHandlerService';
import { Embedder, EmbeddingInput, EmbeddingResult } from './BaseEmbedder';
import { TYPES } from '../types';
import { EnvironmentUtils } from '../config/utils/EnvironmentUtils';
import { ConfigService } from '../config/ConfigService';

/**
 * 简化的嵌入器工厂
 * 使用依赖注入管理嵌入器实例
 */
@injectable()
export class EmbedderFactory {
  private logger: LoggerService;
  private errorHandler: ErrorHandlerService;
  private embedders: Map<string, Embedder> = new Map();
  private defaultProvider: string;
  private configService: ConfigService | null = null;
  // 缓存提供商信息，仅在初始化时校验所有提供商
  private providerInfoCache: Map<string, {
    name: string;
    model: string;
    dimensions: number;
    available: boolean;
    lastChecked: number;
    persistentlyUnavailable: boolean; // 标记持续不可用的提供商
  }> = new Map();
  // 缓存过期时间（15分钟，减少内存占用）
  private readonly CACHE_TTL = 15 * 60 * 1000;
  // 持续不可用的标记时间（30分钟，更快的清理）
  private readonly PERSISTENT_UNAVAILABLE_THRESHOLD = 30 * 60 * 1000;
  // 最大缓存条目数
  private readonly MAX_CACHE_SIZE = 50;
  // 是否跳过已知不可用的提供商检查
  private skipUnavailableProviderChecks: boolean;

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService
  ) {
    this.logger = logger;
    this.errorHandler = errorHandler;

    // 从环境变量获取嵌入提供者配置
    const configProvider = process.env.EMBEDDING_PROVIDER || 'openai';
    this.defaultProvider = configProvider;

    // 从环境变量获取是否跳过不可用提供商检查的配置
    this.skipUnavailableProviderChecks = process.env.SKIP_UNAVAILABLE_PROVIDER_CHECKS === 'true';

    // 初始化嵌入器
    this.initializeEmbedders();
  }

  /**
   * 设置配置服务（延迟注入）
   */
  setConfigService(configService: ConfigService): void {
    this.configService = configService;
  }

  /**
   * 初始化嵌入器
   */
  private initializeEmbedders(): void {
    try {
      // 动态导入嵌入器类，避免循环依赖
      const { OpenAIEmbedder } = require('./OpenAIEmbedder');
      const { OllamaEmbedder } = require('./OllamaEmbedder');
      const { SiliconFlowEmbedder } = require('./SiliconFlowEmbedder');
      const { CustomEmbedder } = require('./CustomEmbedder');
      const { GeminiEmbedder } = require('./GeminiEmbedder');
      const { MistralEmbedder } = require('./MistralEmbedder');

      // 创建嵌入器实例
      const openAIEmbedder = new OpenAIEmbedder(this.logger, this.errorHandler);
      const ollamaEmbedder = new OllamaEmbedder(this.logger, this.errorHandler);
      const siliconFlowEmbedder = new SiliconFlowEmbedder(this.logger, this.errorHandler);
      const custom1Embedder = new CustomEmbedder(this.logger, this.errorHandler, 'custom1');
      const custom2Embedder = new CustomEmbedder(this.logger, this.errorHandler, 'custom2');
      const custom3Embedder = new CustomEmbedder(this.logger, this.errorHandler, 'custom3');
      const similarityEmbedder = new CustomEmbedder(this.logger, this.errorHandler, 'similarity');
      const geminiEmbedder = new GeminiEmbedder(this.logger, this.errorHandler);
      const mistralEmbedder = new MistralEmbedder(this.logger, this.errorHandler);

      // 注册嵌入器
      this.registerProvider('openai', openAIEmbedder);
      this.registerProvider('ollama', ollamaEmbedder);
      this.registerProvider('siliconflow', siliconFlowEmbedder);
      this.registerProvider('custom1', custom1Embedder);
      this.registerProvider('custom2', custom2Embedder);
      this.registerProvider('custom3', custom3Embedder);
      this.registerProvider('similarity', similarityEmbedder);
      this.registerProvider('gemini', geminiEmbedder);
      this.registerProvider('mistral', mistralEmbedder);

      this.logger.info('Embedder factory initialized', {
        providers: Array.from(this.embedders.keys()),
        defaultProvider: this.defaultProvider
      });

      // 初始化时校验所有提供商的可用性并缓存结果
      this.initializeProviderInfoCache();
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to initialize embedders: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'EmbedderFactory', operation: 'initializeEmbedders' }
      );
    }
  }

  /**
   * 获取嵌入器实例
   */
  async getEmbedder(provider?: string): Promise<Embedder> {
    const selectedProvider = provider || this.defaultProvider;

    // 检查提供者是否被禁用
    if (EnvironmentUtils.isEmbeddingProviderDisabled(selectedProvider)) {
      throw new Error(`Embedder provider ${selectedProvider} is disabled`);
    }

    const embedder = this.embedders.get(selectedProvider);
    if (!embedder) {
      throw new Error(`Unsupported embedder provider: ${selectedProvider}`);
    }

    // 检查嵌入器是否可用
    const isAvailable = await embedder.isAvailable();
    if (!isAvailable) {
      throw new Error(`Embedder provider ${selectedProvider} is not available`);
    }

    return embedder;
  }

  /**
   * 初始化提供商信息缓存
   * 仅在项目初始化时校验所有提供商
   */
  private async initializeProviderInfoCache(): Promise<void> {
    const providers = Array.from(this.embedders.keys());

    for (const provider of providers) {
      try {
        // 检查提供者是否被禁用
        if (EnvironmentUtils.isEmbeddingProviderDisabled(provider)) {
          this.logger.info(`Embedder provider ${provider} is disabled, skipping initialization`);
          this.providerInfoCache.set(provider, {
            name: provider,
            model: 'unknown',
            dimensions: 0,
            available: false,
            lastChecked: Date.now(),
            persistentlyUnavailable: true
          });
          continue;
        }

        const embedder = this.embedders.get(provider)!;
        const isAvailable = await embedder.isAvailable();
        const model = embedder.getModelName();
        const dimensions = embedder.getDimensions();

        this.providerInfoCache.set(provider, {
          name: provider,
          model,
          dimensions,
          available: isAvailable,
          lastChecked: Date.now(),
          persistentlyUnavailable: false
        });

        if (isAvailable) {
          this.logger.info(`Embedder provider ${provider} is available`, {
            model,
            dimensions
          });
        } else {
          this.logger.warn(`Embedder provider ${provider} is not available`);
        }
      } catch (error) {
        this.logger.warn(`Failed to initialize provider info for ${provider}:`, { error });

        // 即使初始化失败，也缓存默认信息
        this.providerInfoCache.set(provider, {
          name: provider,
          model: 'unknown',
          dimensions: 0,
          available: false,
          lastChecked: Date.now(),
          persistentlyUnavailable: false
        });
      }
    }

    this.logger.info('Provider info cache initialized', {
      cachedProviders: Array.from(this.providerInfoCache.keys())
    });
  }

  /**
   * 生成嵌入
   */
  async embed(
    input: EmbeddingInput | EmbeddingInput[],
    provider?: string
  ): Promise<EmbeddingResult | EmbeddingResult[]> {
    try {
      const embedder = await this.getEmbedder(provider);
      const selectedProvider = provider || this.defaultProvider;
      
      // 获取嵌入器的最大批处理大小
      const maxBatchSize = this.getEmbedderMaxBatchSize(selectedProvider);
      const inputs = Array.isArray(input) ? input : [input];
      
      // 如果输入数量小于等于最大批处理大小，则直接处理
      if (inputs.length <= maxBatchSize) {
        return embedder.embed(input);
      }
      
      // 分批处理
      const results: EmbeddingResult[] = [];
      for (let i = 0; i < inputs.length; i += maxBatchSize) {
        const batch = inputs.slice(i, i + maxBatchSize);
        const batchResults = await embedder.embed(batch);
        results.push(...(Array.isArray(batchResults) ? batchResults : [batchResults]));
      }
      
      return results;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to generate embeddings: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'EmbedderFactory', operation: 'embed' }
      );
      throw error;
    }
  }
  /**
   * 获取可用的嵌入器提供者列表
   * 现在只检查配置中指定的提供者
   */
  async getAvailableProviders(): Promise<string[]> {
    const available: string[] = [];

    // 只检查配置中指定的提供者
    const configProvider = process.env.EMBEDDING_PROVIDER || 'openai';

    // 检查提供者是否被禁用
    if (EnvironmentUtils.isEmbeddingProviderDisabled(configProvider)) {
      this.logger.info(`Configured embedder provider is disabled: ${configProvider}`);
      return available; // 返回空数组，表示没有可用的提供者
    }

    if (this.embedders.has(configProvider)) {
      try {
        const embedder = this.embedders.get(configProvider)!;
        const isAvailable = await embedder.isAvailable();
        if (isAvailable) {
          available.push(configProvider);
          this.logger.info(`Configured embedder provider is available: ${configProvider}`);
        } else {
          this.logger.warn(`Configured embedder provider is not available: ${configProvider}`);
        }
      } catch (error) {
        this.logger.warn(`Failed to check availability for configured embedder ${configProvider}`, { error });
      }
    } else {
      this.logger.warn(`Configured embedder provider not found: ${configProvider}`);
    }

    return available;
  }

  /**
   * 获取嵌入器信息
   * 优先使用缓存数据，仅对当前使用的提供商进行实时检查
   */
  async getProviderInfo(provider?: string): Promise<{
    name: string;
    model: string;
    dimensions: number;
    available: boolean;
  }> {
    try {
      const selectedProvider = provider || this.defaultProvider;
      
      // 检查提供者是否被禁用
      if (EnvironmentUtils.isEmbeddingProviderDisabled(selectedProvider)) {
        this.logger.info(`Provider is disabled: ${selectedProvider}`);
        return {
          name: selectedProvider,
          model: 'unknown',
          dimensions: 0,
          available: false,
        };
      }
      
      // 检查缓存中是否有该提供商的信息
      const cachedInfo = this.providerInfoCache.get(selectedProvider);
      const now = Date.now();
      
      // 如果配置为跳过不可用提供商检查，并且该提供商已被标记为持续不可用，则直接返回缓存数据
      if (this.skipUnavailableProviderChecks && cachedInfo && cachedInfo.persistentlyUnavailable) {
        this.logger.debug(`Skipping availability check for persistently unavailable provider: ${selectedProvider}`);
        return {
          name: selectedProvider,
          model: cachedInfo.model,
          dimensions: cachedInfo.dimensions,
          available: false,
        };
      }
      
      // 如果缓存存在且未过期，直接返回缓存数据
      if (cachedInfo && (now - cachedInfo.lastChecked < this.CACHE_TTL)) {
        // 对于当前使用的提供商，仍然进行实时检查以确保其可用性
        if (selectedProvider === this.defaultProvider) {
          const embedder = this.embedders.get(selectedProvider);
          if (embedder) {
            const isAvailable = await embedder.isAvailable();
            // 更新缓存中的可用性状态
            this.providerInfoCache.set(selectedProvider, {
              ...cachedInfo,
              available: isAvailable,
              lastChecked: now,
              persistentlyUnavailable: isAvailable ? false : cachedInfo.persistentlyUnavailable
            });
            
            return {
              name: selectedProvider,
              model: cachedInfo.model,
              dimensions: cachedInfo.dimensions,
              available: isAvailable,
            };
          }
        }
        
        // 对于非当前使用的提供商，直接返回缓存数据
        return {
          name: selectedProvider,
          model: cachedInfo.model,
          dimensions: cachedInfo.dimensions,
          available: cachedInfo.available,
        };
      }
      
      // 如果没有缓存或缓存已过期，获取实时信息并更新缓存
      try {
        const embedder = await this.getEmbedder(selectedProvider);
        const available = await embedder.isAvailable();
        const model = embedder.getModelName();
        const dimensions = embedder.getDimensions();
        
        // 如果提供商不可用，检查是否应该将其标记为持续不可用
        let persistentlyUnavailable = false;
        if (!available && cachedInfo && !cachedInfo.available) {
          // 如果提供商之前就不可用，并且已经超过了持续不可用的时间阈值，则标记为持续不可用
          if (now - cachedInfo.lastChecked > this.PERSISTENT_UNAVAILABLE_THRESHOLD) {
            persistentlyUnavailable = true;
            this.logger.info(`Marking provider as persistently unavailable: ${selectedProvider}`);
          }
        }
        
        // 更新缓存，同时检查缓存大小限制
        if (this.providerInfoCache.size >= this.MAX_CACHE_SIZE) {
          // 清理最旧的缓存条目
          const oldestEntries = Array.from(this.providerInfoCache.entries())
            .sort((a, b) => a[1].lastChecked - b[1].lastChecked)
            .slice(0, Math.floor(this.MAX_CACHE_SIZE / 2));

          for (const [key] of oldestEntries) {
            this.providerInfoCache.delete(key);
          }
        }

        this.providerInfoCache.set(selectedProvider, {
          name: selectedProvider,
          model,
          dimensions,
          available,
          lastChecked: now,
          persistentlyUnavailable
        });
        
        return {
          name: selectedProvider,
          model,
          dimensions,
          available,
        };
      } catch (error) {
        // 如果获取提供商信息失败，并且之前缓存显示该提供商不可用，则可能标记为持续不可用
        let persistentlyUnavailable = false;
        if (cachedInfo && !cachedInfo.available) {
          if (now - cachedInfo.lastChecked > this.PERSISTENT_UNAVAILABLE_THRESHOLD) {
            persistentlyUnavailable = true;
            this.logger.info(`Marking provider as persistently unavailable due to repeated failures: ${selectedProvider}`);
          }
        }
        
        // 使用缓存的失败信息（如果有）
        if (cachedInfo) {
          this.providerInfoCache.set(selectedProvider, {
            ...cachedInfo,
            lastChecked: now,
            persistentlyUnavailable
          });
          
          return {
            name: selectedProvider,
            model: cachedInfo.model,
            dimensions: cachedInfo.dimensions,
            available: false,
          };
        }
        
        // 如果没有缓存，则抛出错误
        throw error;
      }
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to get provider info: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'EmbedderFactory', operation: 'getProviderInfo' }
      );
      throw error;
    }
  }

  /**
   * 自动选择可用的嵌入器提供者
   */
  async autoSelectProvider(): Promise<string> {
    try {
      const available = await this.getAvailableProviders();

      if (available.length === 0) {
        throw new Error('No embedder providers available');
      }

      // 优先返回默认提供者（如果可用）
      if (available.includes(this.defaultProvider)) {
        return this.defaultProvider;
      }

      // 否则返回第一个可用的提供者
      return available[0];
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to auto-select provider: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'EmbedderFactory', operation: 'autoSelectProvider' }
      );
      throw error;
    }
  }

  /**
   * 注册嵌入器提供者
   */
  registerProvider(name: string, embedder: Embedder): void {
    this.embedders.set(name, embedder);
    this.logger.info(`Registered embedder provider: ${name}`);
  }

  /**
   * 获取已注册的嵌入器提供者列表
   */
  getRegisteredProviders(): string[] {
    return Array.from(this.embedders.keys());
  }

  /**
   * 检查提供者是否已注册
   */
  isProviderRegistered(name: string): boolean {
    return this.embedders.has(name);
  }

  /**
   * 移除嵌入器提供者
   */
  unregisterProvider(name: string): boolean {
    const removed = this.embedders.delete(name);
    if (removed) {
      this.logger.info(`Unregistered embedder provider: ${name}`);
    }
    return removed;
  }

  /**
   * 获取默认提供者
   */
  getDefaultProvider(): string {
    return this.defaultProvider;
  }

  /**
   * 设置默认提供者
   */
  setDefaultProvider(provider: string): void {
    if (!this.embedders.has(provider)) {
      throw new Error(`Provider ${provider} is not registered`);
    }
    this.defaultProvider = provider;
    this.logger.info(`Default embedder provider set to: ${provider}`);
  }

  /**
   * 获取嵌入器特定的批处理大小限制
   * @param provider 嵌入器提供商名称
   * @returns 最大批处理大小
   */
  private getEmbedderMaxBatchSize(provider: string): number {
    try {
      // 尝试从配置服务获取配置
      if (this.configService) {
        const embeddingBatchConfig = this.configService.get('embeddingBatch');
        if (embeddingBatchConfig) {
          // 根据提供商名称获取对应的批处理限制
          switch(provider) {
            case 'openai':
              return embeddingBatchConfig.providerBatchLimits.openai;
            case 'siliconflow':
              return embeddingBatchConfig.providerBatchLimits.siliconflow;
            case 'ollama':
              return embeddingBatchConfig.providerBatchLimits.ollama;
            case 'gemini':
              return embeddingBatchConfig.providerBatchLimits.gemini;
            case 'mistral':
              return embeddingBatchConfig.providerBatchLimits.mistral;
            case 'custom1':
              return embeddingBatchConfig.providerBatchLimits.custom1;
            case 'custom2':
              return embeddingBatchConfig.providerBatchLimits.custom2;
            case 'custom3':
              return embeddingBatchConfig.providerBatchLimits.custom3;
            case 'similarity':
              return embeddingBatchConfig.providerBatchLimits.similarity;
            default:
              return embeddingBatchConfig.defaultBatchSize;
          }
        }
      }
    } catch (error) {
      this.logger.warn('Failed to get embedding batch config, using defaults', { error });
    }

    // 如果无法从配置服务获取配置，则使用默认值
    // 检查环境变量中的全局嵌入批处理大小
    const globalBatchSize = process.env.EMBEDDING_BATCH_SIZE;
    if (globalBatchSize) {
      const size = parseInt(globalBatchSize, 10);
      if (!isNaN(size) && size > 0) {
        return size;
      }
    }

    // 根据不同的提供商返回特定的最大批处理大小
    const providerBatchLimits: Record<string, number> = {
      'openai': 2048,      // OpenAI 实际限制
      'siliconflow': 64,   // SiliconFlow 限制
      'ollama': 128,       // Ollama 限制
      'gemini': 100,       // Gemini 限制
      'mistral': 512,      // Mistral 限制
      'custom1': 100,      // 自定义提供商1
      'custom2': 100,      // 自定义提供商2
      'custom3': 100,      // 自定义提供商3
      'default': 50        // 默认安全值
    };

    return providerBatchLimits[provider] || providerBatchLimits.default;
  }
}