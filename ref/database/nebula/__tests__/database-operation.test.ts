import { Container } from 'inversify';
import { TYPES } from '../../../types';
import { LoggerService } from '../../../utils/LoggerService';
import { ErrorHandlerService } from '../../../utils/ErrorHandlerService';
import { NebulaConfigService } from '../../../config/service/NebulaConfigService';
import { PerformanceMonitor } from '../../../infrastructure/monitoring/PerformanceMonitor';
import { NebulaClient } from '../client/NebulaClient';
import { ConnectionPool } from '../connection/ConnectionPool';
import { SessionManager } from '../session/SessionManager';
import { QueryRunner } from '../query/QueryRunner';
import { QueryCache } from '../query/QueryCache';
import { ConnectionWarmer } from '../connection/ConnectionWarmer';
import { LoadBalancer } from '../connection/LoadBalancer';
import { QueryPipeline } from '../query/QueryPipeline';
import { ParallelQueryExecutor } from '../query/ParallelQueryExecutor';
import { MemoryOptimizer } from '../memory/MemoryOptimizer';
import { NebulaConfig } from '../NebulaTypes';
import { ProjectMappingService } from '../../../database/ProjectMappingService';
import { SqliteDatabaseService } from '../../../database/splite/SqliteDatabaseService';
import { InfrastructureConfigService } from '../../../infrastructure/config/InfrastructureConfigService';
import { ConfigService } from '../../../config/ConfigService';
import { BatchProcessingService } from '../../../infrastructure/batching/BatchProcessingService';
import { ExponentialBackoffRetryStrategy } from '../retry/RetryStrategy';
import { CircuitBreaker } from '../circuit-breaker/CircuitBreaker';
import { SemanticBatchStrategy } from '../../../infrastructure/batching/strategies/SemanticBatchStrategy';
import { QdrantBatchStrategy } from '../../../infrastructure/batching/strategies/QdrantBatchStrategy';
import { NebulaBatchStrategy } from '../../../infrastructure/batching/strategies/NebulaBatchStrategy';
import { GraphBatchStrategy } from '../../../infrastructure/batching/strategies/GraphBatchStrategy';
import { EmbeddingBatchStrategy } from '../../../infrastructure/batching/strategies/EmbeddingBatchStrategy';
import { BatchStrategyFactory } from '../../../infrastructure/batching/strategies/BatchStrategyFactory';
import { EmbedderFactory } from '../../../embedders/EmbedderFactory';
import { EmbeddingCacheService } from '../../../embedders/EmbeddingCacheService';

// Mock ConfigService for testing
class MockConfigService {
  getConfig() {
    return {
      nebula: {},
      qdrant: {},
      embedding: {},
      logging: {},
      monitoring: {},
      memoryMonitor: {},
      fileProcessing: {},
      batchProcessing: {},
      project: {},
      indexing: {},
      treeSitter: {},
      projectNaming: {},
      embeddingBatch: {},
      graphCache: {},
    };
  }
}

// Mock MemoryMonitorService for testing
class MockMemoryMonitorService {
  startMonitoring() {}
  stopMonitoring() {}
  getMemoryStatus() {
    return { usedMemoryMB: 0, totalMemoryMB: 0, percentUsed: 0, heapUsed: 0, heapTotal: 0 };
  }
  isMemoryAvailable() {
    return true;
  }
  onMemoryWarning() {}
}

// Note: This test file is designed to work with a real Nebula Graph database
// Make sure you have a Nebula Graph instance running before executing these tests

describe('Nebula Graph Database Operation Tests', () => {
  let container: Container;
  let nebulaClient: NebulaClient;
  let config: NebulaConfig;

  // Use environment variables or default values for database connection
  const DB_HOST = process.env.NEBULA_HOST || 'localhost';
  const DB_PORT = parseInt(process.env.NEBULA_PORT || '9669');
  const DB_USER = process.env.NEBULA_USER || 'root';
  const DB_PASSWORD = process.env.NEBULA_PASSWORD || 'nebula';
  const TEST_SPACE = process.env.NEBULA_TEST_SPACE || 'test_space';

  beforeAll(async () => {
    container = new Container();

    // Setup real services
    container.bind<LoggerService>(TYPES.LoggerService).to(LoggerService).inSingletonScope();
    container.bind<ErrorHandlerService>(TYPES.ErrorHandlerService).to(ErrorHandlerService).inSingletonScope();
    
    // Bind infrastructure and database services
    container.bind<ConfigService>(TYPES.ConfigService).toConstantValue(new MockConfigService() as any);
    container.bind<InfrastructureConfigService>(TYPES.InfrastructureConfigService).to(InfrastructureConfigService).inSingletonScope();
    container.bind<SqliteDatabaseService>(TYPES.SqliteDatabaseService).to(SqliteDatabaseService).inSingletonScope();
    container.bind<ProjectMappingService>(TYPES.UnifiedMappingService).to(ProjectMappingService).inSingletonScope();
    
    container.bind<NebulaConfigService>(TYPES.NebulaConfigService).to(NebulaConfigService).inSingletonScope();
    container.bind<PerformanceMonitor>(TYPES.PerformanceMonitor).to(PerformanceMonitor).inSingletonScope();

    // Bind batch processing and resilience services
    container.bind<MockMemoryMonitorService>(TYPES.MemoryMonitorService).toConstantValue(new MockMemoryMonitorService() as any);
    
    // Bind embedding cache and factory services (required by SemanticBatchStrategy)
    container.bind<EmbeddingCacheService>(TYPES.EmbeddingCacheService).to(EmbeddingCacheService).inSingletonScope();
    container.bind<EmbedderFactory>(TYPES.EmbedderFactory).to(EmbedderFactory).inSingletonScope();
    
    // Bind batch processing strategies
    container.bind<SemanticBatchStrategy>(SemanticBatchStrategy).to(SemanticBatchStrategy).inSingletonScope();
    container.bind<SemanticBatchStrategy>(TYPES.SemanticBatchStrategy).to(SemanticBatchStrategy).inSingletonScope();
    container.bind<QdrantBatchStrategy>(TYPES.QdrantBatchStrategy).to(QdrantBatchStrategy).inSingletonScope();
    container.bind<NebulaBatchStrategy>(TYPES.NebulaBatchStrategy).to(NebulaBatchStrategy).inSingletonScope();
    container.bind<GraphBatchStrategy>(TYPES.GraphBatchStrategy).to(GraphBatchStrategy).inSingletonScope();
    container.bind<EmbeddingBatchStrategy>(TYPES.EmbeddingBatchStrategy).to(EmbeddingBatchStrategy).inSingletonScope();
    container.bind<BatchStrategyFactory>(BatchStrategyFactory).to(BatchStrategyFactory).inSingletonScope();
    
    container.bind<BatchProcessingService>(TYPES.BatchProcessingService).to(BatchProcessingService).inSingletonScope();
    container.bind<ExponentialBackoffRetryStrategy>(TYPES.IRetryStrategy).to(ExponentialBackoffRetryStrategy).inSingletonScope();
    container.bind<CircuitBreaker>(TYPES.ICircuitBreaker).to(CircuitBreaker).inSingletonScope();

    // Bind all our custom services
    container.bind<ConnectionWarmer>(TYPES.ConnectionWarmer).to(ConnectionWarmer).inSingletonScope();
    container.bind<LoadBalancer>(TYPES.LoadBalancer).to(LoadBalancer).inSingletonScope();
    container.bind<QueryCache>(TYPES.QueryCache).to(QueryCache).inSingletonScope();
    container.bind<QueryPipeline>(TYPES.QueryPipeline).to(QueryPipeline).inSingletonScope();
    container.bind<ParallelQueryExecutor>(TYPES.ParallelQueryExecutor).to(ParallelQueryExecutor).inSingletonScope();
    container.bind<MemoryOptimizer>(TYPES.MemoryOptimizer).to(MemoryOptimizer).inSingletonScope();

    container.bind<ConnectionPool>(TYPES.IConnectionPool).to(ConnectionPool).inSingletonScope();
    container.bind<SessionManager>(TYPES.ISessionManager).to(SessionManager).inSingletonScope();
    container.bind<QueryRunner>(TYPES.IQueryRunner).to(QueryRunner).inSingletonScope();
    container.bind<NebulaClient>(TYPES.NebulaClient).to(NebulaClient).inSingletonScope();

    // Get the client
    nebulaClient = container.get<NebulaClient>(TYPES.NebulaClient);

    // Configure connection
    config = {
      host: DB_HOST,
      port: DB_PORT,
      username: DB_USER,
      password: DB_PASSWORD,
      space: TEST_SPACE
    };

    // Initialize the client
    await nebulaClient.initialize(config);

    // Create test space if it doesn't exist
    try {
      await nebulaClient.executeQuery(`DROP SPACE IF EXISTS ${TEST_SPACE}`);
    } catch (error) {
      console.log('Space drop failed (may not exist):', error);
    }

    // Create the test space
    await nebulaClient.executeQuery(`
      CREATE SPACE IF NOT EXISTS ${TEST_SPACE} (
        partition_num = 1,
        replica_factor = 1,
        vid_type = FIXED_STRING(30)
      )
    `);

    // Create test schema
    await nebulaClient.executeQuery(`USE ${TEST_SPACE}`);
    await nebulaClient.executeQuery(`
      CREATE TAG IF NOT EXISTS person (
        name STRING,
        age INT
      )
    `);
    await nebulaClient.executeQuery(`
      CREATE EDGE IF NOT EXISTS friend (
        degree STRING
      )
    `);

    // Wait a bit for schema to be created
    await new Promise(resolve => setTimeout(resolve, 2000));
  }, 30000); // 30 second timeout for initialization

  afterAll(async () => {
    if (nebulaClient) {
      // Clean up test space
      try {
        await nebulaClient.executeQuery(`DROP SPACE IF EXISTS ${TEST_SPACE}`);
      } catch (error) {
        console.log('Space cleanup failed:', error);
      }

      // Close the client
      await nebulaClient.close();
    }
  }, 30000); // 30 second timeout for cleanup

  test('should connect to Nebula Graph and show spaces', async () => {
    const result = await nebulaClient.executeQuery('SHOW SPACES');

    expect(result).toBeDefined();
    expect(result.data).toBeDefined();

    // Verify that our test space exists
    // result.data is an array, not an object with rows property
    const spaces = result.data || [];
    const spaceExists = spaces.some((row: any) =>
      row.some((cell: any) => cell?.value === TEST_SPACE)
    );

    expect(spaceExists).toBe(true);
  }, 10000);

  test('should create and switch to test space', async () => {
    const result = await nebulaClient.executeQuery(`USE ${TEST_SPACE}`);

    expect(result).toBeDefined();
    expect(result.data).toBeDefined();
  }, 10000);

  test('should insert data into the graph', async () => {
    // Insert a person
    const insertResult = await nebulaClient.executeQuery(
      'INSERT VERTEX person(name, age) VALUES "person1":("Alice", 30)'
    );

    expect(insertResult).toBeDefined();

    // Insert another person
    const insertResult2 = await nebulaClient.executeQuery(
      'INSERT VERTEX person(name, age) VALUES "person2":("Bob", 25)'
    );

    expect(insertResult2).toBeDefined();

    // Insert an edge
    const edgeResult = await nebulaClient.executeQuery(
      'INSERT EDGE friend(degree) VALUES "person1"->"person2":("friend")'
    );

    expect(edgeResult).toBeDefined();
  }, 15000);

  test('should query data from the graph', async () => {
    const result = await nebulaClient.executeQuery(
      'MATCH (p:person) RETURN p.person.name AS name, p.person.age AS age'
    );

    expect(result).toBeDefined();
    expect(result.data).toBeDefined();

    const rows = result.data || [];
    expect(rows.length).toBeGreaterThan(0);

    // Check that we have the inserted data
    const names = rows.map((row: any) => row[0]?.value);
    expect(names).toContain('Alice');
    expect(names).toContain('Bob');
  }, 10000);

  test('should update data in the graph', async () => {
    // Update Alice's age
    const updateResult = await nebulaClient.executeQuery(
      'UPDATE VERTEX ON person "person1" SET age = 31'
    );

    expect(updateResult).toBeDefined();

    // Verify the update
    const verifyResult = await nebulaClient.executeQuery(
      'MATCH (p:person {name: "Alice"}) RETURN p.person.age AS age'
    );

    expect(verifyResult).toBeDefined();
    const rows = verifyResult.data || [];
    expect(rows.length).toBeGreaterThan(0);
    expect(rows[0][0]?.value).toBe(31);
  }, 10000);

  test('should delete data from the graph', async () => {
    // Delete Bob
    const deleteResult = await nebulaClient.executeQuery(
      'DELETE VERTEX "person2" WITH EDGE'
    );

    expect(deleteResult).toBeDefined();

    // Verify deletion
    const verifyResult = await nebulaClient.executeQuery(
      'MATCH (p:person {name: "Bob"}) RETURN p'
    );

    expect(verifyResult).toBeDefined();
    const rows = verifyResult.data || [];
    expect(rows.length).toBe(0);
  }, 10000);


  test('should execute batch queries', async () => {
    const queries = [
      { query: 'INSERT VERTEX person(name, age) VALUES "batch1":("Frank", 45)' },
      { query: 'INSERT VERTEX person(name, age) VALUES "batch2":("Grace", 38)' },
      { query: 'INSERT VERTEX person(name, age) VALUES "batch3":("Henry", 52)' }
    ];

    const results = await nebulaClient.executeBatch(queries);

    expect(results).toBeDefined();
    expect(results.length).toBe(3);

    // Verify the data was inserted
    const verifyResult = await nebulaClient.executeQuery(
      'MATCH (p:person) WHERE p.person.name IN ["Frank", "Grace", "Henry"] RETURN p.person.name AS name'
    );

    expect(verifyResult).toBeDefined();
    const rows = verifyResult.data || [];
    expect(rows.length).toBe(3);

    const names = rows.map((row: any) => row[0]?.value);
    expect(names).toContain('Frank');
    expect(names).toContain('Grace');
    expect(names).toContain('Henry');
  }, 15000);

  test('should handle complex queries with parameters', async () => {
    // Insert data with parameters
    const result = await nebulaClient.executeQuery(
      'INSERT VERTEX person(name, age) VALUES "person_param":($name, $age)',
      { name: 'Ivy', age: 29 }
    );

    expect(result).toBeDefined();

    // Query with parameters
    const queryResult = await nebulaClient.executeQuery(
      'MATCH (p:person) WHERE p.person.name == $name RETURN p.person.age AS age',
      { name: 'Ivy' }
    );

    expect(queryResult).toBeDefined();
    const rows = queryResult.data || [];
    expect(rows.length).toBe(1);
    expect(rows[0][0]?.value).toBe(29);
  }, 10000);

  test('should get client statistics', () => {
    const stats = nebulaClient.getStats();

    expect(stats).toBeDefined();
    expect(stats.connectionPool).toBeDefined();
    expect(stats.sessionManager).toBeDefined();
    expect(stats.queryRunner).toBeDefined();
  }, 5000);
});