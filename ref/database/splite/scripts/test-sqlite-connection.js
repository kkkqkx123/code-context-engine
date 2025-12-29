const Database = require('better-sqlite3');
const path = require('path');
const fs = require('fs');

// 测试SQLite连接
function testSqliteConnection() {
  try {
    const dbPath = path.join(process.cwd(), 'data', 'code-search-helper.db');
    
    // 确保数据目录存在
    const dataDir = path.dirname(dbPath);
    if (!fs.existsSync(dataDir)) {
      fs.mkdirSync(dataDir, { recursive: true });
    }
    
    console.log('Testing SQLite connection to:', dbPath);
    
    // 连接数据库
    const db = new Database(dbPath);
    console.log('SQLite database connected successfully');
    
    // 设置pragma
    db.pragma('journal_mode = WAL');
    db.pragma('foreign_keys = ON');
    console.log('SQLite pragmas set successfully');
    
    // 测试基本查询
    const result = db.prepare('SELECT 1 as test').get();
    console.log('Test query result:', result);
    
    // 关闭连接
    db.close();
    console.log('SQLite database closed successfully');
    
    return true;
  } catch (error) {
    console.error('SQLite connection test failed:', error.message);
    return false;
  }
}

// 运行测试
const success = testSqliteConnection();
console.log('SQLite connection test result:', success ? 'SUCCESS' : 'FAILED');
process.exit(success ? 0 : 1);