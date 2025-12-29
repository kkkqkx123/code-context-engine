/*
TSX Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
import typescriptQueries from '../typescript/index';
import components from './components';
import jsx from './jsx';
import typesHooks from './types-hooks';

const comments = `
; 注释查询
(comment) @comment
`;

export default `
${typescriptQueries}
${components}
${jsx}
${typesHooks}
${comments}
`;