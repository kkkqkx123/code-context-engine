-- .cce/plugins/tmpl_remap.lua
-- Demo: LanguageRemap — remap a template DSL onto a host built-in grammar.
--
-- The `tmpl` language reuses the host's JavaScript grammar, so `.tmpl` files
-- get the same AST entity extraction as `.js` files without embedding any
-- tree-sitter grammar in a plugin. Query schemes are optional: when a query
-- type is missing, the host falls back to the referenced language's scheme.
--
-- Useful for dialects/supersets of existing languages (template DSLs, config
-- extensions, preprocessor variants) that don't need a dedicated grammar.

plugin = {
    id = "tmpl_remap_plugin",
    name = "Template Language Remap",
    version = "0.1.0",
    priority = 10,
    description = "Remaps the 'tmpl' language onto the host JavaScript grammar (LanguageRemap demo).",
    capabilities = { "language_remap" },
    language_name = "tmpl",
    language_extensions = { "tmpl" },
    remap_grammar_language = "JavaScript"
}
