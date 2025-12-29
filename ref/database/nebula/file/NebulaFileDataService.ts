import { injectable, inject } from 'inversify';
import { DatabaseLoggerService } from '../../common/DatabaseLoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { TYPES } from '../../../types';
import { INebulaQueryService } from '../query/NebulaQueryService';
import { DatabaseEventType } from '../../common/DatabaseEventTypes';

export interface INebulaFileDataService {
  deleteDataForFile(filePath: string): Promise<void>;
}

@injectable()
export class NebulaFileDataService implements INebulaFileDataService {
  private databaseLogger: DatabaseLoggerService;
  private errorHandler: ErrorHandlerService;
  private queryService: INebulaQueryService;

  constructor(
    @inject(TYPES.DatabaseLoggerService) databaseLogger: DatabaseLoggerService,
    @inject(TYPES.ErrorHandlerService) errorHandler: ErrorHandlerService,
    @inject(TYPES.NebulaQueryService) queryService: INebulaQueryService
  ) {
    this.databaseLogger = databaseLogger;
    this.errorHandler = errorHandler;
    this.queryService = queryService;
  }

  /**
   * 删除文件相关的所有数据
   */
  async deleteDataForFile(filePath: string): Promise<void> {
    try {
      // 删除与文件相关的所有节点和关系
      const deleteQuery = `
        LOOKUP ON File WHERE file_path == "${filePath}" YIELD id AS vid |
        FETCH PROP ON * $-.vid YIELD * |
        GO FROM $-.vid OVER * REVERSELY YIELD dst AS dst |
        DELETE VERTEX $-.vid WITH EDGE
      `;

      await this.queryService.executeQuery(deleteQuery);
      
      // 使用 DatabaseLoggerService 记录事件
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.DATA_DELETED,
        source: 'nebula',
        timestamp: new Date(),
        data: { message: `Successfully deleted data for file: ${filePath}`, filePath }
      });
    } catch (error) {
      // 使用 DatabaseLoggerService 记录错误事件
      await this.databaseLogger.logDatabaseEvent({
        type: DatabaseEventType.SERVICE_ERROR,
        source: 'nebula',
        timestamp: new Date(),
        data: { message: `Failed to delete data for file: ${filePath}`, filePath },
        error: error instanceof Error ? error : new Error(String(error))
      });
      this.errorHandler.handleError(
        error instanceof Error ? error : new Error(String(error)),
        { component: 'NebulaFileDataService', operation: 'deleteDataForFile', filePath }
      );
      throw error;
    }
  }
}