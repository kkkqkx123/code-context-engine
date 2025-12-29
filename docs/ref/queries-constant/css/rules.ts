/*
CSS Rule-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; Media queries - important for responsive design
(media_statement) @definition.media

; Keyframe animations - important for animations
(keyframes_statement
  (keyframes_name) @name.definition.keyframe) @definition.keyframe

; Import statements - important for module organization
(import_statement) @definition.import

; Font face rules - important for custom fonts
(at_rule
  (at_keyword) @_at_keyword
  (#eq? @_at_keyword "@font-face")) @definition.font_face

; Supports statements - important for feature detection
(supports_statement) @definition.supports

; Namespace statements - important for XML namespaces
(namespace_statement) @definition.namespace

; At-rules - important for special CSS features
(at_rule
  (at_keyword) @name.definition.at_rule) @definition.at_rule
`;