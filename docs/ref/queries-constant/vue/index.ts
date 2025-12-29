/*
Vue Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
import components from './components';
import templateDirectives from './template-directives';

const comments = `
; 注释查询
(comment) @comment
`;

export default `
${components}
${templateDirectives}
${comments}
`;