import { injectable, inject } from 'inversify';
import { EventEmitter } from 'events';
import { TYPES } from '../../../types';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { PerformanceMonitor } from '../../../infrastructure/monitoring/PerformanceMonitor';
import { Connection, ConnectionState } from './Connection';

// 负载均衡策略
export enum LoadBalanceStrategy {
  ROUND_ROBIN = 'round_robin',
  LEAST_CONNECTIONS = 'least_connections',
  LEAST_RESPONSE_TIME = 'least_response_time',
  WEIGHTED_ROUND_ROBIN = 'weighted_round_robin',
  RANDOM = 'random'
}

// 连接权重信息
export interface ConnectionWeight {
  connectionId: string;
  weight: number;
  responseTime: number;
  errorRate: number;
  connectionCount: number;
  lastUpdated: Date;
}

// 负载均衡配置
export interface LoadBalancerConfig {
  strategy: LoadBalanceStrategy;
  healthCheckWeight: number;
  responseTimeWeight: number;
  errorRateWeight: number;
  connectionCountWeight: number;
  weightUpdateInterval: number;
  maxWeight: number;
  minWeight: number;
}

// 默认负载均衡配置
const DEFAULT_LOAD_BALANCER_CONFIG: LoadBalancerConfig = {
  strategy: LoadBalanceStrategy.LEAST_RESPONSE_TIME,
  healthCheckWeight: 0.3,
  responseTimeWeight: 0.4,
  errorRateWeight: 0.2,
  connectionCountWeight: 0.1,
  weightUpdateInterval: 30000, // 30秒
  maxWeight: 100,
  minWeight: 1
};

/**
 * 智能负载均衡器
 * 负责根据不同策略选择最优连接
 */
@injectable()
export class LoadBalancer extends EventEmitter {
  private logger: LoggerService;
  private errorHandler: ErrorHandlerService;
  private performanceMonitor: PerformanceMonitor;
  private config: LoadBalancerConfig;
  private connectionWeights: Map<string, ConnectionWeight> = new Map();
  private roundRobinIndex: number = 0;
  private weightUpdateTimer: NodeJS.Timeout | null = null;

  constructor(
    @inject(TYPES.LoggerService) logger: LoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.PerformanceMonitor) performanceMonitor: PerformanceMonitor
  ) {
    super();
    this.logger = logger;
    this.errorHandler = errorHandler;
    this.performanceMonitor = performanceMonitor;
    this.config = { ...DEFAULT_LOAD_BALANCER_CONFIG };
    
    this.startWeightUpdateTimer();
  }

  /**
   * 更新负载均衡配置
   */
  updateConfig(config: Partial<LoadBalancerConfig>): void {
    this.config = { ...this.config, ...config };
    
    if (config.weightUpdateInterval) {
      this.restartWeightUpdateTimer();
    }
    
    this.logger.info('Load balancer config updated', { config: this.config });
  }

  /**
   * 选择最优连接
   */
  selectConnection(connections: Connection[]): Connection | null {
    if (connections.length === 0) {
      return null;
    }

    // 过滤健康的连接
    const healthyConnections = connections.filter(conn => 
      conn.getState() === ConnectionState.IDLE
    );

    if (healthyConnections.length === 0) {
      this.logger.warn('No healthy connections available for load balancing');
      return null;
    }

    let selectedConnection: Connection | null = null;

    try {
      switch (this.config.strategy) {
        case LoadBalanceStrategy.ROUND_ROBIN:
          selectedConnection = this.selectRoundRobin(healthyConnections);
          break;
        case LoadBalanceStrategy.LEAST_CONNECTIONS:
          selectedConnection = this.selectLeastConnections(healthyConnections);
          break;
        case LoadBalanceStrategy.LEAST_RESPONSE_TIME:
          selectedConnection = this.selectLeastResponseTime(healthyConnections);
          break;
        case LoadBalanceStrategy.WEIGHTED_ROUND_ROBIN:
          selectedConnection = this.selectWeightedRoundRobin(healthyConnections);
          break;
        case LoadBalanceStrategy.RANDOM:
          selectedConnection = this.selectRandom(healthyConnections);
          break;
        default:
          selectedConnection = healthyConnections[0];
      }

      if (selectedConnection) {
        this.emit('connectionSelected', {
          connectionId: selectedConnection.getId(),
          strategy: this.config.strategy,
          totalConnections: healthyConnections.length
        });
      }

      return selectedConnection;
    } catch (error) {
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error('Load balancing failed'),
        { component: 'LoadBalancer', operation: 'selectConnection' }
      );
      return healthyConnections[0]; // 降级到第一个可用连接
    }
  }

  /**
   * 更新连接权重
   */
  updateConnectionWeight(connection: Connection): void {
    const stats = connection.getStats();
    const connectionId = connection.getId();

    // 计算响应时间权重（响应时间越低权重越高）
    const avgResponseTime = stats.queryCount > 0 ? stats.totalQueryTime / stats.queryCount : 1000;
    const responseTimeScore = Math.max(0, 1000 - avgResponseTime) / 1000;

    // 计算错误率权重（错误率越低权重越高）
    const errorRate = stats.queryCount > 0 ? stats.errorCount / stats.queryCount : 0;
    const errorRateScore = Math.max(0, 1 - errorRate);

    // 计算连接数权重（连接数越少权重越高）
    const connectionCountScore = Math.max(0, 1 - (stats.queryCount / 1000));

    // 计算健康状态权重
    const healthScore = connection.getState() === ConnectionState.IDLE ? 1 : 0;

    // 综合权重计算
    const weight = Math.round(
      healthScore * this.config.healthCheckWeight * 100 +
      responseTimeScore * this.config.responseTimeWeight * 100 +
      errorRateScore * this.config.errorRateWeight * 100 +
      connectionCountScore * this.config.connectionCountWeight * 100
    );

    const connectionWeight: ConnectionWeight = {
      connectionId,
      weight: Math.max(this.config.minWeight, Math.min(this.config.maxWeight, weight)),
      responseTime: avgResponseTime,
      errorRate,
      connectionCount: stats.queryCount,
      lastUpdated: new Date()
    };

    this.connectionWeights.set(connectionId, connectionWeight);
  }

  /**
   * 轮询策略
   */
  private selectRoundRobin(connections: Connection[]): Connection {
    const connection = connections[this.roundRobinIndex % connections.length];
    this.roundRobinIndex++;
    return connection;
  }

  /**
   * 最少连接数策略
   */
  private selectLeastConnections(connections: Connection[]): Connection {
    return connections.reduce((min, current) => {
      const minStats = min.getStats();
      const currentStats = current.getStats();
      return currentStats.queryCount < minStats.queryCount ? current : min;
    });
  }

  /**
   * 最少响应时间策略
   */
  private selectLeastResponseTime(connections: Connection[]): Connection {
    return connections.reduce((min, current) => {
      const minStats = min.getStats();
      const currentStats = current.getStats();
      
      const minAvgTime = minStats.queryCount > 0 ? minStats.totalQueryTime / minStats.queryCount : Infinity;
      const currentAvgTime = currentStats.queryCount > 0 ? currentStats.totalQueryTime / currentStats.queryCount : Infinity;
      
      return currentAvgTime < minAvgTime ? current : min;
    });
  }

  /**
   * 加权轮询策略
   */
  private selectWeightedRoundRobin(connections: Connection[]): Connection {
    // 确保所有连接都有权重信息
    connections.forEach(conn => this.updateConnectionWeight(conn));

    // 计算总权重
    const totalWeight = connections.reduce((sum, conn) => {
      const weight = this.connectionWeights.get(conn.getId())?.weight || 1;
      return sum + weight;
    }, 0);

    if (totalWeight === 0) {
      return connections[0];
    }

    // 随机选择一个权重值
    const randomWeight = Math.random() * totalWeight;
    let currentWeight = 0;

    // 根据权重选择连接
    for (const connection of connections) {
      const weight = this.connectionWeights.get(connection.getId())?.weight || 1;
      currentWeight += weight;
      
      if (randomWeight <= currentWeight) {
        return connection;
      }
    }

    return connections[0];
  }

  /**
   * 随机策略
   */
  private selectRandom(connections: Connection[]): Connection {
    const randomIndex = Math.floor(Math.random() * connections.length);
    return connections[randomIndex];
  }

  /**
   * 启动权重更新定时器
   */
  private startWeightUpdateTimer(): void {
    this.weightUpdateTimer = setInterval(() => {
      this.updateAllConnectionWeights();
    }, this.config.weightUpdateInterval);

    if (this.weightUpdateTimer.unref) {
      this.weightUpdateTimer.unref();
    }
  }

  /**
   * 重启权重更新定时器
   */
  private restartWeightUpdateTimer(): void {
    if (this.weightUpdateTimer) {
      clearInterval(this.weightUpdateTimer);
    }
    this.startWeightUpdateTimer();
  }

  /**
   * 更新所有连接权重
   */
  private updateAllConnectionWeights(): void {
    // 这个方法会在连接池中调用，传入所有连接
    this.emit('updateWeightsRequested');
  }

  /**
   * 清理资源
   */
  destroy(): void {
    if (this.weightUpdateTimer) {
      clearInterval(this.weightUpdateTimer);
      this.weightUpdateTimer = null;
    }
    
    this.connectionWeights.clear();
    this.removeAllListeners();
  }

  /**
   * 获取连接权重信息
   */
  getConnectionWeights(): Map<string, ConnectionWeight> {
    return new Map(this.connectionWeights);
  }

  /**
   * 获取负载均衡统计信息
   */
  getStats(): any {
    return {
      strategy: this.config.strategy,
      totalConnections: this.connectionWeights.size,
      roundRobinIndex: this.roundRobinIndex,
      averageWeight: this.calculateAverageWeight(),
      config: this.config
    };
  }

  /**
   * 计算平均权重
   */
  private calculateAverageWeight(): number {
    if (this.connectionWeights.size === 0) {
      return 0;
    }

    const totalWeight = Array.from(this.connectionWeights.values())
      .reduce((sum, weight) => sum + weight.weight, 0);
    
    return totalWeight / this.connectionWeights.size;
  }
}