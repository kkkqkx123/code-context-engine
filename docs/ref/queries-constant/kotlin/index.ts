/*
Kotlin Language Tree-Sitter Query Patterns
Index for all Kotlin language queries including shared queries
*/

// 导入实体查询
import CLASSES_QUERY from './entities/classes';
import FUNCTIONS_QUERY from './entities/functions';
import CONSTRUCTORS_QUERY from './entities/constructors';
import PROPERTIES_QUERY from './entities/properties';

// 导入关系查询
import CONTROL_FLOW_QUERY from './relationships/control-flow';
import DATA_FLOW_QUERY from './relationships/data-flow';
import LIFECYCLE_QUERY from './relationships/lifecycle';
import SEMANTIC_QUERY from './relationships/semantic';
import CONCURRENCY_QUERY from './relationships/concurrency';

// 导入共享查询
import CALL_EXPRESSIONS from './shared/call-expressions';
import FUNCTION_ANNOTATIONS from './shared/function-annotations';
import IMPORTS_EXPORTS from './shared/imports-exports';

// 定义查询映射
export const KOTLIN_QUERIES = {
  // 实体查询
  'entities/classes': CLASSES_QUERY,
  'entities/functions': FUNCTIONS_QUERY,
  'entities/constructors': CONSTRUCTORS_QUERY,
  'entities/properties': PROPERTIES_QUERY,

  // 关系查询
  'relationships/control-flow': CONTROL_FLOW_QUERY,
  'relationships/data-flow': DATA_FLOW_QUERY,
  'relationships/lifecycle': LIFECYCLE_QUERY,
  'relationships/semantic': SEMANTIC_QUERY,
  'relationships/concurrency': CONCURRENCY_QUERY,

  // 共享查询
  'shared/call-expressions': CALL_EXPRESSIONS,
  'shared/function-annotations': FUNCTION_ANNOTATIONS,
  'shared/imports-exports': IMPORTS_EXPORTS
};

// 导出单独的查询常量
export {
  CLASSES_QUERY,
  FUNCTIONS_QUERY,
  CONSTRUCTORS_QUERY,
  PROPERTIES_QUERY,
  CONTROL_FLOW_QUERY,
  DATA_FLOW_QUERY,
  LIFECYCLE_QUERY,
  SEMANTIC_QUERY,
  CONCURRENCY_QUERY,
  CALL_EXPRESSIONS,
  FUNCTION_ANNOTATIONS,
  IMPORTS_EXPORTS
};

// 默认导出合并的查询字符串（保持向后兼容）
export default `
${CLASSES_QUERY}
${FUNCTIONS_QUERY}
${CONSTRUCTORS_QUERY}
${PROPERTIES_QUERY}
${CONTROL_FLOW_QUERY}
${DATA_FLOW_QUERY}
${LIFECYCLE_QUERY}
${SEMANTIC_QUERY}
${CONCURRENCY_QUERY}
${CALL_EXPRESSIONS}
${FUNCTION_ANNOTATIONS}
${IMPORTS_EXPORTS}

; 注释查询
(comment) @comment
`;