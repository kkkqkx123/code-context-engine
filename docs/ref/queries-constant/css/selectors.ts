/*
CSS Selector-specific Tree-Sitter Query Patterns
Optimized for code chunking and vector embedding
*/
export default `
; CSS rulesets and selectors - primary code structure
(rule_set
  (selectors
    (class_selector
      (class_name) @name.definition.ruleset)) @definition.ruleset)

; Class selectors - important for styling components
(class_selector
  (class_name) @name.definition.class) @definition.class_selector

; ID selectors - important for unique elements
(id_selector
  (id_name) @name.definition.id) @definition.id_selector

; Tag selectors - important for element styling
(tag_name) @name.definition.tag

; Pseudo class selectors - important for state styling
(pseudo_class_selector
  (class_name) @name.definition.pseudo_class) @definition.pseudo_class

; Pseudo element selectors - important for element parts
(pseudo_element_selector
  (tag_name) @name.definition.pseudo_element) @definition.pseudo_element

; Attribute selectors - important for conditional styling
(attribute_selector
  (attribute_name) @name.definition.attribute) @definition.attribute_selector

; Universal selector - important for global styling
(universal_selector) @definition.universal_selector

; Child selectors - important for direct descendant styling
(child_selector) @definition.child_selector

; Sibling selectors - important for adjacent element styling
(sibling_selector) @definition.sibling_selector
(adjacent_sibling_selector) @definition.adjacent_sibling_selector

; Descendant selectors - important for nested element styling
(descendant_selector) @definition.descendant_selector

; Namespace selectors - important for XML/SVG styling
(namespace_selector) @definition.namespace_selector

; Nesting selector - important for CSS nesting
(nesting_selector) @definition.nesting_selector
`;