/*
Go Language Tree-Sitter Query Patterns
Index for all Go language queries including shared queries
*/

// 导入实体查询
import FUNCTIONS_QUERY from './entities/functions';
import TYPES_QUERY from './entities/types';
import VARIABLES_QUERY from './entities/variables';
import IMPORTS_QUERY from './entities/imports';
import EXPRESSIONS_QUERY from './entities/expressions';
import INTERFACES_QUERY from './entities/interfaces';
import PATTERNS_QUERY from './entities/patterns';

// 导入关系查询
import CONTROL_FLOW_QUERY from './relationships/control-flow';
import DATA_FLOW_QUERY from './relationships/data-flow';
import SEMANTIC_QUERY from './relationships/semantic';
import LIFECYCLE_QUERY from './relationships/lifecycle';
import CONCURRENCY_QUERY from './relationships/concurrency';
import DEPENDENCY_QUERY from './relationships/dependency';
import INHERITANCE_QUERY from './relationships/inheritance';

// 导入共享查询
import CALL_EXPRESSIONS from './shared/call-expressions';
import FUNCTION_ANNOTATIONS from './shared/function-annotations';

// 定义查询映射
export const GO_QUERIES = {
  // 实体查询
  'entities/functions': FUNCTIONS_QUERY,
  'entities/types': TYPES_QUERY,
  'entities/variables': VARIABLES_QUERY,
  'entities/imports': IMPORTS_QUERY,
  'entities/expressions': EXPRESSIONS_QUERY,
  'entities/interfaces': INTERFACES_QUERY,
  'entities/patterns': PATTERNS_QUERY,

  // 关系查询
  'relationships/control-flow': CONTROL_FLOW_QUERY,
  'relationships/data-flow': DATA_FLOW_QUERY,
  'relationships/semantic': SEMANTIC_QUERY,
  'relationships/lifecycle': LIFECYCLE_QUERY,
  'relationships/concurrency': CONCURRENCY_QUERY,
  'relationships/dependency': DEPENDENCY_QUERY,
  'relationships/inheritance': INHERITANCE_QUERY,

  // 共享查询
  'shared/call-expressions': CALL_EXPRESSIONS,
  'shared/function-annotations': FUNCTION_ANNOTATIONS
};

// 导出单独的查询常量
export {
  FUNCTIONS_QUERY,
  TYPES_QUERY,
  VARIABLES_QUERY,
  IMPORTS_QUERY,
  EXPRESSIONS_QUERY,
  INTERFACES_QUERY,
  PATTERNS_QUERY,
  CONTROL_FLOW_QUERY,
  DATA_FLOW_QUERY,
  SEMANTIC_QUERY,
  LIFECYCLE_QUERY,
  CONCURRENCY_QUERY,
  DEPENDENCY_QUERY,
  INHERITANCE_QUERY,
  CALL_EXPRESSIONS,
  FUNCTION_ANNOTATIONS
};