//! Relation resolver for converting raw relations to resolved relations
//!
//! Provides functionality to resolve raw relations by looking up symbols in
//! a global symbol table and classifying external calls (standard library,
//! external packages, or unknown).

use super::RelationResolver;
use crate::index::core::RelationIndex;
use crate::symbol::SymbolRef;
use crate::symbol_table::ProjectSymbolTable;
use crate::symbol_table::ResolutionContext;
use crate::symbol_table::project::OverloadContext;

use cce_types::entity::EntityId;
use cce_types::entity::EntityKind;
use cce_types::{ParsedFile, language::Language};
use std::collections::HashMap;

/// File-scoped inputs shared by every name-candidate resolution within one
/// parsed file.
pub(crate) struct NameCandidateContext<'a> {
    pub parsed: &'a ParsedFile,
    pub symbol_table: &'a ProjectSymbolTable,
    pub entity_index: &'a RelationIndex,
    pub resolution_context: &'a ResolutionContext,
    /// Per-file local→global entity ID remap (see
    /// [`RelationIndex::entity_id_remap_for`]).
    pub file_remap: Option<&'a HashMap<EntityId, EntityId>>,
    pub overload_ctx: Option<&'a OverloadContext>,
}

impl RelationResolver {
    /// Extract standard library name from a callee name
    ///
    /// This helper method extracts the library/module name from a standard library entity.
    /// For example:
    /// - "std::collections::HashMap" -> "std::collections"
    /// - "os.path.join" -> "os.path"
    /// - "print" -> "builtin"
    ///
    /// # Arguments
    ///
    /// * `name` - The full name of the standard library entity
    /// * `language` - The programming language
    ///
    /// # Returns
    ///
    pub fn extract_stdlib_name(&self, name: &str, language: &Language) -> String {
        match language {
            // Rust: std::collections::HashMap -> std::collections
            Language::Rust => {
                if let Some(pos) = name.rfind("::") {
                    name[..pos].to_string()
                } else {
                    name.to_string()
                }
            }
            // Python: os.path.join -> os.path
            Language::Python => {
                if let Some(pos) = name.rfind('.') {
                    name[..pos].to_string()
                } else {
                    "builtin".to_string()
                }
            }
            // JavaScript/TypeScript: console.log -> console
            Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx => {
                if let Some(pos) = name.rfind('.') {
                    name[..pos].to_string()
                } else {
                    name.to_string()
                }
            }
            // Go: fmt.Println -> fmt
            Language::Go => {
                if let Some(pos) = name.rfind('.') {
                    name[..pos].to_string()
                } else {
                    name.to_string()
                }
            }
            // Java: java.util.List -> java.util
            Language::Java => {
                if let Some(pos) = name.rfind('.') {
                    name[..pos].to_string()
                } else {
                    name.to_string()
                }
            }
            // Other languages: return the full name
            _ => name.to_string(),
        }
    }

    /// Candidate resolution names for a callee, in priority order.
    ///
    /// The literal name is tried first — a local definition shadows any
    /// import alias. When the literal name is an import alias
    /// (`import { foo as bar }` → `bar()`), the import's original symbol
    /// name is appended as a fallback so the call resolves to the real
    /// entity instead of degrading to a spurious Unknown edge.
    pub(crate) fn resolution_names(&self, callee_name: &str, parsed: &ParsedFile) -> Vec<String> {
        let mut names = vec![callee_name.to_string()];
        if let Some(redirect) = self.import_alias_redirect(callee_name, parsed) {
            if redirect != callee_name {
                names.push(redirect);
            }
        }
        names
    }

    /// If `callee_name` is the local alias of an import in this file
    /// (`import { foo as bar }` → `bar`), return the import's original
    /// symbol name (`foo`). Import tables capture the alias on
    /// `ImportTarget.local_name` and the original symbol on
    /// `original_name`; only explicitly aliased imports redirect.
    pub(crate) fn import_alias_redirect(
        &self,
        callee_name: &str,
        parsed: &ParsedFile,
    ) -> Option<String> {
        let imports = parsed.import_table.as_ref()?;
        for import in &imports.standardized_imports {
            let local = import.alias.as_deref().unwrap_or(&import.target.local_name);
            if local == callee_name {
                if let Some(original) = import.target.original_name.as_deref() {
                    return Some(original.to_string());
                }
            }
        }
        None
    }

    /// Resolve a single callee name candidate to a symbol reference and an
    /// entity ID. The name-agnostic core of the resolution chain, extracted
    /// so alias redirects can retry the same lookups under the original
    /// symbol name.
    ///
    /// The returned entity ID (when present) always lives in the
    /// index-global ID space: hits from the caller file's local symbols carry
    /// ParsedFile-local IDs and are translated through `file_remap`
    /// (the per-file local→global table built by `index_file_core`),
    /// while lookups against `entity_index` already yield global IDs.
    /// Mixing the two spaces without translation silently corrupts edges
    /// whenever a global callee ID numerically collides with one of the
    /// caller file's local IDs.
    pub(crate) fn resolve_name_candidate(
        &self,
        name: &str,
        is_stdlib: bool,
        ctx: &NameCandidateContext<'_>,
    ) -> (Option<SymbolRef>, Option<EntityId>) {
        let NameCandidateContext {
            parsed,
            symbol_table,
            entity_index,
            resolution_context,
            file_remap,
            overload_ctx: _,
        } = *ctx;
        // Translate a ParsedFile-local entity ID into index-global space.
        // Unmapped IDs pass through unchanged: they either already belong to
        // the global space or belong to an unchanged hot-update file whose
        // parsed IDs were seeded to stay globally unique.
        let to_global_id = |id: EntityId| -> EntityId {
            file_remap
                .and_then(|remap| remap.get(&id))
                .copied()
                .unwrap_or(id)
        };
        // Record one lookup for the enhanced resolution attempt
        self.resolve_lookups
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Some(metrics) = &self.metrics {
            metrics.resolve_lookups_total.increment();
        }
        let symbol_ref = if is_stdlib {
            symbol_table.resolve_enhanced_strict(name, resolution_context)
        } else if let Some(overload_ctx) = ctx.overload_ctx {
            symbol_table.resolve_enhanced_with_overload(
                name,
                resolution_context,
                Some(overload_ctx),
            )
        } else {
            symbol_table.resolve_enhanced(name, resolution_context)
        };

        // Mixin/assign forwarding runs before the symbol-dependent chain:
        // when the receiver was extended by `mixin(recv, src)` the target
        // lives in the source module's file, which no name-only lookup can
        // reach. It needs no prior symbol, so it must not sit inside the
        // symbol-gated chain below.
        let resolved_entity_id: Option<EntityId> =
            self.resolve_mixin_member(name, ctx).or_else(|| {
                symbol_ref
                    .as_ref()
                    .and_then(|symbol_ref| {
                        // A receiver-qualified call (`engine.RouterGroup.Use`)
                        // that the symbol table resolved to another file must not
                        // be captured by a same-name local (here `Engine.Use` in
                        // the caller file): the receiver selects the method, so
                        // the local same-name method is a different function.
                        // Bare names keep local priority, except relative-import
                        // aliases (`var View = require('./view')`), which own no
                        // callable meaning themselves: the target module's export is
                        // the reference, so the local alias variable must not win.
                        let symbol_remote_target = name.contains('.')
                            && !cce_types::path::normalized_equals(
                                symbol_ref.metadata.location.file_path.as_ref(),
                                &parsed.path,
                            );
                        (!Self::is_relative_import_alias(parsed, name))
                            .then(|| Self::local_definition_id(parsed, name))
                            .flatten()
                            .map(to_global_id)
                            .or_else(|| {
                                // Qualified names (`obj.method`, `Foo::new`) do
                                // not exist as local-symbol keys; retry with the
                                // last segment. Skipped for stdlib targets so
                                // `Vec::new` can never hit a local `new`.
                                // For Rust, suppress fallback when the receiver is a
                                // generic local variable (e.g., `value.clone` where
                                // `value: T: Clone`) to avoid spurious self-loops.
                                if symbol_remote_target
                                    || self.should_block_last_segment_fallback(name, is_stdlib, ctx)
                                {
                                    return None;
                                }
                                self.last_segment_for_resolution(name, is_stdlib)
                                    .and_then(|last| {
                                        Self::local_definition_id(parsed, last).map(to_global_id)
                                    })
                            })
                            .or_else(|| {
                                if RelationIndex::is_synthetic_id(symbol_ref.symbol_id) {
                                    self.resolve_symbol_to_entity_id(symbol_ref, entity_index)
                                } else {
                                    Some(symbol_ref.symbol_id)
                                }
                            })
                    })
                    .or_else(|| {
                        // Fallback: the enhanced resolver did not produce a symbol
                        // ref, but the flat project index may still know the target.
                        // resolve through the real entity index by name; the
                        // symbol-table ID space must never be mixed into entity IDs.
                        // query keys use the same normalized path form the
                        // symbol table registered them under.
                        // Self-receiver abstention, mirroring the symbol-table
                        // layers: a bare name with a self-rooted (recovered or
                        // inferred) receiver names the caller's own object graph.
                        // The caller-file entity already had its chance
                        // upstream; only same-file locals may still satisfy
                        // it, never a cross-file affinity pick.
                        if ctx
                            .overload_ctx
                            .as_ref()
                            .and_then(|overload| overload.receiver_type.as_deref())
                            .is_some_and(cce_parser_core::is_self_receiver)
                        {
                            return parsed
                                .local_symbols
                                .get(name)
                                .and_then(|ids| ids.first().copied())
                                .map(to_global_id);
                        }
                        let normalized_path = cce_types::normalize_project_path(&parsed.path);
                        let qualified_name = format!("{}::{}", normalized_path, name);
                        if symbol_table
                            .get_by_qualified_name(&qualified_name)
                            .is_some()
                        {
                            self.resolve_entity_by_name(
                                name,
                                Some(&parsed.path),
                                Some(&parsed.path),
                                entity_index,
                            )
                        } else if symbol_table.get_by_simple_name(name).is_some() {
                            self.resolve_entity_by_name(
                                name,
                                None,
                                Some(&parsed.path),
                                entity_index,
                            )
                        } else if self.should_block_last_segment_fallback(name, is_stdlib, ctx) {
                            None
                        } else if let Some(last) = self.last_segment_for_resolution(name, is_stdlib)
                        {
                            let qualified_last = format!("{}::{}", normalized_path, last);
                            if symbol_table
                                .get_by_qualified_name(&qualified_last)
                                .is_some()
                            {
                                self.resolve_entity_by_name(
                                    last,
                                    Some(&parsed.path),
                                    Some(&parsed.path),
                                    entity_index,
                                )
                            } else if symbol_table.get_by_simple_name(last).is_some() {
                                self.resolve_entity_by_name(
                                    last,
                                    None,
                                    Some(&parsed.path),
                                    entity_index,
                                )
                            } else {
                                parsed
                                    .local_symbols
                                    .get(last)
                                    .and_then(|ids| ids.first().copied())
                                    .map(to_global_id)
                            }
                        } else {
                            parsed
                                .local_symbols
                                .get(name)
                                .and_then(|ids| ids.first().copied())
                                .map(to_global_id)
                        }
                    })
            });

        (symbol_ref, resolved_entity_id)
    }

    /// Return the last path segment of a qualified name (`obj.method` ->
    /// `method`, `Vec::new` -> `new`) when the name is qualified and the
    /// target is not stdlib.
    ///
    /// Extraction now preserves full callee paths, so lookups that were
    /// keyed on the bare name must retry with the trailing segment. Stdlib
    /// names are excluded so `Vec::new` can never resolve to a project-local
    /// `new`.
    pub(crate) fn last_segment_for_resolution<'a>(
        &self,
        name: &'a str,
        is_stdlib: bool,
    ) -> Option<&'a str> {
        if is_stdlib {
            return None;
        }
        let last = name.rsplit(['.', ':']).next().unwrap_or(name);
        (last != name).then_some(last)
    }

    fn should_block_last_segment_fallback(
        &self,
        name: &str,
        is_stdlib: bool,
        ctx: &NameCandidateContext<'_>,
    ) -> bool {
        // Receiver type knowledge is resolved here (the resolver owns the
        // inference lookups); the decision rules stay centralized in the
        // post-processor.
        let receiver_type_known = if is_stdlib || (!name.contains('.') && !name.contains(':')) {
            false
        } else {
            let head = name.split(['.', ':']).next().unwrap_or("");
            !head.is_empty()
                && self
                    .lookup_inferred_receiver_type(
                        head,
                        &ctx.parsed.path,
                        ctx.symbol_table,
                        ctx.parsed,
                    )
                    .is_some()
        };
        // Delegated to the centralized post-processor so all receiver
        // heuristics live in one deterministic location.
        self.effective_post_processor()
            .should_block_last_segment_fallback(
                name,
                ctx.parsed,
                ctx.symbol_table,
                is_stdlib,
                receiver_type_known,
            )
    }

    /// Resolve `recv.member` through a mixin/assign call in this file.
    ///
    /// `mixin(app, proto)` / `Object.assign(app, proto)` attach another
    /// module's members to a local object; a later `app.handle(...)` then
    /// names the mixed-in member, not a local definition. When the receiver
    /// is the mixin target and the source alias is a relative import, the
    /// member is resolved in the source module's file.
    pub(crate) fn resolve_mixin_member(
        &self,
        name: &str,
        ctx: &NameCandidateContext<'_>,
    ) -> Option<EntityId> {
        let (receiver, member) = name.rsplit_once(['.', ':'])?;
        if receiver.is_empty()
            || member.is_empty()
            || receiver.contains(['.', ':'])
            || !super::is_bare_identifier(receiver)
            || !super::is_bare_identifier(member)
        {
            return None;
        }
        let parsed = ctx.parsed;
        for raw in &parsed.raw_relations {
            let is_mixin_attach = raw.dst_name == "mixin" || raw.dst_name.ends_with(".assign");
            if !is_mixin_attach || !raw.relation_type.is_call() {
                continue;
            }
            // Raw call spans cover the callee name only, so the argument
            // list sits past `span.end_byte`. Extend the window past the
            // opening paren (allowing a little whitespace) and let the
            // balanced splitter find the arguments.
            let bytes = parsed.source.as_bytes();
            let mut paren = raw.span.end_byte.min(bytes.len());
            let mut skipped = 0usize;
            while paren < bytes.len() && skipped < 16 && bytes[paren] != b'(' {
                if !bytes[paren].is_ascii_whitespace() {
                    break;
                }
                paren += 1;
                skipped += 1;
            }
            if paren >= bytes.len() || bytes[paren] != b'(' {
                continue;
            }
            let mut probe = raw.span;
            probe.end_byte = (paren + 2000).min(bytes.len());
            let Some(args) = super::extract_call_arguments(parsed.source.as_ref(), probe) else {
                continue;
            };
            if args.len() < 2 || args[0].trim() != receiver {
                continue;
            }
            let src_alias = args[1].trim();
            if !super::is_bare_identifier(src_alias) {
                continue;
            }
            let import = parsed
                .import_table
                .as_ref()?
                .standardized_imports
                .iter()
                .find(|import| {
                    let local = import.alias.as_deref().unwrap_or(&import.target.local_name);
                    local == src_alias && import.is_relative
                })?;
            let package = ctx.symbol_table.get_package_for_file(&parsed.path)?;
            let target = package.get_module_by_relative_source(&parsed.path, &import.source)?;
            return self.resolve_entity_by_name(
                member,
                Some(&target.file_path),
                Some(&parsed.path),
                ctx.entity_index,
            );
        }
        None
    }

    /// Whether `name` is bound by a relative JavaScript/TypeScript import
    /// in this file, in which case the import target owns the reference.
    fn is_relative_import_alias(parsed: &ParsedFile, name: &str) -> bool {
        if !matches!(
            parsed.language,
            Language::JavaScript | Language::TypeScript | Language::Jsx | Language::Tsx
        ) {
            return false;
        }
        parsed.import_table.as_ref().is_some_and(|table| {
            table.standardized_imports.iter().any(|import| {
                let local = import.alias.as_deref().unwrap_or(&import.target.local_name);
                local == name && import.is_relative
            })
        })
    }

    fn local_definition_id(parsed: &ParsedFile, name: &str) -> Option<EntityId> {
        let ids = parsed.local_symbols.get(name)?;
        for id in ids {
            let is_import = parsed
                .entities
                .iter()
                .find(|entity| entity.id == *id)
                .is_some_and(|entity| entity.kind == EntityKind::Import);
            if !is_import {
                return Some(*id);
            }
        }
        None
    }
}
