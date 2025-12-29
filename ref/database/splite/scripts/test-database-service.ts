import { SqliteDatabaseService } from '../SqliteDatabaseService';
import fs from 'fs';
import path from 'path';

console.log('开始测试SQLite数据库服务...');

// 删除旧的测试数据库文件
const dbPath = path.join(process.cwd(), 'data', 'code-search-helper.db');
if (fs.existsSync(dbPath)) {
  fs.unlinkSync(dbPath);
  console.log('已删除旧的测试数据库文件');
}

const dbService = new SqliteDatabaseService({} as any); // Mock LoggerService

try {
  // 测试1: 连接数据库
  console.log('1. 测试数据库连接...');
  dbService.connect();
  console.log('   ✓ 数据库连接成功');

  // 测试2: 初始化表结构
  console.log('2. 测试表结构初始化...');
  dbService.initializeTables();
  console.log('   ✓ 表结构初始化成功');

  // 测试3: 插入项目数据
  console.log('3. 测试项目数据插入...');
  const insertProject = dbService.prepare(`
    INSERT INTO projects (
      id, path, name, description, collection_name, space_name,
      created_at, updated_at, last_indexed_at, status, settings, metadata
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `);

  const projectId = 'service-test-project-1';
  const result1 = insertProject.run(
    projectId,
    '/path/to/service/test/project',
    'Service Test Project',
    'A test project for database service',
    'service_collection',
    'service_space',
    new Date().toISOString(),
    new Date().toISOString(),
    new Date().toISOString(),
    'active',
    JSON.stringify({ language: 'typescript', include: ['**/*.ts'] }),
    JSON.stringify({ version: '1.0.0', author: 'service-test' })
  );
  console.log('   ✓ 项目数据插入成功，ID:', result1.lastInsertRowid);

  // 测试4: 插入文件索引状态数据
  console.log('4. 测试文件索引状态数据插入...');
  const insertFileState = dbService.prepare(`
    INSERT INTO file_index_states (
      project_id, file_path, relative_path, content_hash, file_size,
      last_modified, last_indexed, chunk_count, vector_count, language, 
      file_type, status, error_message, metadata, created_at, updated_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `);

  const result2 = insertFileState.run(
    projectId,
    '/path/to/service/test/project/src/main.ts',
    'src/main.ts',
    'b2c3d4e5f67890123456789012345678901234567890123456789012345678',
    2048,
    new Date().toISOString(),
    new Date().toISOString(),
    8,
    15,
    'typescript',
    'source',
    'indexed',
    null,
    JSON.stringify({ lines: 200, functions: 8 }),
    new Date().toISOString(),
    new Date().toISOString()
  );
  console.log('   ✓ 文件索引状态数据插入成功，ID:', result2.lastInsertRowid);

  // 测试5: 插入项目状态数据
  console.log('5. 测试项目状态数据插入...');
  const insertProjectStatus = dbService.prepare(`
    INSERT INTO project_status (
      project_id, vector_status, graph_status, indexing_progress,
      total_files, indexed_files, failed_files, last_updated
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
  `);

  const result3 = insertProjectStatus.run(
    projectId,
    JSON.stringify({ indexed: true, count: 15, last_updated: new Date().toISOString() }),
    JSON.stringify({ indexed: true, nodes: 25, edges: 30, last_updated: new Date().toISOString() }),
    50.0,
    10,
    5,
    0,
    new Date().toISOString()
  );
  console.log('   ✓ 项目状态数据插入成功，影响行数:', result3.changes);

  // 测试6: 插入文件变更历史数据
  console.log('6. 测试文件变更历史数据插入...');
  const insertChangeHistory = dbService.prepare(`
    INSERT INTO file_change_history (
      file_path, project_id, change_type, previous_hash, current_hash,
      file_size, timestamp, metadata
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
  `);

  const result4 = insertChangeHistory.run(
    '/path/to/service/test/project/src/main.ts',
    projectId,
    'modified',
    'a1b2c3d4e5f67890123456789012345678901234567890123456789012345678',
    'b2c3d4e5f67890123456789012345678901234567890123456789012345678',
    2048,
    new Date().toISOString(),
    JSON.stringify({ reason: 'code update', lines_changed: 10 })
  );
  console.log('   ✓ 文件变更历史数据插入成功，ID:', result4.lastInsertRowid);

  // 测试7: 查询数据
  console.log('7. 测试数据查询...');
  const selectProject = dbService.prepare('SELECT * FROM projects WHERE id = ?');
  const project = selectProject.get(projectId);
  console.log('   ✓ 查询项目成功:', project);

  const selectFileStates = dbService.prepare('SELECT * FROM file_index_states WHERE project_id = ?');
  const fileStates = selectFileStates.all(projectId);
  console.log('   ✓ 查询文件状态成功，返回', fileStates.length, '条记录');

  // 测试8: 事务测试
  console.log('8. 测试事务功能...');
  const transactionResult = dbService.transaction(() => {
    const stmt1 = dbService.prepare('INSERT INTO projects (id, path, name, created_at, updated_at, status) VALUES (?, ?, ?, ?, ?, ?)');
    const stmt2 = dbService.prepare('INSERT INTO project_status (project_id, vector_status, graph_status, last_updated) VALUES (?, ?, ?, ?)');

    stmt1.run('transaction-test-project', '/path/to/transaction/test', 'Transaction Test Project', new Date().toISOString(), new Date().toISOString(), 'active');
    stmt2.run('transaction-test-project', JSON.stringify({}), JSON.stringify({}), new Date().toISOString());

    return '事务执行成功';
  });
  console.log('   ✓', transactionResult);

  // 测试9: 数据库统计信息
  console.log('9. 测试数据库统计信息...');
  const stats = dbService.getStats();
  console.log('   ✓ 数据库统计信息:', stats);

  // 测试10: 数据库备份
  console.log('10. 测试数据库备份...');
  const backupPath = dbService.getDatabasePath().replace('.db', '-backup.db');
  dbService.backup(backupPath);
  console.log('   ✓ 数据库备份成功');

  console.log('\nSQLite数据库服务测试完成！所有功能均正常。');

} catch (error) {
  console.error('SQLite数据库服务测试失败:', error);
} finally {
  // 关闭数据库连接
  dbService.close();
  console.log('数据库连接已关闭');
}