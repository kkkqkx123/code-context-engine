/**
 * 统一数据库错误类型定义
 * 为Qdrant和Nebula服务提供一致的错误处理机制
 */

export enum DatabaseErrorType {
  CONNECTION_ERROR = 'CONNECTION_ERROR',
  QUERY_ERROR = 'QUERY_ERROR',
  DATA_OPERATION_ERROR = 'DATA_OPERATION_ERROR',
  CONFIGURATION_ERROR = 'CONFIGURATION_ERROR',
  VALIDATION_ERROR = 'VALIDATION_ERROR',
  TIMEOUT_ERROR = 'TIMEOUT_ERROR',
  AUTHENTICATION_ERROR = 'AUTHENTICATION_ERROR',
  AUTHORIZATION_ERROR = 'AUTHORIZATION_ERROR',
  RESOURCE_NOT_FOUND = 'RESOURCE_NOT_FOUND',
  RESOURCE_EXISTS = 'RESOURCE_EXISTS',
  INTERNAL_ERROR = 'INTERNAL_ERROR',
  UNKNOWN_ERROR = 'UNKNOWN_ERROR'
}

export interface DatabaseErrorContext {
  component: string;
  operation: string;
 details?: any;
  timestamp: Date;
  originalError?: Error;
}

export class DatabaseError extends Error {
  public readonly type: DatabaseErrorType;
  public readonly context: DatabaseErrorContext;
  public readonly originalError?: Error;

  constructor(
    type: DatabaseErrorType,
    message: string,
    context: DatabaseErrorContext,
    originalError?: Error
  ) {
    // 调用父类Error的构造函数
    super(message);

    // 维护原型链
    Object.setPrototypeOf(this, DatabaseError.prototype);

    this.type = type;
    this.context = context;
    this.originalError = originalError;

    // 设置错误名称
    this.name = this.constructor.name;
  }

  /**
   * 创建连接错误
   */
  static connectionError(message: string, context: Omit<DatabaseErrorContext, 'timestamp'>, originalError?: Error): DatabaseError {
    return new DatabaseError(
      DatabaseErrorType.CONNECTION_ERROR,
      message,
      { ...context, timestamp: new Date() },
      originalError
    );
  }

  /**
   * 创建查询错误
   */
  static queryError(message: string, context: Omit<DatabaseErrorContext, 'timestamp'>, originalError?: Error): DatabaseError {
    return new DatabaseError(
      DatabaseErrorType.QUERY_ERROR,
      message,
      { ...context, timestamp: new Date() },
      originalError
    );
  }

  /**
   * 创建数据操作错误
   */
  static dataOperationError(message: string, context: Omit<DatabaseErrorContext, 'timestamp'>, originalError?: Error): DatabaseError {
    return new DatabaseError(
      DatabaseErrorType.DATA_OPERATION_ERROR,
      message,
      { ...context, timestamp: new Date() },
      originalError
    );
  }

  /**
   * 创建配置错误
   */
  static configurationError(message: string, context: Omit<DatabaseErrorContext, 'timestamp'>, originalError?: Error): DatabaseError {
    return new DatabaseError(
      DatabaseErrorType.CONFIGURATION_ERROR,
      message,
      { ...context, timestamp: new Date() },
      originalError
    );
  }

  /**
   * 创建验证错误
   */
  static validationError(message: string, context: Omit<DatabaseErrorContext, 'timestamp'>, originalError?: Error): DatabaseError {
    return new DatabaseError(
      DatabaseErrorType.VALIDATION_ERROR,
      message,
      { ...context, timestamp: new Date() },
      originalError
    );
  }

  /**
   * 创建超时错误
   */
  static timeoutError(message: string, context: Omit<DatabaseErrorContext, 'timestamp'>, originalError?: Error): DatabaseError {
    return new DatabaseError(
      DatabaseErrorType.TIMEOUT_ERROR,
      message,
      { ...context, timestamp: new Date() },
      originalError
    );
  }

  /**
   * 创建认证错误
   */
  static authenticationError(message: string, context: Omit<DatabaseErrorContext, 'timestamp'>, originalError?: Error): DatabaseError {
    return new DatabaseError(
      DatabaseErrorType.AUTHENTICATION_ERROR,
      message,
      { ...context, timestamp: new Date() },
      originalError
    );
 }

  /**
   * 创建授权错误
   */
  static authorizationError(message: string, context: Omit<DatabaseErrorContext, 'timestamp'>, originalError?: Error): DatabaseError {
    return new DatabaseError(
      DatabaseErrorType.AUTHORIZATION_ERROR,
      message,
      { ...context, timestamp: new Date() },
      originalError
    );
  }

  /**
   * 创建资源未找到错误
   */
  static resourceNotFoundError(message: string, context: Omit<DatabaseErrorContext, 'timestamp'>, originalError?: Error): DatabaseError {
    return new DatabaseError(
      DatabaseErrorType.RESOURCE_NOT_FOUND,
      message,
      { ...context, timestamp: new Date() },
      originalError
    );
  }

  /**
   * 创建资源已存在错误
   */
  static resourceExistsError(message: string, context: Omit<DatabaseErrorContext, 'timestamp'>, originalError?: Error): DatabaseError {
    return new DatabaseError(
      DatabaseErrorType.RESOURCE_EXISTS,
      message,
      { ...context, timestamp: new Date() },
      originalError
    );
  }

  /**
   * 创建内部错误
   */
  static internalError(message: string, context: Omit<DatabaseErrorContext, 'timestamp'>, originalError?: Error): DatabaseError {
    return new DatabaseError(
      DatabaseErrorType.INTERNAL_ERROR,
      message,
      { ...context, timestamp: new Date() },
      originalError
    );
  }

  /**
   * 创建未知错误
   */
  static unknownError(message: string, context: Omit<DatabaseErrorContext, 'timestamp'>, originalError?: Error): DatabaseError {
    return new DatabaseError(
      DatabaseErrorType.UNKNOWN_ERROR,
      message,
      { ...context, timestamp: new Date() },
      originalError
    );
  }

  /**
   * 从普通错误创建DatabaseError
   */
  static fromError(error: Error, context: Omit<DatabaseErrorContext, 'timestamp'>): DatabaseError {
    if (error instanceof DatabaseError) {
      return error;
    }

    return new DatabaseError(
      DatabaseErrorType.UNKNOWN_ERROR,
      error.message,
      { ...context, timestamp: new Date() },
      error
    );
  }

  /**
   * 获取错误的详细信息
   */
  getDetails(): {
    type: DatabaseErrorType;
    message: string;
    component: string;
    operation: string;
    details?: any;
    timestamp: Date;
    originalError?: Error;
  } {
    return {
      type: this.type,
      message: this.message,
      component: this.context.component,
      operation: this.context.operation,
      details: this.context.details,
      timestamp: this.context.timestamp,
      originalError: this.originalError
    };
  }
}