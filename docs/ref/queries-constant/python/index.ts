/*
Python Language Tree-Sitter Query Patterns
Index for all Python language queries including shared queries
*/

// 导入实体查询
import CLASSES_QUERY from './entities/classes';
import FUNCTIONS_QUERY from './entities/functions';
import VARIABLES_QUERY from './entities/variables';
import IMPORTS_QUERY from './entities/imports';
import TYPES_DECORATORS_QUERY from './entities/types-decorators';

// 导入关系查询
import CONTROL_FLOW_QUERY from './relationships/control-flow';
import DATA_FLOW_QUERY from './relationships/data-flow';
import SEMANTIC_RELATIONSHIPS_QUERY from './relationships/semantic-relationships';
import LIFECYCLE_RELATIONSHIPS_QUERY from './relationships/lifecycle-relationships';

// 导入共享查询
import DATA_STRUCTURES_QUERY from './shared/data-structures';
import CALL_EXPRESSIONS_QUERY from './shared/call-expressions';
import FUNCTION_ANNOTATIONS_QUERY from './shared/function-annotations';

// 定义查询映射
export const PYTHON_QUERIES = {
  // 实体查询
  'entities/classes': CLASSES_QUERY,
  'entities/functions': FUNCTIONS_QUERY,
  'entities/variables': VARIABLES_QUERY,
  'entities/imports': IMPORTS_QUERY,
  'entities/types-decorators': TYPES_DECORATORS_QUERY,

  // 关系查询
  'relationships/control-flow': CONTROL_FLOW_QUERY,
  'relationships/data-flow': DATA_FLOW_QUERY,
  'relationships/semantic-relationships': SEMANTIC_RELATIONSHIPS_QUERY,
  'relationships/lifecycle-relationships': LIFECYCLE_RELATIONSHIPS_QUERY,

  // 共享查询
  'shared/data-structures': DATA_STRUCTURES_QUERY,
  'shared/call-expressions': CALL_EXPRESSIONS_QUERY,
  'shared/function-annotations': FUNCTION_ANNOTATIONS_QUERY
};

// 导出单独的查询常量
export {
  CLASSES_QUERY,
  FUNCTIONS_QUERY,
  VARIABLES_QUERY,
  IMPORTS_QUERY,
  TYPES_DECORATORS_QUERY,
  CONTROL_FLOW_QUERY,
  DATA_FLOW_QUERY,
  SEMANTIC_RELATIONSHIPS_QUERY,
  LIFECYCLE_RELATIONSHIPS_QUERY,
  DATA_STRUCTURES_QUERY,
  CALL_EXPRESSIONS_QUERY,
  FUNCTION_ANNOTATIONS_QUERY
};

// 默认导出所有查询的组合（向后兼容）
const comments = `
; 注释查询
(comment) @comment
`;

export default `
${CLASSES_QUERY}
${FUNCTIONS_QUERY}
${VARIABLES_QUERY}
${IMPORTS_QUERY}
${TYPES_DECORATORS_QUERY}
${CONTROL_FLOW_QUERY}
${DATA_FLOW_QUERY}
${SEMANTIC_RELATIONSHIPS_QUERY}
${LIFECYCLE_RELATIONSHIPS_QUERY}
${DATA_STRUCTURES_QUERY}
${CALL_EXPRESSIONS_QUERY}
${FUNCTION_ANNOTATIONS_QUERY}
${comments}
`;