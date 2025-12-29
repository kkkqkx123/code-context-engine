import { LoggerService } from '../utils/LoggerService';
import { ErrorHandlerService } from '../utils/ErrorHandlerService';
import { BaseEmbedder, EmbeddingInput, EmbeddingResult } from './BaseEmbedder';

/**
 * OpenAI嵌入器实现
 * 适配基础嵌入器类，简化配置获取
 */
export class OpenAIEmbedder extends BaseEmbedder {
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
    this.apiKey = process.env.OPENAI_API_KEY || '';
    this.baseUrl = process.env.OPENAI_BASE_URL || 'https://api.openai.com';
    this.model = process.env.OPENAI_MODEL || 'text-embedding-ada-002';
    this.dimensions = parseInt(process.env.OPENAI_DIMENSIONS || '1536');
  }

  async embed(
    input: EmbeddingInput | EmbeddingInput[]
  ): Promise<EmbeddingResult | EmbeddingResult[]> {
    // 检查提供者是否被禁用
    if (this.isProviderDisabled('openai')) {
      throw new Error('OpenAI provider is disabled');
    }
    
    return await this.embedWithCache(input, async inputs => {
      return await this.makeOpenAIRequest(inputs);
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
    if (this.isProviderDisabled('openai')) {
      this.logger.info('OpenAI provider is disabled');
      return false;
    }
    
    try {
      if (!this.apiKey) {
        this.logger.warn('OpenAI API key is not configured');
        return false;
      }

      const response = await fetch(`${this.baseUrl}/v1/models`, {
        method: 'GET',
        headers: {
          'Authorization': `Bearer ${this.apiKey}`,
          'Content-Type': 'application/json',
        },
      });

      return response.ok;
    } catch (error) {
      this.logger.warn('OpenAI availability check failed', { error });
      return false;
    }
  }

  /**
   * 发送OpenAI API请求
   */
  private async makeOpenAIRequest(inputs: EmbeddingInput[]): Promise<EmbeddingResult[]> {
    if (!this.apiKey) {
      throw new Error('OpenAI API key is not configured');
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
        throw new Error(`OpenAI API request failed: ${response.status} - ${errorText}`);
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
        new Error(`OpenAI embedding request failed: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'OpenAIEmbedder', operation: 'makeOpenAIRequest' }
      );
      throw error;
    }
  }

  /**
   * 获取组件名称
   */
  private getComponentName(): string {
    return 'OpenAIEmbedder';
  }
}