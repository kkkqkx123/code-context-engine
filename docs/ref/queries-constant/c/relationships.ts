/*
C Relationships Tree-Sitter Query Patterns
Index for all C language relationship queries
*/

// 导入关系查询
import CALL_QUERY from './relationships/call';
import DEPENDENCY_QUERY from './relationships/dependency';

// 定义查询映射
export const C_RELATIONSHIPS_QUERIES = {
  // 关系查询
  'relationships/call': CALL_QUERY,
  'relationships/dependency': DEPENDENCY_QUERY,
};

// 默认导出，保持与原有结构的兼容性
export default C_RELATIONSHIPS_QUERIES;