/*
CSS Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
import selectors from './selectors';
import properties from './properties';
import rules from './rules';

const comments = `
; 注释查询
(comment) @comment
`;

export default `
${selectors}
${properties}
${comments}
${rules}
`;