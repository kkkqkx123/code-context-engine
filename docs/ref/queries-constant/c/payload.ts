/*
C Payload Tree-Sitter Query Patterns
Index for all C language payload queries
*/

// 导入有效载荷查询
import CONCURRENCY_QUERY from './descriptions/concurrency';
import CONTROL_FLOW_QUERY from './descriptions/control-flow';
import LIFECYCLE_QUERY from './descriptions/lifecycle';
import SEMANTIC_QUERY from './descriptions/semantic';

// 定义查询映射
export const C_PAYLOAD_QUERIES = {
  // 有效载荷查询
  'payload/concurrency': CONCURRENCY_QUERY,
  'payload/control-flow': CONTROL_FLOW_QUERY,
  'payload/lifecycle': LIFECYCLE_QUERY,
  'payload/semantic': SEMANTIC_QUERY,
};

// 默认导出，保持与原有结构的兼容性
export default C_PAYLOAD_QUERIES;