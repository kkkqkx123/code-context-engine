//! EntityGroup conversion methods for AST to Natural Language conversion
//!
//! This module handles the conversion of entity groups into natural language,
//! producing `GroupConversions` that maintain the hierarchical relationship
//! between groups and their conversions.
//!
//! # Interface Contract with Chunker
//!
//! The `GroupConversions` struct serves as the primary interface between the converter
//! and chunker modules. It provides:
//!
//! - **Header Conversion**: Group-level description (e.g., class overview)
//! - **Member Conversions**: Individual entity descriptions (e.g., method descriptions)
//!
//! The chunker uses this structure to implement smart chunking strategies where:
//! 1. Header text is repeated in each chunk for context preservation
//! 2. Members are grouped by token budget to respect size limits
//! 3. Each chunk maintains semantic coherence
//!
//! # Output Structure
//!
//! For a class with methods, the output might be:
//! ```text
//! GroupConversions {
//!     header_conversion: Some("A User class with authentication methods"),
//!     member_conversions: [
//!         "login method validates credentials",
//!         "logout method clears session",
//!         ...
//!     ]
//! }
//! ```

use crate::ast_to_nl::ConversionRequest;
use crate::ast_to_nl::embedding::text_cleaner::EmbeddingTextCleaner;
use crate::grouper::ProcessingResult;
use crate::grouper::{EntityGroup, PatternInfo};
use cce_types::ConversionResult;
use cce_types::OutputMode;
use cce_types::entity::{EntityId, EntityKind, GroupedEntity, meta_keys};
use std::collections::{HashMap, HashSet};

use super::index_enrichment::IndexTextEnricher;

/// Collected child summary data used to enrich a parent group's description
struct ParentEnrichment {
    parent_idx: usize,
    child_names: Vec<String>,
    child_doc_lines: Vec<String>,
    stdlib_names: Vec<String>,
}

/// Represents a group with its associated conversion results
/// Maintains the hierarchical relationship between groups and their conversions
///
/// Cross-layer contract, defined in `cce_core` so the plugin chunk
/// capability can reference it.
pub use cce_types::ast_to_nl::GroupConversions;

impl super::AstToNlConverter {
    /// Convert entity groups from PreProcessor output.
    ///
    /// This is the single entry point for all NL conversion paths.
    /// When `processing_result` is provided, control-flow and behavior sidecar
    /// data is automatically enriched into the output text (no separate
    /// `_for_index` method needed).
    pub fn convert_entity_groups(
        &self,
        groups: &[EntityGroup],
        file_path: &str,
        request: Option<&ConversionRequest>,
        processing_result: Option<&ProcessingResult>,
        source: Option<&str>,
    ) -> Vec<GroupConversions> {
        // Try batch plugin processing first if plugins are available
        if let Some(ref registry) = self.plugin_registry {
            let language_str = groups.first().map(|g| g.language.to_string());
            let language = language_str.as_deref();
            let mode = self.resolve_mode(request);

            let has_matching_plugins = match mode {
                OutputMode::Bm25 => !registry
                    .get_bm25_generators(Some(file_path), language)
                    .is_empty(),
                OutputMode::Embedding => !registry
                    .get_embedding_generators(Some(file_path), language)
                    .is_empty(),
                OutputMode::Both => {
                    !registry
                        .get_bm25_generators(Some(file_path), language)
                        .is_empty()
                        || !registry
                            .get_embedding_generators(Some(file_path), language)
                            .is_empty()
                }
            };

            if has_matching_plugins && !groups.is_empty() {
                let group_refs: Vec<&EntityGroup> = groups.iter().collect();
                let file_paths: Vec<&str> = vec![file_path; groups.len()];

                let mut group_conversions = self
                    .convert_entity_groups_batch(&group_refs, &file_paths, request)
                    .into_iter()
                    .zip(groups.iter())
                    .map(|(conversions, group)| {
                        Self::assemble_group_conversions(group, conversions)
                    })
                    .collect::<Vec<_>>();

                if let (Some(pr), Some(src)) = (processing_result, source) {
                    for gc in &mut group_conversions {
                        let language = gc.group.language;
                        if let Some(header) = gc.header_conversion.as_mut() {
                            IndexTextEnricher::new().enrich_conversion(
                                header,
                                pr,
                                src,
                                language,
                                &self.bm25_cleaner,
                            );
                        }
                        for member in &mut gc.member_conversions {
                            IndexTextEnricher::new().enrich_conversion(
                                member,
                                pr,
                                src,
                                language,
                                &self.bm25_cleaner,
                            );
                        }
                    }
                }

                return group_conversions;
            }
        }

        // Collect all entity IDs that are headers of some group.
        // If a member entity of one group is also the header of another group,
        // we skip converting it as a member — it will be fully covered by its
        // own group's header conversion. This prevents duplicate sections
        // where the same entity appears twice with split content.
        let header_only_ids: HashSet<EntityId> = groups
            .iter()
            .filter_map(|g| g.header.as_ref().map(|h| h.id))
            .collect();

        let mut group_conversions: Vec<GroupConversions> = Vec::new();
        let mut converted_entity_ids: HashSet<EntityId> = HashSet::new();

        for group in groups {
            // Skip standalone groups for low-level entities.
            if group.members.is_empty() {
                if let Some(ref header) = group.header {
                    if matches!(header.kind, EntityKind::Variable) {
                        continue;
                    }
                }
            }

            // Skip groups whose header has already been converted by a previous group.
            // This prevents file documentation from appearing in both a merged group
            // and a standalone file_doc group.
            if let Some(ref header) = group.header {
                if converted_entity_ids.contains(&header.id) {
                    continue;
                }
            }

            let conversions =
                self.convert_entity_group(group, file_path, request, &header_only_ids);
            let assembled = Self::assemble_group_conversions(group, conversions);

            // Track all entity IDs covered by this group
            if let Some(ref header) = group.header {
                converted_entity_ids.insert(header.id);
            }
            for conv in &assembled.member_conversions {
                converted_entity_ids.insert(conv.entity_id);
            }

            group_conversions.push(assembled);
        }

        // Phase 2: Enrich parent/child descriptions based on group hierarchy.
        Self::enrich_hierarchy_descriptions(&mut group_conversions);

        // Phase 3: Enrich with control-flow and behavior sidecars when available.
        if let (Some(pr), Some(src)) = (processing_result, source) {
            for gc in &mut group_conversions {
                let language = gc.group.language;
                if let Some(header) = gc.header_conversion.as_mut() {
                    IndexTextEnricher::new().enrich_conversion(
                        header,
                        pr,
                        src,
                        language,
                        &self.bm25_cleaner,
                    );
                }
                for member in &mut gc.member_conversions {
                    IndexTextEnricher::new().enrich_conversion(
                        member,
                        pr,
                        src,
                        language,
                        &self.bm25_cleaner,
                    );
                }
            }
        }

        group_conversions
    }

    /// Enrich parent/child descriptions based on group hierarchy.
    ///
    /// Phase 2 enrichment after all groups have been converted.
    /// Builds parent→children inverse map from parent_group_id on child groups,
    /// then enriches both parent and child descriptions with cross-references.
    fn enrich_hierarchy_descriptions(group_conversions: &mut [GroupConversions]) {
        // Build lookup: group_id -> index in group_conversions
        let mut conv_index: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (i, gc) in group_conversions.iter().enumerate() {
            conv_index.insert(gc.group.group_id.to_string(), i);
        }

        // Build inverse map: parent_group_id -> Vec<child_index>
        let mut parent_to_children: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();
        for (i, gc) in group_conversions.iter().enumerate() {
            if let Some(ref parent_id) = gc.group.parent_group_id {
                if let Some(&parent_idx) = conv_index.get(parent_id.as_str()) {
                    parent_to_children.entry(parent_idx).or_default().push(i);
                }
            }
        }

        if parent_to_children.is_empty() {
            return;
        }

        // Phase 2a: Enrich parent descriptions with child summaries
        // Collect enrichment data first to avoid borrow conflicts
        let parent_enrichments: Vec<ParentEnrichment> = parent_to_children
            .iter()
            .filter_map(|(&parent_idx, child_indices)| {
                let mut child_names: Vec<String> = Vec::new();
                let mut child_doc_lines: Vec<String> = Vec::new();
                let mut stdlib_names: Vec<String> = Vec::new();

                for &child_idx in child_indices {
                    if let Some(ref child_conv) = group_conversions[child_idx].header_conversion {
                        child_names.push(child_conv.name.clone());
                        // Collect stdlib child names for compact summary
                        if group_conversions[child_idx]
                            .group
                            .header
                            .as_ref()
                            .is_some_and(|h| h.is_stdlib)
                        {
                            stdlib_names.push(child_conv.name.clone());
                        }
                        if let Some(ref emb_text) = child_conv.embedding_text {
                            let first_line = emb_text.lines().next().unwrap_or("").trim();
                            if !first_line.is_empty() {
                                child_doc_lines
                                    .push(format!("{}: {}", child_conv.name, first_line));
                            }
                        } else if let Some(ref bm25_text) = child_conv.bm25_text {
                            let first_line = bm25_text.lines().next().unwrap_or("").trim();
                            if !first_line.is_empty() {
                                child_doc_lines
                                    .push(format!("{}: {}", child_conv.name, first_line));
                            }
                        }
                    }
                }

                if child_names.is_empty() {
                    None
                } else {
                    Some(ParentEnrichment {
                        parent_idx,
                        child_names,
                        child_doc_lines,
                        stdlib_names,
                    })
                }
            })
            .collect();

        // Apply parent enrichments
        for ParentEnrichment {
            parent_idx,
            child_names,
            child_doc_lines,
            stdlib_names,
        } in parent_enrichments
        {
            if let Some(ref mut header) = group_conversions[parent_idx].header_conversion {
                let parent_name = group_conversions[parent_idx].group.name.to_string();
                if let Some(ref mut bm25_text) = header.bm25_text {
                    bm25_text.push_str(" children:");
                    bm25_text.push_str(&child_names.join(" "));
                }
                for child_name in &child_names {
                    // Skip if child name is a substring of parent name (avoid keyword dilution)
                    if !parent_name.contains(child_name.as_str())
                        && !header.keywords.contains(child_name)
                    {
                        header.keywords.push(child_name.clone());
                    }
                }
                if let Some(ref mut emb_text) = header.embedding_text {
                    if !emb_text.is_empty() {
                        // Append stdlib summary as a natural-language "Implements" line
                        let trait_names: Vec<&String> = stdlib_names
                            .iter()
                            .filter(|name| {
                                // Drop structural import blocks (e.g. `core::{...}`) and
                                // qualified paths — keep simple trait identifiers only.
                                !name.contains('{') && !name.contains("::")
                            })
                            .collect();
                        let mut seen = HashSet::new();
                        let unique_names: Vec<&str> = trait_names
                            .into_iter()
                            .filter(|name| seen.insert(name.as_str()))
                            .map(|name| name.as_str())
                            .collect();
                        if !unique_names.is_empty() {
                            emb_text
                                .push_str(&format!("\nImplements: {}.", unique_names.join(", ")));
                        }
                        // Append non-stdlib children with their NL descriptions
                        let non_stdlib_lines: Vec<&String> = child_doc_lines
                            .iter()
                            .filter(|line| {
                                !stdlib_names
                                    .iter()
                                    .any(|s| line.starts_with(&format!("{}:", s)))
                            })
                            .collect();
                        if !non_stdlib_lines.is_empty() {
                            emb_text.push_str("\n\nContains:");
                            for line in non_stdlib_lines {
                                emb_text.push_str("\n- ");
                                emb_text.push_str(line);
                            }
                        }
                    }
                }
            }
        }

        // Phase 2b: Enrich child descriptions with parent context
        // Collect enrichment data first
        let child_enrichments: Vec<(usize, String, EntityKind)> = group_conversions
            .iter()
            .enumerate()
            .filter_map(|(i, gc)| {
                gc.group.parent_group_id.as_ref().and_then(|parent_id| {
                    conv_index.get(parent_id.as_str()).map(|&parent_idx| {
                        let parent_name = group_conversions[parent_idx].group.name.to_string();
                        let parent_kind = group_conversions[parent_idx].group.kind;
                        (i, parent_name, parent_kind)
                    })
                })
            })
            .collect();

        // Apply child enrichments — inject parent context as a prefix for embedding path
        for (child_idx, parent_name, parent_kind) in child_enrichments {
            if let Some(ref mut header) = group_conversions[child_idx].header_conversion {
                if let Some(ref mut bm25_text) = header.bm25_text {
                    bm25_text.push_str(&format!(" belongs_to:{}", parent_name));
                }
                if !header.keywords.contains(&parent_name) {
                    header.keywords.push(parent_name.clone());
                }
                if let Some(ref mut emb_text) = header.embedding_text {
                    if !emb_text.is_empty() {
                        // Determine the context prefix format based on entity kind.
                        // Function-like entities use a compact qualified path instead
                        // of the verbose "In <parent> <kind>:" boilerplate.
                        let prefix = if matches!(
                            header.kind,
                            EntityKind::Function | EntityKind::Method | EntityKind::Constructor
                        ) {
                            format!("{}.{}(). ", parent_name, header.name)
                        } else if matches!(header.kind, EntityKind::Field | EntityKind::Property) {
                            format!("Field of {}: ", parent_name)
                        } else if matches!(header.kind, EntityKind::TraitImpl) {
                            format!("Implementation for {}: ", parent_name)
                        } else {
                            format!("In {} {}: ", parent_name, Self::kind_label(parent_kind))
                        };
                        let mut new_text = prefix;
                        new_text.push_str(emb_text);
                        *emb_text = new_text;
                    }
                }
            }
        }

        // Phase 2c: Aggregate call relationships at group level.
        // Collect all call paths from member conversions within each group
        // and append a summary to the group's embedding text.
        for gc in group_conversions.iter_mut() {
            let mut local_calls: Vec<String> = Vec::new();
            let mut external_calls: Vec<String> = Vec::new();
            let group_name = gc.group.name.to_string();

            for member_conv in &gc.member_conversions {
                if let Some(ref emb_text) = member_conv.embedding_text {
                    for line in emb_text.lines() {
                        let line = line.trim();
                        if line.starts_with("Calls ") {
                            // Extract call names: "Calls clone, clone_from, get."
                            let calls_str = line.trim_start_matches("Calls ").trim_end_matches('.');
                            for call_name in calls_str.split(',').map(|s| s.trim()) {
                                if call_name.is_empty() {
                                    continue;
                                }
                                // Classify as local (same group prefix) or external
                                if call_name.contains(&format!("::{}", group_name))
                                    || call_name.contains(&format!("{}::", group_name))
                                {
                                    local_calls.push(call_name.to_string());
                                } else {
                                    external_calls.push(call_name.to_string());
                                }
                            }
                        }
                    }
                }
            }

            if !local_calls.is_empty() || !external_calls.is_empty() {
                // Deduplicate and limit
                local_calls.sort();
                local_calls.dedup();
                external_calls.sort();
                external_calls.dedup();
                local_calls.truncate(5);
                external_calls.truncate(5);

                if let Some(ref mut header) = gc.header_conversion {
                    if let Some(ref mut emb_text) = header.embedding_text {
                        if !emb_text.is_empty() {
                            emb_text.push_str("\nCalls: ");
                            let mut parts: Vec<String> = Vec::new();
                            if !local_calls.is_empty() {
                                parts.push(format!("internal [{}]", local_calls.join(", ")));
                            }
                            if !external_calls.is_empty() {
                                parts.push(format!("external [{}]", external_calls.join(", ")));
                            }
                            emb_text.push_str(&parts.join("; "));
                            emb_text.push('.');
                        }
                    }
                }
            }
        }

        // Phase 2d: Enrich member conversions with parent context for container groups.
        // Unlike Phase 2b (cross-group via parent_group_id), this handles in-group
        // parent-child relationships (e.g., ClassWithMethods → method conversions).
        for gc in group_conversions.iter_mut() {
            if !gc.group.group_type.is_container() {
                continue;
            }
            let parent_name = gc.group.name.to_string();
            let parent_kind = gc.group.kind;
            for member in &mut gc.member_conversions {
                if let Some(ref mut emb_text) = member.embedding_text {
                    if !emb_text.is_empty() {
                        let prefix = if matches!(
                            member.kind,
                            EntityKind::Function | EntityKind::Method | EntityKind::Constructor
                        ) {
                            format!("{}.{}(). ", parent_name, member.name)
                        } else if matches!(member.kind, EntityKind::Field | EntityKind::Property) {
                            format!("Field of {}: ", parent_name)
                        } else {
                            format!("In {} {}: ", parent_name, Self::kind_label(parent_kind))
                        };
                        let mut new_text = prefix;
                        new_text.push_str(emb_text);
                        *emb_text = new_text;
                    }
                }
                if let Some(ref mut bm25_text) = member.bm25_text {
                    bm25_text.push_str(&format!(" belongs_to:{}", parent_name));
                }
            }
        }
    }

    /// Get a human-readable label for an entity kind.
    fn kind_label(kind: EntityKind) -> &'static str {
        match kind {
            EntityKind::Module => "module",
            EntityKind::Namespace => "namespace",
            EntityKind::Package => "package",
            EntityKind::Class => "class",
            EntityKind::Struct => "struct",
            EntityKind::Enum => "enum",
            EntityKind::EnumVariant => "variant",
            EntityKind::Union => "union",
            EntityKind::Trait => "trait",
            EntityKind::Interface => "interface",
            EntityKind::TraitImpl => "trait implementation",
            EntityKind::InherentImpl => "inherent implementation",
            EntityKind::TypeAlias => "type alias",
            EntityKind::Function => "function",
            EntityKind::Method => "method",
            EntityKind::Constructor => "constructor",
            EntityKind::Destructor => "destructor",
            EntityKind::Operator => "operator",
            EntityKind::Field => "field",
            EntityKind::Property => "property",
            EntityKind::Variable => "variable",
            EntityKind::Constant => "constant",
            EntityKind::Import => "import",
            EntityKind::Require => "require",
            EntityKind::Include => "include",
            EntityKind::Export => "export",
            EntityKind::Annotation => "annotation",
            EntityKind::Macro => "macro",
            EntityKind::StyleRule => "style rule",
            EntityKind::StyleSelector => "style selector",
            EntityKind::StyleProperty => "style property",
            EntityKind::Keyframe => "keyframe",
            EntityKind::Element => "element",
            EntityKind::Attribute => "attribute",
            EntityKind::Expression => "expression",
            EntityKind::Component => "component",
            EntityKind::Template => "template",
            EntityKind::Directive => "directive",
            EntityKind::ControlFlow => "control flow",
            EntityKind::Animation => "animation",
            EntityKind::Binding => "binding",
            EntityKind::Action => "action",
            EntityKind::AtRule => "at-rule",
            EntityKind::EventHandler => "event handler",
            EntityKind::ScriptContent => "script content",
            EntityKind::StyleContent => "style content",
            EntityKind::EmbeddedBlock => "embedded block",
            EntityKind::TestSuite => "test suite",
            EntityKind::TestCase => "test case",
            EntityKind::TestHook => "test hook",
            EntityKind::Assertion => "assertion",
            EntityKind::Mock => "mock",
            EntityKind::Unknown => "unknown",
        }
    }

    fn assemble_group_conversions(
        group: &EntityGroup,
        conversions: Vec<ConversionResult>,
    ) -> GroupConversions {
        let (header_conversion, member_conversions) = if group.header_id.is_some() {
            if conversions.is_empty() {
                (None, vec![])
            } else {
                let mut convs = conversions.into_iter();
                (convs.next(), convs.collect())
            }
        } else {
            (None, conversions)
        };

        GroupConversions {
            group: group.clone(),
            header_conversion,
            member_conversions,
        }
    }

    /// Convert a single entity group
    ///
    /// Dispatches to specific conversion logic based on group type and pattern info.
    /// Returns a Vec of ConversionResults - for pattern-aware groups, this may include
    /// the class description plus descriptions for significant/core methods.
    ///
    /// Three-tier conversion order:
    /// 1. Override-tier plugins (effective priority ≥ 0), first non-empty wins.
    /// 2. Built-in generators.
    /// 3. Fallback-tier plugins (negative priority), only when the built-in
    ///    produced no conversions at all.
    fn convert_entity_group(
        &self,
        group: &EntityGroup,
        file_path: &str,
        request: Option<&ConversionRequest>,
        header_only_ids: &HashSet<EntityId>,
    ) -> Vec<ConversionResult> {
        if let Some(ref registry) = self.plugin_registry {
            let language_str = group.language.to_string();
            let language = Some(language_str.as_str());
            let (bm25_above, bm25_below) =
                registry.get_override_bm25_generators(Some(file_path), language);
            let (embedding_above, embedding_below) =
                registry.get_override_embedding_generators(Some(file_path), language);

            // Step 1: overrides in priority order (short-circuit).
            if let Some(result) =
                self.generate_from_plugins(group, file_path, request, &bm25_above, &embedding_above)
            {
                return result;
            }

            // Step 2: built-in generation uses the same group-aware BM25/Embedding
            // dispatchers as the public generators. This keeps the indexing pipeline
            // aligned with direct generator calls.
            let builtin =
                self.convert_group_with_generators(group, file_path, request, header_only_ids);

            // Step 3: below-builtin fallback tier — consulted only when the
            // built-in produced no conversions (e.g. an unsupported language).
            if builtin.is_empty() {
                if let Some(result) = self.generate_from_plugins(
                    group,
                    file_path,
                    request,
                    &bm25_below,
                    &embedding_below,
                ) {
                    return result;
                }
            }
            return builtin;
        }

        self.convert_group_with_generators(group, file_path, request, header_only_ids)
    }

    /// Convert a group through the BM25/Embedding generator dispatchers.
    ///
    /// The converter layer is responsible for result packaging and preserving
    /// header/member hierarchy; the generator layer owns the text policy.
    fn convert_group_with_generators(
        &self,
        group: &EntityGroup,
        file_path: &str,
        request: Option<&ConversionRequest>,
        header_only_ids: &HashSet<EntityId>,
    ) -> Vec<ConversionResult> {
        let mut results = Vec::new();
        let mode = self.resolve_mode(request);

        if let Some(header) = &group.header {
            let bm25_text = if matches!(mode, OutputMode::Bm25 | OutputMode::Both) {
                self.bm25_generator.generate_for_group(group)
            } else {
                String::new()
            };

            let (embedding_text, embedding_line_offsets) =
                if matches!(mode, OutputMode::Embedding | OutputMode::Both) {
                    let descriptions = self.embedding_generator.generate_for_group(group);
                    if descriptions.is_empty() {
                        let text = self.embedding_generator.generate(header);
                        (text, Vec::new())
                    } else {
                        let mut line_count = 0usize;
                        let offsets: Vec<usize> = descriptions
                            .iter()
                            .enumerate()
                            .filter_map(|(i, desc)| {
                                let internal_nl = desc.lines().count().saturating_sub(1);
                                line_count += internal_nl;
                                if i < descriptions.len() - 1 {
                                    line_count += 1;
                                    Some(line_count)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        (descriptions.join("\n"), offsets)
                    }
                } else {
                    (String::new(), Vec::new())
                };

            let keywords = if matches!(mode, OutputMode::Bm25 | OutputMode::Both) {
                self.collect_group_keywords(group)
            } else {
                Vec::new()
            };

            let (bm25_brief, embedding_brief) = if group.members.is_empty() {
                (None, None)
            } else {
                let bm25_brief = if matches!(mode, OutputMode::Bm25 | OutputMode::Both) {
                    Some(self.bm25_generator.generate_brief_for_group(group))
                } else {
                    None
                };
                let embedding_brief = if matches!(mode, OutputMode::Embedding | OutputMode::Both) {
                    Some(self.embedding_generator.generate_brief_for_group(group))
                } else {
                    None
                };
                (bm25_brief, embedding_brief)
            };

            let mut header_result = match mode {
                OutputMode::Bm25 => ConversionResult::bm25_only(
                    header.id,
                    header.kind,
                    header.name.clone(),
                    file_path.to_string(),
                    bm25_text,
                    keywords,
                ),
                OutputMode::Embedding => ConversionResult::embedding_only(
                    header.id,
                    header.kind,
                    header.name.clone(),
                    file_path.to_string(),
                    embedding_text,
                ),
                OutputMode::Both => ConversionResult::new(
                    header.id,
                    header.kind,
                    header.name.clone(),
                    file_path.to_string(),
                    bm25_text,
                    embedding_text,
                    keywords,
                ),
            };

            let source_ids = {
                let ids = group.all_entity_ids();
                if ids.is_empty() { vec![header.id] } else { ids }
            };

            header_result = header_result
                .with_source_entity_ids(source_ids)
                .with_source_span(group.span);

            if !embedding_line_offsets.is_empty() {
                header_result.entity_end_lines = embedding_line_offsets;
            }

            // Trim docstring if it exceeds 50% of total tokens
            if let Some(ref mut emb_text) = header_result.embedding_text {
                if !header_result.entity_end_lines.is_empty() {
                    let cleaner = EmbeddingTextCleaner::new().with_docstring_ratio(0.50);
                    *emb_text =
                        cleaner.clean_with_boundaries(emb_text, &header_result.entity_end_lines);
                }
            }

            header_result.bm25_brief_header = bm25_brief;
            header_result.embedding_brief_header = embedding_brief;

            results.push(header_result);
        }

        for member in &group.members {
            if self.should_convert_member(group, member, header_only_ids) {
                let member_result = self.convert_grouped(member, file_path, request);
                results.push(member_result);
            }
        }

        if matches!(mode, OutputMode::Embedding | OutputMode::Both) {
            if let Some(summary) = Self::trait_impl_summary(group) {
                if let Some(first_trait_result) = results
                    .iter_mut()
                    .skip(usize::from(group.header.is_some()))
                    .find(|result| result.kind == EntityKind::TraitImpl)
                {
                    first_trait_result.embedding_text = Some(summary);
                }
            }
        }

        results
    }

    fn trait_impl_summary(group: &EntityGroup) -> Option<String> {
        let mut names = Vec::new();
        let mut seen = HashSet::new();

        for member in &group.members {
            if member.kind != EntityKind::TraitImpl || !seen.insert(member.name.clone()) {
                continue;
            }

            let mut name = String::new();
            if member.modifiers.iter().any(|modifier| modifier == "unsafe") {
                name.push_str("unsafe ");
            }
            name.push_str(&member.name);
            names.push(name);
        }

        (!names.is_empty()).then(|| format!("Trait implementations: {}.", names.join(", ")))
    }

    fn should_convert_member(
        &self,
        group: &EntityGroup,
        member: &GroupedEntity,
        header_only_ids: &HashSet<EntityId>,
    ) -> bool {
        if member.is_stdlib {
            return false;
        }

        // If this member is the same entity as the current group's header, skip it.
        // The header is already processed separately; processing it again as a member
        // would create duplicate output (e.g., same function appearing twice in BM25).
        if let Some(header) = &group.header {
            if header.id == member.id {
                return false;
            }
        }

        // If this member entity is the header of another group, skip it here.
        // It will be fully covered by its own group's header conversion,
        // including all its children (type params, fn params, etc.).
        // Converting it here would create a duplicate section with split content.
        if header_only_ids.contains(&member.id) {
            return false;
        }

        // Skip low-level entities (local variables) from appearing as
        // standalone sections. These are implementation details that should
        // only appear in their parent function's text content.
        if matches!(member.kind, EntityKind::Variable) {
            return false;
        }

        // Skip standalone zero-variant enums (e.g., `enum Void {}` used as
        // companion never-type for infallible closures). These provide no
        // retrievable information beyond the identifier itself.
        if matches!(member.kind, EntityKind::Enum)
            && member.doc_comment.is_none()
            && !member.metadata.contains_key(meta_keys::ANNOTATIONS)
        {
            return false;
        }

        if matches!(group.pattern_info, PatternInfo::None) {
            return true;
        }

        crate::grouper::types::pattern::get_member_role(&group.member_roles, &member.id)
            .map(|role| role.has_independent_description())
            .unwrap_or(false)
    }

    fn collect_group_keywords(&self, group: &EntityGroup) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut keywords = Vec::new();

        if let Some(header) = &group.header {
            for keyword in self.bm25_generator.extract_keywords(header) {
                if seen.insert(keyword.clone()) {
                    keywords.push(keyword);
                }
            }
        }

        for member in &group.members {
            if member.is_stdlib {
                continue;
            }
            for keyword in self.bm25_generator.extract_keywords(member) {
                if seen.insert(keyword.clone()) {
                    keywords.push(keyword);
                }
            }
        }

        keywords
    }

    /// Try to generate text using NL template plugins
    ///
    /// Returns a single-element Vec if any plugin succeeds (for header),
    /// merging BM25 and Embedding results into one ConversionResult.
    /// Returns None to fall back to the next tier (built-in or the
    /// below-builtin fallback tier).
    ///
    /// The registry returns generators sorted by priority (descending), so
    /// the first plugin that returns text is deterministically the
    /// highest-priority one.
    fn generate_from_plugins(
        &self,
        group: &EntityGroup,
        file_path: &str,
        request: Option<&ConversionRequest>,
        bm25_generators: &[&std::sync::Arc<dyn cce_plugin::CodePlugin>],
        embedding_generators: &[&std::sync::Arc<dyn cce_plugin::CodePlugin>],
    ) -> Option<Vec<ConversionResult>> {
        let mode = self.resolve_mode(request);

        let bm25_text: Option<String> = if matches!(mode, OutputMode::Bm25 | OutputMode::Both) {
            // Serial short-circuit: stop at the first plugin producing text,
            // so lower-priority generators are not invoked (they may carry
            // side effects or external API costs).
            let mut found = None;
            for plugin in bm25_generators {
                let plugin_id = plugin.metadata().id.clone();
                match plugin.generate_bm25(group) {
                    Ok(Some(text)) => {
                        found = Some(text);
                        break;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!(group_id = %group.group_id, plugin_id = %plugin_id, error = %e, "Plugin BM25 generation failed");
                    }
                }
            }
            found
        } else {
            None
        };

        let embedding_text: Option<String> = if matches!(
            mode,
            OutputMode::Embedding | OutputMode::Both
        ) {
            let mut found = None;
            for plugin in embedding_generators {
                let plugin_id = plugin.metadata().id.clone();
                match plugin.generate_embedding(group) {
                    Ok(Some(text)) => {
                        found = Some(text);
                        break;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!(group_id = %group.group_id, plugin_id = %plugin_id, error = %e, "Plugin embedding generation failed");
                    }
                }
            }
            found
        } else {
            None
        };

        if bm25_text.is_none() && embedding_text.is_none() {
            return None;
        }

        let header = group.header.as_ref()?;

        let bm25_text = bm25_text
            .map(|text| self.bm25_cleaner.clean(&text))
            .unwrap_or_default();
        let embedding_text = embedding_text
            .map(|text| self.embedding_cleaner.clean(&text))
            .unwrap_or_default();

        let result = ConversionResult::new(
            header.id,
            header.kind,
            header.name.clone(),
            file_path.to_string(),
            bm25_text,
            embedding_text,
            Vec::new(),
        );

        Some(vec![result])
    }

    /// Batch convert multiple entity groups using plugins (optimized for reduced boundary calls)
    ///
    /// This method processes multiple groups in batch to minimize Rust↔Lua boundary crossings.
    ///
    /// Groups not covered by any plugin fall back to the built-in converters
    /// individually — a single uncovered group no longer aborts the whole batch.
    fn convert_entity_groups_batch(
        &self,
        groups: &[&EntityGroup],
        file_paths: &[&str],
        request: Option<&ConversionRequest>,
    ) -> Vec<Vec<ConversionResult>> {
        // Compute header-only IDs to prevent duplicate member conversion.
        let batch_header_only_ids: HashSet<EntityId> = groups
            .iter()
            .filter_map(|g| g.header.as_ref().map(|h| h.id))
            .collect();

        let mut plugin_results: Vec<Option<Vec<ConversionResult>>> = match self.plugin_registry {
            Some(ref registry) => {
                self.try_plugin_generation_batch(groups, file_paths, request, registry)
            }
            None => vec![None; groups.len()],
        };

        groups
            .iter()
            .zip(file_paths.iter())
            .enumerate()
            .map(|(i, (group, file_path))| match plugin_results[i].take() {
                Some(conversions) => conversions,
                None => {
                    self.convert_entity_group(group, file_path, request, &batch_header_only_ids)
                }
            })
            .collect()
    }

    /// Try batch plugin generation for multiple groups
    ///
    /// Returns one `Option` per group: `Some` when plugins produced text for
    /// that group, `None` when no matching plugin covered it (the caller then
    /// falls back to the built-in converter for that group only).
    ///
    /// Groups are aggregated by (file_path, language) since plugins are
    /// filtered per file/language; each plugin's batch interface is called
    /// **once** per block with the whole group list, so Lua plugins that
    /// implement `generate_*_batch` see a real multi-group batch.
    fn try_plugin_generation_batch(
        &self,
        groups: &[&EntityGroup],
        file_paths: &[&str],
        request: Option<&ConversionRequest>,
        registry: &std::sync::Arc<crate::plugin::PluginRegistry>,
    ) -> Vec<Option<Vec<ConversionResult>>> {
        let mode = self.resolve_mode(request);
        let mut results: Vec<Option<Vec<ConversionResult>>> = vec![None; groups.len()];
        if groups.is_empty() {
            return results;
        }

        // Aggregate indices by (file_path, language).
        let mut blocks: Vec<Vec<usize>> = Vec::new();
        let mut block_keys: HashMap<String, usize> = HashMap::new();
        for (i, group) in groups.iter().enumerate() {
            let key = format!("{}\u{0}{}", file_paths[i], group.language);
            let bid = *block_keys.entry(key).or_insert_with(|| {
                blocks.push(Vec::new());
                blocks.len() - 1
            });
            blocks[bid].push(i);
        }

        for block in blocks {
            let file_path = file_paths[block[0]];
            let language_str = groups[block[0]].language.to_string();
            let language = Some(language_str.as_str());

            let mut bm25_texts: Vec<Option<String>> = vec![None; groups.len()];
            let mut embedding_texts: Vec<Option<String>> = vec![None; groups.len()];

            if matches!(mode, OutputMode::Bm25 | OutputMode::Both) {
                let mut pending: Vec<usize> = block.clone();
                // Override-tier segment only (priority >= 0); groups still
                // uncovered run the full three-tier path (override → built-in
                // → below-builtin fallback) in `convert_entity_group`.
                let (bm25_above, _) =
                    registry.get_override_bm25_generators(Some(file_path), language);
                for plugin in bm25_above {
                    if pending.is_empty() {
                        break;
                    }
                    let plugin_id = plugin.metadata().id.clone();
                    let batch: Vec<&EntityGroup> = pending.iter().map(|&i| groups[i]).collect();
                    match plugin.generate_bm25_batch(&batch) {
                        Ok(texts) => {
                            for (text, &i) in texts.into_iter().zip(pending.iter()) {
                                if let Some(t) = text {
                                    bm25_texts[i] = Some(t);
                                }
                            }
                            pending.retain(|&i| bm25_texts[i].is_none());
                        }
                        Err(e) => {
                            tracing::warn!(plugin_id = %plugin_id, error = %e, "Plugin BM25 batch generation failed");
                        }
                    }
                }
            }

            if matches!(mode, OutputMode::Embedding | OutputMode::Both) {
                let mut pending: Vec<usize> = block.clone();
                let (embedding_above, _) =
                    registry.get_override_embedding_generators(Some(file_path), language);
                for plugin in embedding_above {
                    if pending.is_empty() {
                        break;
                    }
                    let plugin_id = plugin.metadata().id.clone();
                    let batch: Vec<&EntityGroup> = pending.iter().map(|&i| groups[i]).collect();
                    match plugin.generate_embedding_batch(&batch) {
                        Ok(texts) => {
                            for (text, &i) in texts.into_iter().zip(pending.iter()) {
                                if let Some(t) = text {
                                    embedding_texts[i] = Some(t);
                                }
                            }
                            pending.retain(|&i| embedding_texts[i].is_none());
                        }
                        Err(e) => {
                            tracing::warn!(plugin_id = %plugin_id, error = %e, "Plugin embedding batch generation failed");
                        }
                    }
                }
            }

            for &i in &block {
                if bm25_texts[i].is_some() || embedding_texts[i].is_some() {
                    let bm25_text = if matches!(mode, OutputMode::Bm25 | OutputMode::Both) {
                        bm25_texts[i]
                            .as_deref()
                            .map(|text| self.bm25_cleaner.clean(text))
                    } else {
                        None
                    };
                    let embedding_text = if matches!(mode, OutputMode::Embedding | OutputMode::Both)
                    {
                        embedding_texts[i]
                            .as_deref()
                            .map(|text| self.embedding_cleaner.clean(text))
                    } else {
                        None
                    };
                    let result = ConversionResult {
                        bm25_text,
                        embedding_text,
                        ..Default::default()
                    };
                    results[i] = Some(vec![result]);
                }
            }
        }

        results
    }
}
