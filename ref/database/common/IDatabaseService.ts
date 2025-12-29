import { EventListener } from '../../types';
import { Subscription } from './DatabaseEventTypes';

/**
 * 统一数据库服务接口
 * 定义所有数据库服务必须实现的基础方法
 */
export interface IDatabaseService {
  // 基础连接操作
  initialize(): Promise<boolean>;
  isConnected(): boolean;
  close(): Promise<void>;

  // 项目相关操作
  createProjectSpace(projectPath: string, config?: any): Promise<boolean>;
  deleteProjectSpace(projectPath: string): Promise<boolean>;
  getProjectSpaceInfo(projectPath: string): Promise<any>;

  // 数据操作
  insertData(projectPath: string, data: any): Promise<boolean>;
  updateData(projectPath: string, id: string, data: any): Promise<boolean>;
  deleteData(projectPath: string, id: string): Promise<boolean>;

  // 查询操作
  searchData(projectPath: string, query: any): Promise<any[]>;
  getDataById(projectPath: string, id: string): Promise<any>;

  // 事件系统
  subscribe(eventType: string, listener: EventListener): Subscription;

  // 健康检查
  healthCheck(): Promise<{
    status: 'healthy' | 'unhealthy';
    details?: any;
    error?: string;
  }>;
}

/**
 * 数据库连接管理器接口
 */
export interface IConnectionManager {
  // 连接操作
  connect(): Promise<boolean>;
  disconnect(): Promise<void>;
  isConnected(): boolean;

  // 配置管理
  getConfig(): any;
  updateConfig(config: any): void;

  // 连接状态
  getConnectionStatus(): any;

  // 事件系统
  subscribe(eventType: string, listener: EventListener): Subscription;
}

/**
 * 项目管理器接口
 */
export interface IProjectManager {
  // 项目空间操作
  createProjectSpace(projectPath: string, config?: any): Promise<boolean>;
  deleteProjectSpace(projectPath: string): Promise<boolean>;
  getProjectSpaceInfo(projectPath: string): Promise<any>;
  clearProjectSpace(projectPath: string): Promise<boolean>;
  listProjectSpaces(): Promise<any[]>;

  // 项目数据操作
  insertProjectData(projectPath: string, data: any): Promise<boolean>;
  updateProjectData(projectPath: string, id: string, data: any): Promise<boolean>;
  deleteProjectData(projectPath: string, id: string): Promise<boolean>;

  // 项目数据查询
  searchProjectData(projectPath: string, query: any): Promise<any[]>;
  getProjectDataById(projectPath: string, id: string): Promise<any>;

  // 事件系统
  subscribe(eventType: string, listener: EventListener): Subscription;
}