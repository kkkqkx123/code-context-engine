import { LoggerService } from '../utils/LoggerService';
import { ErrorHandlerService } from '../utils/ErrorHandlerService';
import { BaseEmbedder, EmbeddingInput, EmbeddingResult } from './BaseEmbedder';

/**
 * SiliconFlow嵌入器实现
 * 适配基础嵌入器类，简化配置获取
 */
export class SiliconFlowEmbedder extends BaseEmbedder {
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
    this.apiKey = process.env.SILICONFLOW_API_KEY || '';
    this.baseUrl = process.env.SILICONFLOW_BASE_URL || 'https://api.siliconflow.cn/v1';
    this.model = process.env.SILICONFLOW_MODEL || 'BAAI/bge-m3';
    this.dimensions = parseInt(process.env.SILICONFLOW_DIMENSIONS || '1024');
  }

  async embed(
    input: EmbeddingInput | EmbeddingInput[]
  ): Promise<EmbeddingResult | EmbeddingResult[]> {
    // 检查提供者是否被禁用
    if (this.isProviderDisabled('siliconflow')) {
      throw new Error('SiliconFlow provider is disabled');
    }
    
    return await this.embedWithCache(input, async inputs => {
      return await this.makeSiliconFlowRequest(inputs);
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
    if (this.isProviderDisabled('siliconflow')) {
      this.logger.info('SiliconFlow provider is disabled');
      return false;
    }
    
    try {
      if (!this.apiKey) {
        this.logger.warn('SiliconFlow API key is not configured');
        return false;
      }

      const response = await fetch(`${this.baseUrl}/models`, {
        method: 'GET',
        headers: {
          'Authorization': `Bearer ${this.apiKey}`,
          'Content-Type': 'application/json',
        },
      });

      return response.ok;
    } catch (error) {
      this.logger.warn('SiliconFlow availability check failed', { error });
      return false;
    }
  }

  /**
   * 发送SiliconFlow API请求
   */
  private async makeSiliconFlowRequest(inputs: EmbeddingInput[]): Promise<EmbeddingResult[]> {
    if (!this.apiKey) {
      throw new Error('SiliconFlow API key is not configured');
    }

    try {
      const response = await fetch(`${this.baseUrl}/embeddings`, {
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
        throw new Error(`SiliconFlow API request failed: ${response.status} - ${errorText}`);
      }

      const data = await response.json();
      
      // 调试：记录API响应数据结构
      this.logger.debug('SiliconFlow API response structure', {
        hasData: !!data.data,
        dataLength: data.data?.length,
        firstItemKeys: data.data?.[0] ? Object.keys(data.data[0]) : [],
        sampleEmbeddingType: typeof data.data?.[0]?.embedding,
        sampleEmbeddingLength: Array.isArray(data.data?.[0]?.embedding) ? data.data[0].embedding.length : 0,
        sampleEmbeddingFirst5: Array.isArray(data.data?.[0]?.embedding) ? data.data[0].embedding.slice(0, 5) : []
      });

      return data.data.map((item: any) => {
        // 确保向量数据是纯数字数组
        const embeddingArray = Array.isArray(item.embedding)
          ? item.embedding.map((val: any) => {
              // 转换所有值为数字类型
              const num = Number(val);
              if (isNaN(num)) {
                this.logger.warn('Invalid embedding value found, replacing with 0', {
                  originalValue: val,
                  type: typeof val
                });
                return 0;
              }
              return num;
            })
          : [];

        // 验证向量维度和内容
        if (embeddingArray.length !== this.dimensions) {
          this.logger.warn(`Embedding dimension mismatch: expected ${this.dimensions}, got ${embeddingArray.length}`);
        }

        this.logger.debug('Processed embedding sample', {
          length: embeddingArray.length,
          first5Values: embeddingArray.slice(0, 5),
          allNumeric: embeddingArray.every((val: any) => typeof val === 'number' && !isNaN(val))
        });

        return {
          vector: embeddingArray,
          dimensions: embeddingArray.length,
          model: this.model,
          processingTime: 0, // 将由基类的 measureTime 方法自动更新为实际处理时间
        };
      });
    } catch (error) {
      this.errorHandler.handleError(
        new Error(`SiliconFlow embedding request failed: ${error instanceof Error ? error.message : String(error)}`),
        { component: 'SiliconFlowEmbedder', operation: 'makeSiliconFlowRequest' }
      );
      throw error;
    }
  }

  /**
   * 获取组件名称
   */
  private getComponentName(): string {
    return 'SiliconFlowEmbedder';
  }
}