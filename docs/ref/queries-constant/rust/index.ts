/*
Rust Language Tree-Sitter Query Patterns
Index for all Rust language queries including shared queries
*/

// 导入实体查询
import FUNCTIONS_QUERY from './entities/functions';
import STRUCTS_QUERY from './entities/structs';
import VARIABLES_QUERY from './entities/variables';
import EXPRESSIONS_QUERY from './entities/expressions';
import MODULES_QUERY from './entities/modules';
import TYPES_QUERY from './entities/types';
import MACROS_QUERY from './entities/macros';

// 导入关系查询
import CONTROL_FLOW_QUERY from './relationships/control-flow';
import DATA_FLOW_QUERY from './relationships/data-flow';
import SEMANTIC_QUERY from './relationships/semantic';
import LIFECYCLE_QUERY from './relationships/lifecycle';
import CONCURRENCY_QUERY from './relationships/concurrency';

// 导入共享查询
import CALL_EXPRESSIONS from './shared/call-expressions';
import FUNCTION_ANNOTATIONS from './shared/function-annotations';

// 定义查询映射
export const RUST_QUERIES = {
  // 实体查询
  'entities/functions': FUNCTIONS_QUERY,
  'entities/structs': STRUCTS_QUERY,
  'entities/variables': VARIABLES_QUERY,
  'entities/expressions': EXPRESSIONS_QUERY,
  'entities/modules': MODULES_QUERY,
  'entities/types': TYPES_QUERY,
  'entities/macros': MACROS_QUERY,

  // 关系查询
  'relationships/control-flow': CONTROL_FLOW_QUERY,
  'relationships/data-flow': DATA_FLOW_QUERY,
  'relationships/semantic': SEMANTIC_QUERY,
  'relationships/lifecycle': LIFECYCLE_QUERY,
  'relationships/concurrency': CONCURRENCY_QUERY,

  // 共享查询
  'shared/call-expressions': CALL_EXPRESSIONS,
  'shared/function-annotations': FUNCTION_ANNOTATIONS
};

// 导出单独的查询常量
export {
  FUNCTIONS_QUERY,
  STRUCTS_QUERY,
  VARIABLES_QUERY,
  EXPRESSIONS_QUERY,
  MODULES_QUERY,
  TYPES_QUERY,
  MACROS_QUERY,
  CONTROL_FLOW_QUERY,
  DATA_FLOW_QUERY,
  SEMANTIC_QUERY,
  LIFECYCLE_QUERY,
  CONCURRENCY_QUERY,
  CALL_EXPRESSIONS,
  FUNCTION_ANNOTATIONS
};