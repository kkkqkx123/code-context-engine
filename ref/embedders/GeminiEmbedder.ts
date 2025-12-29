import { LoggerService } from '../utils/LoggerService';
import { ErrorHandlerService } from '../utils/ErrorHandlerService';
import { BaseEmbedder, EmbeddingInput, EmbeddingResult } from './BaseEmbedder';

/**
 * Gemini嵌入器实现
 * 适配基础嵌入器类，简化配置获取
 */
export class GeminiEmbedder extends BaseEmbedder {
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
    this.apiKey = process.env.GEMINI_API_KEY || '';
    this.baseUrl = process.env.GEMINI_BASE_URL || 'https://generativelanguage.googleapis.com';
    this.model = process.env.GEMINI_MODEL || 'embedding-001';
    this.dimensions = parseInt(process.env.GEMINI_DIMENSIONS || '768');
  }

  async embed(
    input: EmbeddingInput | EmbeddingInput[]
  ): Promise<EmbeddingResult | EmbeddingResult[]> {
    // 检查提供者是否被禁用
    if (this.isProviderDisabled('gemini')) {
      throw new Error('Gemini provider is disabled');
    }
    
    return await this.embedWithCache(input, async inputs => {
      return await this.makeGeminiRequest(inputs);
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
    if (this.isProviderDisabled('gemini')) {
      this.logger.info('Gemini provider is disabled');
      return false;
    }
    
    try {
      if (!this.apiKey) {
        this.logger.warn('Gemini API key is not configured');
        return false;
      }

      // 尝试获取模型信息来检查可用性
      const response = await fetch(`${this.baseUrl}/v1beta/models?key=${this.apiKey}`, {
        method: 'GET',
      });

      return response.ok;
    } catch (error) {
      this.logger.warn('Gemini availability check failed', { error });
      return false;
    }
  }

  /**
   * 发送Gemini API请求
   */
 private async makeGeminiRequest(inputs: EmbeddingInput[]): Promise<EmbeddingResult[]> {
    if (!this.apiKey) {
      throw new Error('Gemini API key is not configured');
    }

    try {
      const results: EmbeddingResult[] = [];

      // Gemini API通常需要逐个处理文本
      for (const input of inputs) {
        const response = await fetch(
          `${this.baseUrl}/v1beta/models/${this.model}:embedContent?key=${this.apiKey}`,
          {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json',
            },
            body: JSON.stringify({
              content: {
                parts: [
                  {
                    text: input.text
                  }
                ]
              }
            }),
          }
        );

        if (!response.ok) {
          const errorText = await response.text();
          throw new Error(`Gemini API request failed: ${response.status} - ${errorText}`);
        }

        const data = await response.json();

        results.push({
          vector: (data.embedding && data.embedding.values) || [],
          dimensions: ((data.embedding && data.embedding.values) || []).length,
          model: this.model,
          processingTime: 0,
        });
      }

      return results;
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Gemini embedding request failed: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'GeminiEmbedder', operation: 'makeGeminiRequest' }
      );
      throw error;
    }
  }

  /**
   * 获取组件名称
   */
  private getComponentName(): string {
    return 'GeminiEmbedder';
  }
}