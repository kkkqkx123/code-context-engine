/*
HTML Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
import elements from './elements';
import attributesContent from './attributes-content';

const comments = `
; 注释查询
(comment) @comment
`;

export default `
${elements}
${attributesContent}
${comments}
`;