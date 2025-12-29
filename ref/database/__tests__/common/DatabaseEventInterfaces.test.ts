import {
  QdrantConnectionEvent,
  QdrantCollectionEvent,
  QdrantVectorEvent,
  QdrantIndexEvent,
  NebulaSpaceEvent,
  NebulaNodeEvent,
  NebulaRelationshipEvent,
  NebulaQueryEvent,
  PerformanceMetricEvent,
  QueryExecutionEvent,
  BatchOperationEvent,
  ProjectIndexEvent,
  ErrorEvent,
  DatabaseEventUnion,
  DatabaseEventTypes,
  DatabaseEventListener,
  isQdrantConnectionEvent,
  isQdrantCollectionEvent,
  isQdrantVectorEvent,
  isNebulaSpaceEvent,
  isNebulaQueryEvent,
  isPerformanceMetricEvent,
  isQueryExecutionEvent,
  isBatchOperationEvent,
  isProjectIndexEvent,
  isErrorEvent
} from '../../../database/common/DatabaseEventInterfaces';
import {
  DatabaseEventType,
  QdrantEventType,
  NebulaEventType
} from '../../../database/common/DatabaseEventTypes';

describe('DatabaseEventInterfaces', () => {
  describe('Type Guards', () => {
    describe('isQdrantConnectionEvent', () => {
      it('should return true for valid QdrantConnectionEvent', () => {
        const event: QdrantConnectionEvent = {
          type: QdrantEventType.COLLECTION_CREATED,
          timestamp: new Date(),
          source: 'qdrant',
          data: {
            host: 'localhost',
            port: 6333,
            status: 'connected'
          }
        };

        expect(isQdrantConnectionEvent(event)).toBe(true);
      });

      it('should return false for invalid QdrantConnectionEvent', () => {
        const invalidEvent = {
          type: QdrantEventType.COLLECTION_CREATED,
          timestamp: new Date(),
          source: 'nebula', // Wrong source
          data: {
            host: 'localhost',
            port: 6333,
            status: 'connected'
          }
        };

        expect(isQdrantConnectionEvent(invalidEvent)).toBe(false);
      });

      it('should return false for missing data', () => {
        const invalidEvent = {
          type: QdrantEventType.COLLECTION_CREATED,
          timestamp: new Date(),
          source: 'qdrant'
          // Missing data field
        };

        expect(isQdrantConnectionEvent(invalidEvent)).toBe(false);
      });
    });

    describe('isQdrantCollectionEvent', () => {
      it('should return true for valid QdrantCollectionEvent', () => {
        const event: QdrantCollectionEvent = {
          type: QdrantEventType.COLLECTION_CREATED,
          timestamp: new Date(),
          source: 'qdrant',
          data: {
            collectionName: 'test_collection',
            vectorSize: 128,
            distance: 'Cosine' as const,
            status: 'success'
          }
        };

        expect(isQdrantCollectionEvent(event)).toBe(true);
      });

      it('should return false for invalid QdrantCollectionEvent', () => {
        const invalidEvent = {
          type: QdrantEventType.COLLECTION_CREATED,
          timestamp: new Date(),
          source: 'qdrant',
          data: {
            // Missing collectionName
            vectorSize: 128,
            distance: 'Cosine',
            status: 'success'
          }
        };

        expect(isQdrantCollectionEvent(invalidEvent)).toBe(false);
      });
    });

    describe('isQdrantVectorEvent', () => {
      it('should return true for valid QdrantVectorEvent', () => {
        const event: QdrantVectorEvent = {
          type: QdrantEventType.VECTOR_INSERTED,
          timestamp: new Date(),
          source: 'qdrant',
          data: {
            collectionName: 'test_collection',
            vectorCount: 100,
            operation: 'insert' as const,
            status: 'success'
          }
        };

        expect(isQdrantVectorEvent(event)).toBe(true);
      });

      it('should return false for invalid QdrantVectorEvent', () => {
        const invalidEvent = {
          type: QdrantEventType.VECTOR_INSERTED,
          timestamp: new Date(),
          source: 'qdrant',
          data: {
            // Missing collectionName
            vectorCount: 100,
            operation: 'insert',
            status: 'success'
          }
        };

        expect(isQdrantVectorEvent(invalidEvent)).toBe(false);
      });
    });

    describe('isNebulaSpaceEvent', () => {
      it('should return true for valid NebulaSpaceEvent', () => {
        const event: NebulaSpaceEvent = {
          type: NebulaEventType.SPACE_CREATED,
          timestamp: new Date(),
          source: 'nebula',
          data: {
            spaceName: 'test_space',
            partitionNum: 10,
            replicaFactor: 3,
            status: 'success'
          }
        };

        expect(isNebulaSpaceEvent(event)).toBe(true);
      });

      it('should return false for invalid NebulaSpaceEvent', () => {
        const invalidEvent = {
          type: NebulaEventType.SPACE_CREATED,
          timestamp: new Date(),
          source: 'nebula',
          data: {
            // Missing spaceName
            partitionNum: 10,
            replicaFactor: 3,
            status: 'success'
          }
        };

        expect(isNebulaSpaceEvent(invalidEvent)).toBe(false);
      });
    });

    describe('isNebulaQueryEvent', () => {
      it('should return true for valid NebulaQueryEvent', () => {
        const event: NebulaQueryEvent = {
          type: NebulaEventType.QUERY_EXECUTED,
          timestamp: new Date(),
          source: 'nebula',
          data: {
            query: 'MATCH (n) RETURN n',
            executionTime: 100,
            status: 'success'
          }
        };

        expect(isNebulaQueryEvent(event)).toBe(true);
      });

      it('should return false for invalid NebulaQueryEvent', () => {
        const invalidEvent = {
          type: NebulaEventType.QUERY_EXECUTED,
          timestamp: new Date(),
          source: 'nebula',
          data: {
            // Missing query
            executionTime: 100,
            status: 'success'
          }
        };

        expect(isNebulaQueryEvent(invalidEvent)).toBe(false);
      });
    });

    describe('isPerformanceMetricEvent', () => {
      it('should return true for valid PerformanceMetricEvent', () => {
        const event: PerformanceMetricEvent = {
          type: DatabaseEventType.PERFORMANCE_METRIC,
          timestamp: new Date(),
          source: 'qdrant',
          data: {
            operation: 'insert',
            duration: 50,
            success: true
          }
        };

        expect(isPerformanceMetricEvent(event)).toBe(true);
      });

      it('should return false for invalid PerformanceMetricEvent', () => {
        const invalidEvent = {
          type: DatabaseEventType.PERFORMANCE_METRIC,
          timestamp: new Date(),
          source: 'qdrant',
          data: {
            // Missing operation
            duration: 50,
            success: true
          }
        };

        expect(isPerformanceMetricEvent(invalidEvent)).toBe(false);
      });
    });

    describe('isQueryExecutionEvent', () => {
      it('should return true for valid QueryExecutionEvent', () => {
        const event: QueryExecutionEvent = {
          type: DatabaseEventType.QUERY_EXECUTED,
          timestamp: new Date(),
          source: 'qdrant',
          data: {
            query: 'SELECT * FROM test',
            executionTime: 50,
            success: true,
            cached: false
          }
        };

        expect(isQueryExecutionEvent(event)).toBe(true);
      });

      it('should return false for invalid QueryExecutionEvent', () => {
        const invalidEvent = {
          type: DatabaseEventType.QUERY_EXECUTED,
          timestamp: new Date(),
          source: 'qdrant',
          data: {
            // Missing query
            executionTime: 50,
            success: true,
            cached: false
          }
        };

        expect(isQueryExecutionEvent(invalidEvent)).toBe(false);
      });
    });

    describe('isBatchOperationEvent', () => {
      it('should return true for valid BatchOperationEvent', () => {
        const event: BatchOperationEvent = {
          type: DatabaseEventType.BATCH_OPERATION_COMPLETED,
          timestamp: new Date(),
          source: 'qdrant',
          data: {
            operationType: 'insert' as const,
            batchSize: 100,
            success: true,
            duration: 200
          }
        };

        expect(isBatchOperationEvent(event)).toBe(true);
      });

      it('should return false for invalid BatchOperationEvent', () => {
        const invalidEvent = {
          type: DatabaseEventType.BATCH_OPERATION_COMPLETED,
          timestamp: new Date(),
          source: 'qdrant',
          data: {
            // Missing operationType
            batchSize: 100,
            success: true,
            duration: 200
          }
        };

        expect(isBatchOperationEvent(invalidEvent)).toBe(false);
      });
    });

    describe('isProjectIndexEvent', () => {
      it('should return true for valid ProjectIndexEvent', () => {
        const event: ProjectIndexEvent = {
          type: DatabaseEventType.SERVICE_INITIALIZED,
          timestamp: new Date(),
          source: 'common',
          data: {
            projectId: 'test-project',
            fileCount: 50,
            indexingTime: 1000,
            status: 'completed' as const
          }
        };

        expect(isProjectIndexEvent(event)).toBe(true);
      });

      it('should return false for invalid ProjectIndexEvent', () => {
        const invalidEvent = {
          type: DatabaseEventType.SERVICE_INITIALIZED,
          timestamp: new Date(),
          source: 'common',
          data: {
            // Missing projectId
            fileCount: 50,
            indexingTime: 1000,
            status: 'completed'
          }
        };

        expect(isProjectIndexEvent(invalidEvent)).toBe(false);
      });
    });

    describe('isErrorEvent', () => {
      it('should return true for valid ErrorEvent', () => {
        const event: ErrorEvent = {
          type: DatabaseEventType.ERROR_OCCURRED,
          timestamp: new Date(),
          source: 'qdrant',
          data: {
            operation: 'insert',
            errorType: 'ValidationError',
            errorMessage: 'Invalid data format'
          }
        };

        expect(isErrorEvent(event)).toBe(true);
      });

      it('should return false for invalid ErrorEvent', () => {
        const invalidEvent = {
          type: DatabaseEventType.ERROR_OCCURRED,
          timestamp: new Date(),
          source: 'qdrant',
          data: {
            // Missing operation
            errorType: 'ValidationError',
            errorMessage: 'Invalid data format'
          }
        };

        expect(isErrorEvent(invalidEvent)).toBe(false);
      });
    });
  });

  describe('Event Interfaces', () => {
    it('should define all event interfaces correctly', () => {
      // 这些测试主要是为了确保接口定义正确，TypeScript编译器会检查类型
      const qdrantConnectionEvent: QdrantConnectionEvent = {
        type: DatabaseEventType.CONNECTION_OPENED,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          host: 'localhost',
          port: 6333,
          status: 'connected'
        }
      };

      const nebulaQueryEvent: NebulaQueryEvent = {
        type: NebulaEventType.QUERY_EXECUTED,
        timestamp: new Date(),
        source: 'nebula',
        data: {
          query: 'MATCH (n) RETURN n',
          executionTime: 100,
          resultCount: 10,
          status: 'success'
        }
      };

      const performanceEvent: PerformanceMetricEvent = {
        type: DatabaseEventType.PERFORMANCE_METRIC,
        timestamp: new Date(),
        source: 'common',
        data: {
          operation: 'search',
          duration: 50,
          success: true,
          dataSize: 1024
        }
      };

      // 确保这些变量被使用，避免未使用变量的警告
      expect(qdrantConnectionEvent).toBeDefined();
      expect(nebulaQueryEvent).toBeDefined();
      expect(performanceEvent).toBeDefined();
    });
  });

  describe('DatabaseEventUnion', () => {
    it('should be a union of all event types', () => {
      // 测试联合类型是否包含所有事件类型
      const event: DatabaseEventUnion = {
        type: DatabaseEventType.CONNECTION_OPENED,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          host: 'localhost',
          port: 6333,
          status: 'connected'
        }
      };

      // 确保事件变量被使用
      expect(event).toBeDefined();
      
      // 验证联合类型可以是任何一种事件类型
      const qdrantEvent: DatabaseEventUnion = {
        type: QdrantEventType.COLLECTION_CREATED,
        timestamp: new Date(),
        source: 'qdrant',
        data: {
          collectionName: 'test',
          vectorSize: 128,
          distance: 'Cosine',
          status: 'success'
        }
      };

      expect(qdrantEvent).toBeDefined();
    });
  });

  describe('DatabaseEventListener', () => {
    it('should define a generic event listener type', () => {
      // 测试事件监听器类型
      const listener: DatabaseEventListener<QdrantConnectionEvent> = (event) => {
        expect(event.source).toBe('qdrant');
      };

      // 确保监听器变量被使用
      expect(listener).toBeDefined();

      // 测试默认的事件监听器类型
      const defaultListener: DatabaseEventListener = (event) => {
        expect(event.timestamp).toBeInstanceOf(Date);
      };

      expect(defaultListener).toBeDefined();
    });
  });
});