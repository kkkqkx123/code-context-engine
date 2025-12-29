import { LoggerService } from '../utils/LoggerService';
import { ErrorHandlerService } from '../utils/ErrorHandlerService';
import { BaseEmbedder, EmbeddingInput, EmbeddingResult } from './BaseEmbedder';

/**
 * 自定义HTTP嵌入器实现
 * 支持CUSTOM_CUSTOM1格式的配置
 * SIMILARITY也使用该文件实现
 */
export class CustomEmbedder extends BaseEmbedder {
  private apiKey: string;
  private baseUrl: string;
  private model: string;
  private dimensions: number;
  private providerName: string;

  constructor(
    logger: LoggerService,
    errorHandler: ErrorHandlerService,
    providerName: string = 'custom1'
  ) {
    super(logger, errorHandler);

    this.providerName = providerName;

    // 根据providerName获取相应的配置
    let prefix: string;
    let defaultModel: string;
    let defaultDimensions: string;

    if (providerName === 'similarity') {
      prefix = 'SIMILARITY';
      defaultModel = 'sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2';
      defaultDimensions = '384';
    } else {
      prefix = `CUSTOM_${providerName.toUpperCase()}`;
      defaultModel = 'default-model';
      defaultDimensions = '768';
    }

    this.apiKey = process.env[`${prefix}_API_KEY`] || '';
    this.baseUrl = process.env[`${prefix}_BASE_URL`] || '';
    this.model = process.env[`${prefix}_MODEL`] || defaultModel;
    this.dimensions = parseInt(process.env[`${prefix}_DIMENSIONS`] || defaultDimensions);
  }

  async embed(
    input: EmbeddingInput | EmbeddingInput[]
  ): Promise<EmbeddingResult | EmbeddingResult[]> {
    // 检查提供者是否被禁用 - 使用实际的providerName
    if (this.isProviderDisabled(this.providerName)) {
      throw new Error(`Custom provider ${this.providerName} is disabled`);
    }

    return await this.embedWithCache(input, async inputs => {
      return await this.makeCustomRequest(inputs);
    });
  }

  getDimensions(): number {
    return this.dimensions;
  }

  getModelName(): string {
    return this.model;
  }

  async isAvailable(): Promise<boolean> {
    // 检查提供者是否被禁用 - 使用实际的providerName
    if (this.isProviderDisabled(this.providerName)) {
      this.logger.info(`Custom provider ${this.providerName} is disabled`);
      return false;
    }

    try {
      if (!this.baseUrl) {
        this.logger.warn('Custom embedder base URL is not configured');
        return false;
      }

      // 尝试发送一个简单的请求来检查服务是否可用
      const response = await fetch(this.baseUrl, {
        method: 'GET',
        headers: this.getHeaders(),
      });

      return response.ok;
    } catch (error) {
      this.logger.warn('Custom embedder availability check failed', { error });
      return false;
    }
  }

  /**
   * 发送自定义API请求
   */
  private async makeCustomRequest(inputs: EmbeddingInput[]): Promise<EmbeddingResult[]> {
    if (!this.baseUrl) {
      throw new Error('Custom embedder base URL is not configured');
    }

    try {
      const response = await fetch(this.baseUrl, {
        method: 'POST',
        headers: this.getHeaders(),
        body: JSON.stringify({
          input: inputs.map(inp => inp.text),
          model: this.model,
        }),
      });

      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`Custom embedder API request failed: ${response.status} - ${errorText}`);
      }

      const data = await response.json();

      // 根据API响应格式处理结果
      return this.processApiResponse(data);
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`Custom embedder request failed: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'CustomEmbedder', operation: 'makeCustomRequest', provider: this.providerName }
      );
      throw error;
    }
  }

  /**
   * 获取请求头
   */
  private getHeaders(): Record<string, string> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
    };

    if (this.apiKey) {
      headers['Authorization'] = `Bearer ${this.apiKey}`;
    }

    return headers;
  }

  /**
   * 处理API响应
   */
  private processApiResponse(data: any): EmbeddingResult[] {
    // 处理常见的嵌入API响应格式
    if (data.data && Array.isArray(data.data)) {
      return data.data.map((item: any) => ({
        vector: item.embedding || item.vector,
        dimensions: item.embedding?.length || item.dimensions || this.dimensions,
        model: item.model || this.model,
        processingTime: 0,
      }));
    }

    // 处理直接返回嵌入向量的格式
    if (data.embeddings && Array.isArray(data.embeddings)) {
      return data.embeddings.map((embedding: any) => ({
        vector: embedding,
        dimensions: embedding.length,
        model: this.model,
        processingTime: 0,
      }));
    }

    // 处理单个嵌入结果
    if (data.embedding || data.vector) {
      return [{
        vector: data.embedding || data.vector,
        dimensions: (data.embedding || data.vector).length,
        model: data.model || this.model,
        processingTime: 0,
      }];
    }

    throw new Error('Unsupported API response format');
  }

  /**
   * 获取组件名称
   */
  private getComponentName(): string {
    return `CustomEmbedder(${this.providerName})`;
  }
}