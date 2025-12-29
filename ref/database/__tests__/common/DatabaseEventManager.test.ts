import { DatabaseEventManager } from '../../../database/common/DatabaseEventManager';
import {
  DatabaseEvent,
  DatabaseEventType,
  QdrantEventType,
  NebulaEventType
} from '../../../database/common/DatabaseEventTypes';
import { DatabaseEventListener } from '../../../database/common/DatabaseEventInterfaces';

interface TestEvents {
  'test.event': { value: number };
  'another.event': { message: string };
}

describe('DatabaseEventManager', () => {
  let eventManager: DatabaseEventManager<TestEvents>;

  beforeEach(() => {
    eventManager = new DatabaseEventManager<TestEvents>();
    jest.clearAllMocks();
  });

  describe('subscribe', () => {
    it('should subscribe event listener for specific event type', () => {
      const listener: DatabaseEventListener<{ value: number }> = jest.fn();

      const subscription = eventManager.subscribe('test.event', listener);

      // 触发事件
      const event: DatabaseEvent = {
        type: DatabaseEventType.SERVICE_INITIALIZED,
        timestamp: new Date(),
        source: 'common',
        data: { value: 42 }
      };

      eventManager.emitEvent(event);

      // 注意：由于类型不匹配，监听器不会被调用
      // 这个测试主要是确保方法可以正常调用
      expect(eventManager.getListenerCount('test.event')).toBe(1);
      expect(subscription.unsubscribe).toBeDefined();
    });

    it('should add multiple listeners for the same event type', () => {
      const listener1: DatabaseEventListener<any> = jest.fn();
      const listener2: DatabaseEventListener<any> = jest.fn();

      eventManager.subscribe('test.event', listener1);
      eventManager.subscribe('test.event', listener2);

      expect(eventManager.getListenerCount('test.event')).toBe(2);
    });
  });

  describe('unsubscribe', () => {
    it('should remove specific event listener via unsubscribe', () => {
      const listener: DatabaseEventListener<any> = jest.fn();

      const subscription = eventManager.subscribe('test.event', listener);
      expect(eventManager.getListenerCount('test.event')).toBe(1);

      subscription.unsubscribe();
      expect(eventManager.getListenerCount('test.event')).toBe(0);
    });

    it('should handle unsubscribe when listener does not exist', () => {
      const listener1: DatabaseEventListener<any> = jest.fn();
      const listener2: DatabaseEventListener<any> = jest.fn();

      const subscription1 = eventManager.subscribe('test.event', listener1);
      const subscription2 = eventManager.subscribe('test.event', listener2);

      subscription2.unsubscribe(); // Remove listener2
      expect(eventManager.getListenerCount('test.event')).toBe(1);

      subscription1.unsubscribe(); // Remove listener1
      expect(eventManager.getListenerCount('test.event')).toBe(0);
    });

    it('should remove event type entry when no listeners remain', () => {
      const listener: DatabaseEventListener<any> = jest.fn();

      const subscription = eventManager.subscribe('test.event', listener);
      subscription.unsubscribe();

      expect(eventManager.getListenerCount('test.event')).toBe(0);
    });
  });

  describe('emitEvent', () => {
    it('should emit event to specific listeners', () => {
      const listener: DatabaseEventListener<DatabaseEvent> = jest.fn();
      const consoleSpy = jest.spyOn(console, 'error').mockImplementation();
      
      eventManager.subscribe('service_initialized' as any, listener);
      
      const event: DatabaseEvent = {
        type: DatabaseEventType.SERVICE_INITIALIZED,
        timestamp: new Date(),
        source: 'common'
      };
      
      eventManager.emitEvent(event);
      
      expect(listener).toHaveBeenCalledWith(event);
      expect(consoleSpy).not.toHaveBeenCalled();
      
      consoleSpy.mockRestore();
    });

    it('should emit event to general listeners', () => {
      const listener: DatabaseEventListener<DatabaseEvent> = jest.fn();
      
      eventManager.subscribe('*' as any, listener);
      
      const event: DatabaseEvent = {
        type: DatabaseEventType.SERVICE_INITIALIZED,
        timestamp: new Date(),
        source: 'common'
      };
      
      eventManager.emitEvent(event);
      
      expect(listener).toHaveBeenCalledWith(event);
    });

    it('should emit error events to error listeners', () => {
      const errorListener: DatabaseEventListener<DatabaseEvent> = jest.fn();
      
      eventManager.subscribe('error_occurred' as any, errorListener);
      
      const event: DatabaseEvent = {
        type: DatabaseEventType.SERVICE_INITIALIZED,
        timestamp: new Date(),
        source: 'common',
        error: new Error('Test error')
      };
      
      eventManager.emitEvent(event);
      
      expect(errorListener).toHaveBeenCalledWith(event);
    });

    it('should handle errors in event listeners', () => {
      const failingListener: DatabaseEventListener<DatabaseEvent> = () => {
        throw new Error('Listener error');
      };
      const consoleSpy = jest.spyOn(console, 'error').mockImplementation();
      
      eventManager.subscribe('service_initialized' as any, failingListener);
      
      const event: DatabaseEvent = {
        type: DatabaseEventType.SERVICE_INITIALIZED,
        timestamp: new Date(),
        source: 'common'
      };
      
      eventManager.emitEvent(event);
      
      expect(consoleSpy).toHaveBeenCalledWith(
        'Error in event listener for service_initialized:',
        expect.any(Error)
      );
      
      consoleSpy.mockRestore();
    });
  });

  describe('removeAllListeners', () => {
    it('should remove all listeners', () => {
      const listener1: DatabaseEventListener<any> = jest.fn();
      const listener2: DatabaseEventListener<any> = jest.fn();

      eventManager.subscribe('test.event', listener1);
      eventManager.subscribe('another.event', listener2);
      
      expect(eventManager.getListenerCount()).toBe(2);
      
      eventManager.removeAllListeners();
      
      expect(eventManager.getListenerCount()).toBe(0);
    });
  });

  describe('getListenerCount', () => {
    it('should return count of all listeners when no event type specified', () => {
      const listener1: DatabaseEventListener<any> = jest.fn();
      const listener2: DatabaseEventListener<any> = jest.fn();

      eventManager.subscribe('test.event', listener1);
      eventManager.subscribe('another.event', listener2);
      
      expect(eventManager.getListenerCount()).toBe(2);
    });

    it('should return count of listeners for specific event type', () => {
      const listener1: DatabaseEventListener<any> = jest.fn();
      const listener2: DatabaseEventListener<any> = jest.fn();

      eventManager.subscribe('test.event', listener1);
      eventManager.subscribe('test.event', listener2);
      eventManager.subscribe('another.event', jest.fn());
      
      expect(eventManager.getListenerCount('test.event')).toBe(2);
      expect(eventManager.getListenerCount('another.event')).toBe(1);
    });
  });

  describe('eventHistory', () => {
    it('should record events in history', () => {
      const event: DatabaseEvent = {
        type: DatabaseEventType.SERVICE_INITIALIZED,
        timestamp: new Date(),
        source: 'common'
      };
      
      eventManager.emitEvent(event);
      
      const history = eventManager.getEventHistory();
      expect(history).toHaveLength(1);
      expect(history[0]).toEqual(event);
    });

    it('should limit event history to max size', () => {
      eventManager.setMaxEventHistory(2);
      
      for (let i = 0; i < 5; i++) {
        const event: DatabaseEvent = {
          type: DatabaseEventType.SERVICE_INITIALIZED,
          timestamp: new Date(),
          source: 'common',
          data: { index: i }
        };
        eventManager.emitEvent(event);
      }
      
      const history = eventManager.getEventHistory();
      expect(history).toHaveLength(2);
      expect(history[0].data).toEqual({ index: 3 });
      expect(history[1].data).toEqual({ index: 4 });
    });

    it('should clear event history', () => {
      const event: DatabaseEvent = {
        type: DatabaseEventType.SERVICE_INITIALIZED,
        timestamp: new Date(),
        source: 'common'
      };
      
      eventManager.emitEvent(event);
      expect(eventManager.getEventHistory()).toHaveLength(1);
      
      eventManager.clearEventHistory();
      expect(eventManager.getEventHistory()).toHaveLength(0);
    });
  });

  describe('setMaxEventHistory', () => {
    it('should set maximum event history size', () => {
      eventManager.setMaxEventHistory(5);
      expect((eventManager as any).maxEventHistory).toBe(5);
    });

    it('should truncate existing history when reducing max size', () => {
      // 添加5个事件
      for (let i = 0; i < 5; i++) {
        const event: DatabaseEvent = {
          type: DatabaseEventType.SERVICE_INITIALIZED,
          timestamp: new Date(),
          source: 'common'
        };
        eventManager.emitEvent(event);
      }
      
      expect(eventManager.getEventHistory()).toHaveLength(5);
      
      // 减少历史记录大小到2
      eventManager.setMaxEventHistory(2);
      expect(eventManager.getEventHistory()).toHaveLength(2);
    });

    it('should not allow negative max history size', () => {
      eventManager.setMaxEventHistory(-1);
      expect((eventManager as any).maxEventHistory).toBe(1);
    });
  });

  describe('static methods', () => {
    describe('createEvent', () => {
      it('should create a new event', () => {
        const event = DatabaseEventManager.createEvent(
          DatabaseEventType.SERVICE_INITIALIZED,
          'qdrant',
          { test: 'data' },
          undefined,
          { operation: 'test' }
        );
        
        expect(event.type).toBe(DatabaseEventType.SERVICE_INITIALIZED);
        expect(event.source).toBe('qdrant');
        expect(event.data).toEqual({ test: 'data' });
        expect(event.metadata).toEqual({ operation: 'test' });
        expect(event.timestamp).toBeInstanceOf(Date);
      });
    });

    describe('createPerformanceEvent', () => {
      it('should create a performance event', () => {
        const event = DatabaseEventManager.createPerformanceEvent(
          'search',
          100,
          true,
          'qdrant',
          1024
        );
        
        expect(event.type).toBe(DatabaseEventType.PERFORMANCE_METRIC);
        expect(event.source).toBe('qdrant');
        expect(event.data).toEqual({
          operation: 'search',
          duration: 100,
          success: true,
          dataSize: 1024,
          timestamp: expect.any(Date)
        });
      });
    });

    describe('createQueryEvent', () => {
      it('should create a query event', () => {
        const event = DatabaseEventManager.createQueryEvent(
          'SELECT * FROM test',
          50,
          true,
          'nebula',
          { param: 'value' },
          10,
          undefined
        );
        
        expect(event.type).toBe(DatabaseEventType.QUERY_EXECUTED);
        expect(event.source).toBe('nebula');
        expect(event.data).toEqual({
          query: 'SELECT * FROM test',
          parameters: { param: 'value' },
          executionTime: 50,
          resultCount: 10,
          success: true
        });
      });
    });

    describe('createBatchOperationEvent', () => {
      it('should create a batch operation event', () => {
        const event = DatabaseEventManager.createBatchOperationEvent(
          'insert',
          100,
          true,
          200,
          'common',
          5
        );
        
        expect(event.type).toBe(DatabaseEventType.BATCH_OPERATION_COMPLETED);
        expect(event.source).toBe('common');
        expect(event.data).toEqual({
          operationType: 'insert',
          batchSize: 100,
          success: true,
          duration: 200,
          failedItems: 5
        });
      });
    });
  });
});