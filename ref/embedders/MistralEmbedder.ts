import { LoggerService } from '../utils/LoggerService';
import { ErrorHandlerService } from '../utils/ErrorHandlerService';
import { BaseEmbedder, EmbeddingInput, EmbeddingResult } from './BaseEmbedder';

/**
 * Mistral嵌入器实现
 * 适配基础嵌入器类，简化配置获取
 */
export class MistralEmbedder extends BaseEmbedder {
  private apiKey: string;
  private baseUrl: string;
  private model: string;
  private dimensions: number;

  constructor(
    logger: LoggerService,
    errorHandler: ErrorHandlerService
  ) {
    super(logger, errorHandler);

    // 简化配置获取
    this.apiKey = process.env.MISTRAL_API_KEY || '';
    this.baseUrl = process.env.MISTRAL_BASE_URL || 'https://api.mistral.ai';
    this.model = process.env.MISTRAL_MODEL || 'mistral-embed';
    this.dimensions = parseInt(process.env.MISTRAL_DIMENSIONS || '1024');
  }

  async embed(
    input: EmbeddingInput | EmbeddingInput[]
  ): Promise<EmbeddingResult | EmbeddingResult[]> {
    // 检查提供者是否被禁用
    if (this.isProviderDisabled('mistral')) {
      throw new Error('Mistral provider is disabled');
    }
    
    return await this.embedWithCache(input, async inputs => {
      return await this.makeMistralRequest(inputs);
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
    if (this.isProviderDisabled('mistral')) {
      this.logger.info('Mistral provider is disabled');
      return false;
    }
    
    try {
      if (!this.apiKey) {
        this.logger.warn('Mistral API key is not configured');
        return false;
      }

      // 尝试获取模型列表来检查可用性
      const response = await fetch(`${this.baseUrl}/v1/models`, {
        method: 'GET',
        headers: {
          'Authorization': `Bearer ${this.apiKey}`,
          'Content-Type': 'application/json',
        },
      });

      return response.ok;
    } catch (error) {
      this.logger.warn('Mistral availability check failed', { error });
      return false;
    }
  }

  /**
   * 发送Mistral API请求
   */
  private async makeMistralRequest(inputs: EmbeddingInput[]): Promise<EmbeddingResult[]> {
    if (!this.apiKey) {
      throw new Error('Mistral API key is not configured');
    }

    try {
      const response = await fetch(`${this.baseUrl}/v1/embeddings`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${this.apiKey}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          input: inputs.map(inp => inp.text),
          model: this.model,
        }),
      });

      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`Mistral API request failed: ${response.status} - ${errorText}`);
      }

      const data = await response.json();

      return data.data.map((item: any) => ({
        vector: item.embedding,
        dimensions: item.embedding.length,
        model: this.model,
        processingTime: 0,
      }));
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Mistral embedding request failed: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'MistralEmbedder', operation: 'makeMistralRequest' }
      );
      throw error;
    }
  }

  /**
   * 获取组件名称
   */
  private getComponentName(): string {
    return 'MistralEmbedder';
  }
}