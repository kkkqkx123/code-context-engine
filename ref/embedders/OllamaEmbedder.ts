import { LoggerService } from '../utils/LoggerService';
import { ErrorHandlerService } from '../utils/ErrorHandlerService';
import { BaseEmbedder, EmbeddingInput, EmbeddingResult } from './BaseEmbedder';

/**
 * Ollama嵌入器实现
 * 适配基础嵌入器类，简化配置获取
 */
export class OllamaEmbedder extends BaseEmbedder {
  private baseUrl: string;
  private model: string;
  private dimensions: number;

  constructor(
    logger: LoggerService,
    errorHandler: ErrorHandlerService
  ) {
    super(logger, errorHandler);

    // 简化配置获取
    this.baseUrl = process.env.OLLAMA_BASE_URL || 'http://localhost:11434';
    this.model = process.env.OLLAMA_MODEL || 'nomic-embed-text';
    this.dimensions = parseInt(process.env.OLLAMA_DIMENSIONS || '768');
  }

  async embed(
    input: EmbeddingInput | EmbeddingInput[]
  ): Promise<EmbeddingResult | EmbeddingResult[]> {
    // 检查提供者是否被禁用
    if (this.isProviderDisabled('ollama')) {
      throw new Error('Ollama provider is disabled');
    }
    
    return await this.embedWithCache(input, async inputs => {
      return await this.makeOllamaRequest(inputs);
    });
  }

  getDimensions(): number {
    return this.dimensions;
  }

  getModelName(): string {
    return this.model;
  }

  async isAvailable(): Promise<boolean> {
    // 检查提供者是否被禁用
    if (this.isProviderDisabled('ollama')) {
      this.logger.info('Ollama provider is disabled');
      return false;
    }
    
    try {
      const response = await fetch(`${this.baseUrl}/api/tags`, {
        method: 'GET',
      });

      if (!response.ok) {
        this.logger.warn('Ollama service is not responding', {
          status: response.status,
          url: this.baseUrl
        });
        return false;
      }

      // 检查请求的模型是否可用
      const data = await response.json();
      const models = data.models || [];
      const modelNames = models.map((m: any) => m.name);

      if (!modelNames.includes(this.model)) {
        this.logger.warn('Ollama model not found', {
          model: this.model,
          availableModels: modelNames
        });
        return false;
      }

      return true;
    } catch (error) {
      this.logger.warn('Ollama availability check failed', { error });
      return false;
    }
  }

  /**
   * 发送Ollama API请求
   */
  private async makeOllamaRequest(inputs: EmbeddingInput[]): Promise<EmbeddingResult[]> {
    const results: EmbeddingResult[] = [];

    try {
      // Ollama API需要逐个处理输入
      for (const input of inputs) {
        const response = await fetch(`${this.baseUrl}/api/embeddings`, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({
            prompt: input.text,
            model: this.model,
          }),
        });

        if (!response.ok) {
          const errorText = await response.text();
          throw new Error(`Ollama API request failed: ${response.status} - ${errorText}`);
        }

        const data = await response.json();

        results.push({
          vector: data.embedding,
          dimensions: data.embedding.length,
          model: this.model,
          processingTime: 0,
        });
      }

      return results;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Ollama embedding request failed: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'OllamaEmbedder', operation: 'makeOllamaRequest' }
      );
      throw error;
    }
  }

  /**
   * 获取可用的模型列表
   */
  async getAvailableModels(): Promise<string[]> {
    try {
      const response = await fetch(`${this.baseUrl}/api/tags`, {
        method: 'GET',
      });

      if (!response.ok) {
        throw new Error(`Failed to get Ollama models: ${response.status}`);
      }

      const data = await response.json();
      const models = data.models || [];
      return models.map((m: any) => m.name);
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Failed to get available Ollama models: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'OllamaEmbedder', operation: 'getAvailableModels' }
      );
      return [];
    }
  }

  /**
   * 设置模型
   */
  setModel(model: string): void {
    this.model = model;
    this.logger.info(`Ollama model set to: ${model}`);
  }

  /**
   * 获取组件名称
   */
  private getComponentName(): string {
    return 'OllamaEmbedder';
  }
}