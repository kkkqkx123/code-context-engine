// Nebula 数据库配置类型
export interface NebulaConfig {
  host: string;
  port: number;
  username: string;
  password: string;
  timeout?: number;
  maxConnections?: number;
  retryAttempts?: number;
  retryDelay?: number;
  space?: string;
  bufferSize?: number;
  pingInterval?: number;
  vidTypeLength?: number;
}

// Nebula 连接状态类型
export interface NebulaConnectionStatus {
  connected: boolean;
  host: string;
  port: number;
  username: string;
  space?: string;
  lastConnected?: Date;
  error?: string;
}

// 统计信息类型
export interface ResultStats {
  rowCount: number;
  columnCount: number;
  dataSize: number;
  processedTime: number;
}

// Nebula 查询结果类型
export interface NebulaQueryResult {
  table: any;
  results: any;
  rows: any;
  data?: any[];
  error?: string;
  executionTime?: number;
  timeCost?: number;
  space?: string;
  errorCode?: any;
  errorDetails?: any;
  stats?: ResultStats;
  query?: string;
  metadata?: Record<string, any>;
}

// Nebula 错误处理类型
export interface NebulaErrorContext {
  component: string;
  operation: string;
  query?: string;
  parameters?: Record<string, any>;
  retryCount?: number;
  duration?: number;
}

// Nebula 批量顶点类型
export interface BatchVertex {
  tag: string;
  id: string;
  properties: Record<string, any>;
}

// Nebula 批量边类型
export interface BatchEdge {
  type: string;
  srcId: string;
  dstId: string;
  properties: Record<string, any>;
}

// Nebula 空间信息类型
export interface NebulaSpaceInfo {
  name: string;
  partition_num: number;
  replica_factor: number;
  vid_type: string;
  charset?: string;
  collate?: string;
}

// Nebula 节点类型
export interface NebulaNode {
  id: string;
  label: string;
  properties: Record<string, any>;
}

// Nebula 关系类型
export interface NebulaRelationship {
  id?: string;
  type: string;
  sourceId: string;
  targetId: string;
  properties?: Record<string, any>;
}

// Nebula 空间配置类型
export interface NebulaSpaceConfig {
  partitionNum?: number;
  replicaFactor?: number;
  vidType?: string;
  tags?: any[];
  edges?: any[];
}

// 项目空间信息类型
export interface ProjectSpaceInfo {
  projectPath: string;
  spaceName: string;
  spaceInfo: NebulaSpaceInfo;
  createdAt: Date;
  updatedAt: Date;
}