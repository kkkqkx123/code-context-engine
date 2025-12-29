/*
C# Language Tree-Sitter Query Patterns
Index for all C# language queries including shared queries
*/

// 导入实体查询
import CLASSES_QUERY from './entities/classes';
import METHODS_QUERY from './entities/methods';
import PROPERTIES_QUERY from './entities/properties';
import VARIABLES_QUERY from './entities/variables';
import LINQ_QUERY from './entities/linq';
import PATTERNS_QUERY from './entities/patterns';
import EXPRESSIONS_QUERY from './entities/expressions';

// 导入关系查询
import CONTROL_FLOW_QUERY from './relationships/control-flow';
import DATA_FLOW_QUERY from './relationships/data-flow';
import DEPENDENCY_QUERY from './relationships/dependency';
import INHERITANCE_QUERY from './relationships/inheritance';
import CONCURRENCY_QUERY from './relationships/concurrency';
import LIFECYCLE_QUERY from './relationships/lifecycle';
import SEMANTIC_QUERY from './relationships/semantic';

// 导入共享查询
import CALL_EXPRESSIONS from './shared/call-expressions';
import FUNCTION_ANNOTATIONS from './shared/function-annotations';

// 定义查询映射
export const CSHARP_QUERIES = {
  // 实体查询
  'entities/classes': CLASSES_QUERY,
  'entities/methods': METHODS_QUERY,
  'entities/properties': PROPERTIES_QUERY,
  'entities/variables': VARIABLES_QUERY,
  'entities/linq': LINQ_QUERY,
  'entities/patterns': PATTERNS_QUERY,
  'entities/expressions': EXPRESSIONS_QUERY,

  // 关系查询
  'relationships/control-flow': CONTROL_FLOW_QUERY,
  'relationships/data-flow': DATA_FLOW_QUERY,
  'relationships/dependency': DEPENDENCY_QUERY,
  'relationships/inheritance': INHERITANCE_QUERY,
  'relationships/concurrency': CONCURRENCY_QUERY,
  'relationships/lifecycle': LIFECYCLE_QUERY,
  'relationships/semantic': SEMANTIC_QUERY,

  // 共享查询
  'shared/call-expressions': CALL_EXPRESSIONS,
  'shared/function-annotations': FUNCTION_ANNOTATIONS
};

// 导出单独的查询常量
export {
  CLASSES_QUERY,
  METHODS_QUERY,
  PROPERTIES_QUERY,
  VARIABLES_QUERY,
  LINQ_QUERY,
  PATTERNS_QUERY,
  EXPRESSIONS_QUERY,
  CONTROL_FLOW_QUERY,
  DATA_FLOW_QUERY,
  DEPENDENCY_QUERY,
  INHERITANCE_QUERY,
  CONCURRENCY_QUERY,
  LIFECYCLE_QUERY,
  SEMANTIC_QUERY,
  CALL_EXPRESSIONS,
  FUNCTION_ANNOTATIONS
};