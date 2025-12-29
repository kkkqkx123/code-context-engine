import { NebulaDataBatchProcessor } from '../NebulaDataBatchProcessor';
import { INebulaQueryService } from '../../query/NebulaQueryService';
import { INebulaDataOperations } from '../../operation/NebulaDataOperations';
import { NebulaNode, NebulaRelationship } from '../../NebulaTypes';
import { DatabaseLoggerService } from '../../../common/DatabaseLoggerService';
import { ErrorHandlerService } from '../../../../utils/ErrorHandlerService';
import { NebulaDataService } from '../../data/NebulaDataService';
import { IBatchProcessingService } from '../../../../infrastructure/batching/types';
import { BatchResult } from '../../../../infrastructure/batching/types';
import { DatabaseType } from '../../../../infrastructure/types';

describe('NebulaDataBatchProcessor', () => {
  let processor: NebulaDataBatchProcessor;
  let mockBatchService: jest.Mocked<IBatchProcessingService>;
  let mockQueryService: jest.Mocked<INebulaQueryService>;
  let mockDataOperations: jest.Mocked<INebulaDataOperations>;
  let mockDatabaseLogger: jest.Mocked<DatabaseLoggerService>;
  let mockErrorHandler: jest.Mocked<ErrorHandlerService>;
  let mockDataService: jest.Mocked<NebulaDataService>;

  beforeEach(() => {
    mockBatchService = {
      executeBatch: jest.fn(),
      executeDatabaseBatch: jest.fn(),
      executeEmbeddingBatch: jest.fn(),
      executeSimilarityBatch: jest.fn(),
      executeHotReloadBatch: jest.fn(),
      executeBatchWithRetry: jest.fn(),
      executeBatchWithMonitoring: jest.fn(),
      executeWithRetry: jest.fn(),
      executeWithMonitoring: jest.fn(),
      updateConfig: jest.fn(),
      getPerformanceStats: jest.fn(),
      getCurrentBatchSize: jest.fn(),
      optimizeMemory: jest.fn(),
      optimizeBatchSize: jest.fn(),
      resetPerformanceStats: jest.fn(),
    } as any;

    mockQueryService = {
      executeQuery: jest.fn(),
    } as any;

    mockDataOperations = {
      insertNodes: jest.fn(),
      insertRelationships: jest.fn(),
    } as any;

    mockDatabaseLogger = {
      logDatabaseEvent: jest.fn(),
    } as any;

    mockErrorHandler = {
      handleError: jest.fn(),
    } as any;

    mockDataService = {} as any;

    processor = new NebulaDataBatchProcessor(
      mockDatabaseLogger,
      mockErrorHandler,
      mockQueryService,
      mockDataOperations,
      mockDataService,
      mockBatchService
    );
  });

  describe('insertNodes', () => {
    it('should use batch processing service for node insertion', async () => {
      // Arrange
      const nodes: NebulaNode[] = [
        {
          id: 'node1',
          label: 'TestNode',
          properties: { name: 'test', projectId: 'project1' }
        }
      ];

      const mockBatchResult: BatchResult = {
        totalOperations: 1,
        successfulOperations: 1,
        failedOperations: 0,
        totalDuration: 100,
        results: [{ success: true }]
      };

      mockBatchService.executeDatabaseBatch.mockResolvedValue(mockBatchResult.results);

      // Act
      const result = await processor.insertNodes(nodes);

      // Assert
      expect(result).toBe(true);
      expect(mockBatchService.executeDatabaseBatch).toHaveBeenCalledWith(
        nodes,
        DatabaseType.NEBULA,
        expect.objectContaining({
          databaseType: DatabaseType.NEBULA,
          context: expect.objectContaining({
            domain: 'database',
            subType: 'nebula',
            metadata: expect.objectContaining({
              operationType: 'insert',
              nodeCount: 1
            })
          }),
          operationType: 'write',
          enableRetry: true,
          enableMonitoring: true
        })
      );
    });

    it('should handle empty nodes array', async () => {
      // Act
      const result = await processor.insertNodes([]);

      // Assert
      expect(result).toBe(true);
      expect(mockBatchService.executeDatabaseBatch).not.toHaveBeenCalled();
    });
  });

  describe('insertRelationships', () => {
    it('should use batch processing service for relationship insertion', async () => {
      // Arrange
      const relationships: NebulaRelationship[] = [
        {
          id: 'rel1',
          type: 'TestRelation',
          sourceId: 'node1',
          targetId: 'node2',
          properties: { projectId: 'project1' }
        }
      ];

      const mockBatchResult: BatchResult = {
        totalOperations: 1,
        successfulOperations: 1,
        failedOperations: 0,
        totalDuration: 100,
        results: [{ success: true }]
      };

      mockBatchService.executeDatabaseBatch.mockResolvedValue(mockBatchResult.results);

      // Act
      const result = await processor.insertRelationships(relationships);

      // Assert
      expect(result).toBe(true);
      expect(mockBatchService.executeDatabaseBatch).toHaveBeenCalledWith(
        relationships,
        DatabaseType.NEBULA,
        expect.objectContaining({
          databaseType: DatabaseType.NEBULA,
          context: expect.objectContaining({
            domain: 'database',
            subType: 'nebula',
            metadata: expect.objectContaining({
              operationType: 'insert',
              relationshipCount: 1
            })
          }),
          operationType: 'write',
          enableRetry: true,
          enableMonitoring: true
        })
      );
    });
  });

  describe('assessGraphComplexity', () => {
    it('should assess high complexity correctly', () => {
      const nodes: NebulaNode[] = Array(15000).fill(null).map((_, i) => ({
        id: `node${i}`,
        label: 'TestNode',
        properties: { prop1: 'value1', prop2: 'value2' }
      }));

      const complexity = (processor as any).assessGraphComplexity(nodes);
      expect(complexity).toBe('high');
    });

    it('should assess medium complexity correctly', () => {
      const nodes: NebulaNode[] = Array(5000).fill(null).map((_, i) => ({
        id: `node${i}`,
        label: 'TestNode',
        properties: { prop1: 'value1' }
      }));

      const complexity = (processor as any).assessGraphComplexity(nodes);
      expect(complexity).toBe('medium');
    });

    it('should assess low complexity correctly', () => {
      const nodes: NebulaNode[] = Array(100).fill(null).map((_, i) => ({
        id: `node${i}`,
        label: 'TestNode',
        properties: { prop1: 'value1' }
      }));

      const complexity = (processor as any).assessGraphComplexity(nodes);
      expect(complexity).toBe('low');
    });
  });
});