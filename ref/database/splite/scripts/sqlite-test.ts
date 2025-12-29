import Database from 'better-sqlite3';
import path from 'path';
import fs from 'fs';

// 创建一个测试数据库连接
const dbPath = path.join(__dirname, '../../../test-database.db');

// 如果数据库文件已存在，删除它
if (fs.existsSync(dbPath)) {
  fs.unlinkSync(dbPath);
  console.log('已删除旧的测试数据库文件');
}

const db = new Database(dbPath);

console.log('SQLite数据库测试开始...');

try {
  // 测试1: 创建项目表 (projects)
  console.log('1. 创建项目表...');
  db.exec(`
    CREATE TABLE IF NOT EXISTS projects (
        id TEXT PRIMARY KEY,
        path TEXT UNIQUE NOT NULL,
        name TEXT,
        description TEXT,
        collection_name TEXT,
        space_name TEXT,
        created_at DATETIME NOT NULL,
        updated_at DATETIME NOT NULL,
        last_indexed_at DATETIME,
        status TEXT NOT NULL,
        settings JSON,
        metadata JSON
    )
  `);
  console.log('   ✓ 项目表创建成功');

  // 测试2: 创建文件索引状态表 (file_index_states)
  console.log('2. 创建文件索引状态表...');
  db.exec(`
    CREATE TABLE IF NOT EXISTS file_index_states (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        project_id TEXT NOT NULL,
        file_path TEXT NOT NULL,
        relative_path TEXT NOT NULL,
        content_hash TEXT NOT NULL,
        file_size INTEGER,
        last_modified DATETIME NOT NULL,
        last_indexed DATETIME,
        indexing_version INTEGER DEFAULT 1,
        chunk_count INTEGER,
        vector_count INTEGER,
        language TEXT,
        file_type TEXT,
        status TEXT DEFAULT 'pending',
        error_message TEXT,
        metadata JSON,
        created_at DATETIME NOT NULL,
        updated_at DATETIME NOT NULL,
        
        UNIQUE(project_id, file_path),
        FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
    )
  `);
  console.log('   ✓ 文件索引状态表创建成功');

  // 测试3: 创建项目状态表 (project_status)
  console.log('3. 创建项目状态表...');
  db.exec(`
    CREATE TABLE IF NOT EXISTS project_status (
        project_id TEXT PRIMARY KEY,
        vector_status JSON NOT NULL,
        graph_status JSON NOT NULL,
        indexing_progress REAL DEFAULT 0,
        total_files INTEGER DEFAULT 0,
        indexed_files INTEGER DEFAULT 0,
        failed_files INTEGER DEFAULT 0,
        last_updated DATETIME NOT NULL,
        
        FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
    )
  `);
  console.log('   ✓ 项目状态表创建成功');

  // 测试4: 创建文件变更历史表 (file_change_history)
  console.log('4. 创建文件变更历史表...');
  db.exec(`
    CREATE TABLE IF NOT EXISTS file_change_history (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        file_path TEXT NOT NULL,
        project_id TEXT NOT NULL,
        change_type TEXT NOT NULL,
        previous_hash TEXT,
        current_hash TEXT,
        file_size INTEGER,
        timestamp DATETIME NOT NULL,
        metadata JSON,
        
        FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
    )
  `);
  console.log('   ✓ 文件变更历史表创建成功');

  // 测试5: 创建索引
  console.log('5. 创建索引...');
  db.exec('CREATE INDEX IF NOT EXISTS idx_file_states_project ON file_index_states(project_id)');
  db.exec('CREATE INDEX IF NOT EXISTS idx_file_states_hash ON file_index_states(content_hash)');
  db.exec('CREATE INDEX IF NOT EXISTS idx_file_states_path ON file_index_states(file_path)');
  db.exec('CREATE INDEX IF NOT EXISTS idx_file_states_modified ON file_index_states(last_modified)');
  db.exec('CREATE INDEX IF NOT EXISTS idx_change_history_project ON file_change_history(project_id, timestamp)');
  db.exec('CREATE INDEX IF NOT EXISTS idx_change_history_file ON file_change_history(file_path, timestamp)');
  console.log('   ✓ 索引创建成功');

  // 测试6: 插入测试数据到项目表
  console.log('6. 插入测试数据到项目表...');
  const insertProject = db.prepare(`
    INSERT INTO projects (
      id, path, name, description, collection_name, space_name,
      created_at, updated_at, last_indexed_at, status, settings, metadata
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `);

  const projectId = 'test-project-1';
  const result = insertProject.run(
    projectId,
    '/path/to/test/project',
    'Test Project',
    'A test project for SQLite database',
    'test_collection',
    'test_space',
    new Date().toISOString(),
    new Date().toISOString(),
    new Date().toISOString(),
    'active',
    JSON.stringify({ language: 'typescript', include: ['**/*.ts'] }),
    JSON.stringify({ version: '1.0.0', author: 'test' })
  );
  console.log('   ✓ 项目数据插入成功，ID:', result.lastInsertRowid);

  // 测试7: 插入测试数据到文件索引状态表
 console.log('7. 插入测试数据到文件索引状态表...');
  const insertFileState = db.prepare(`
    INSERT INTO file_index_states (
      project_id, file_path, relative_path, content_hash, file_size,
      last_modified, last_indexed, chunk_count, vector_count, language,
      file_type, status, error_message, metadata, created_at, updated_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
  `);

  const fileStateResult = insertFileState.run(
    projectId,
    '/path/to/test/project/src/main.ts',
    'src/main.ts',
    'a1b2c3d4e5f67890123456789012345678901234567890123456789012345678',
    1024,
    new Date().toISOString(),
    new Date().toISOString(),
    5,
    10,
    'typescript',
    'source',
    'indexed',
    null,
    JSON.stringify({ lines: 100, functions: 5 }),
    new Date().toISOString(),
    new Date().toISOString()
  );
  console.log('   ✓ 文件索引状态数据插入成功，ID:', fileStateResult.lastInsertRowid);

  // 测试8: 查询测试数据
  console.log('8. 查询测试数据...');
  const selectProject = db.prepare('SELECT * FROM projects WHERE id = ?');
  const project = selectProject.get(projectId) as any;
  console.log('   ✓ 查询项目成功:', project);

  const selectFileState = db.prepare('SELECT * FROM file_index_states WHERE project_id = ?');
  const fileStates = selectFileState.all(projectId) as any[];
  console.log('   ✓ 查询文件状态成功，返回', fileStates.length, '条记录');

  // 测试9: 更新测试数据
  console.log('9. 更新测试数据...');
  const updateProject = db.prepare('UPDATE projects SET status = ? WHERE id = ?');
  const updateResult = updateProject.run('indexed', projectId);
  console.log('   ✓ 项目状态更新成功，影响行数:', updateResult.changes);

  // 测试10: 复杂查询测试
  console.log('10. 复杂查询测试...');
  const complexQuery = db.prepare(`
    SELECT p.name, p.status, f.relative_path, f.status as file_status
    FROM projects p
    JOIN file_index_states f ON p.id = f.project_id
    WHERE p.status = ?
  `);
  const complexResults = complexQuery.all('indexed') as any[];
  console.log('   ✓ 复杂查询成功，返回', complexResults.length, '条记录');

  // 测试11: 事务测试
  console.log('11. 事务测试...');
  const stmt1 = db.prepare('INSERT INTO projects (id, path, name, created_at, updated_at, status) VALUES (?, ?, ?, ?, ?, ?)');
  const stmt2 = db.prepare('INSERT INTO project_status (project_id, vector_status, graph_status, last_updated) VALUES (?, ?, ?, ?)');
  
  const transaction = db.transaction(() => {
    stmt1.run('transaction-test', '/path/to/transaction/test', 'Transaction Test', new Date().toISOString(), new Date().toISOString(), 'active');
    stmt2.run('transaction-test', JSON.stringify({}), JSON.stringify({}), new Date().toISOString());
  });
  
  transaction();
  console.log('   ✓ 事务执行成功');

  console.log('\nSQLite数据库测试完成！所有操作均正常。');
  
} catch (error) {
  console.error('SQLite数据库测试失败:', error);
} finally {
  // 关闭数据库连接
  db.close();
  console.log('数据库连接已关闭');
}