//! Pipeline debug utilities for displaying intermediate parsing results.
//!
//! These renderers produce human-readable text snapshots of each pipeline
//! stage: entity extraction, grouping, chunking, and overall file summary.
//! They are designed for manual debugging and visual inspection of index
//! quality.

use std::collections::HashMap;
use std::fmt::Write as _;

use cce_parser::ast_to_nl::chunker::ChunkedResult;
use cce_parser::grouper::ProcessingResult;
use cce_types::{Entity, ParsedFile};

/// Debug exporter for pipeline intermediate results.
pub struct PipelineDebugExporter;

impl PipelineDebugExporter {
    /// Render the entity tree from a parsed file.
    ///
    /// Shows the semantic parent/children hierarchy with depth, kind,
    /// signature, and source span.
    pub fn render_entity_tree(parsed_file: &ParsedFile) -> String {
        let mut out = String::new();
        out.push_str("# Entity Tree\n\n");

        if parsed_file.entities.is_empty() {
            out.push_str("(no entities)\n");
            return out;
        }

        let root_entities: Vec<&Entity> = parsed_file
            .entities
            .iter()
            .filter(|e| e.parent.is_none())
            .collect();

        if root_entities.is_empty() {
            out.push_str("(no root entities — all have parents)\n\n");
            for entity in &parsed_file.entities {
                let _ = writeln!(
                    out,
                    "  id={} | depth={} | kind={} | name={}",
                    entity.id, entity.depth, entity.kind, entity.name
                );
            }
            return out;
        }

        let mut sorted_roots: Vec<&Entity> = root_entities;
        sorted_roots.sort_by_key(|e| e.span.start_position.row);

        for root in sorted_roots {
            Self::render_node(&mut out, root, &parsed_file.entities, 0);
        }

        out
    }

    fn render_node(out: &mut String, entity: &Entity, all_entities: &[Entity], depth: usize) {
        let indent = "  ".repeat(depth);
        let line = format!(
            "lines {}-{}",
            entity.span.start_position.row + 1,
            entity.span.end_position.row + 1
        );

        let signature = if entity.signature.is_empty() {
            String::new()
        } else {
            format!(" | signature=\"{}\"", entity.signature)
        };
        let doc = entity
            .doc_comment
            .as_ref()
            .map(|d| {
                let trimmed = d.trim();
                if trimmed.len() > 60 {
                    format!("{}...", cce_utils::truncate_to_byte_len(trimmed, 57))
                } else {
                    trimmed.to_string()
                }
            })
            .filter(|d| !d.is_empty())
            .map(|d| format!(" | doc=\"{}\"", d))
            .unwrap_or_default();

        let _ = writeln!(
            out,
            "{}id={} | depth={} | kind={} | name={} | {}{}{}",
            indent, entity.id, entity.depth, entity.kind, entity.name, line, signature, doc
        );

        let mut children: Vec<&Entity> = all_entities
            .iter()
            .filter(|e| e.parent == Some(entity.id))
            .collect();
        children.sort_by_key(|e| e.span.start_position.row);

        for child in children {
            Self::render_node(out, child, all_entities, depth + 1);
        }
    }

    /// Render a pipeline summary for one file.
    ///
    /// Aggregates entity counts, grouping stats, chunk per-path counts,
    /// and sidecar fact counts into a compact debug view.
    pub fn render_pipeline_summary(
        parsed_file: &ParsedFile,
        processing_result: &ProcessingResult,
        emb_chunks: &[ChunkedResult],
        bm25_chunks: &[ChunkedResult],
    ) -> String {
        let entity_count = parsed_file.entities.len();
        let top_level = parsed_file.entities.iter().filter(|e| e.depth == 0).count();
        let max_depth = parsed_file
            .entities
            .iter()
            .map(|e| e.depth)
            .max()
            .unwrap_or(0);

        let group_count = processing_result.groups.len();
        let stats = &processing_result.stats;

        let emb_count = Self::count_chunks_by_path(emb_chunks);
        let bm25_count = Self::count_chunks_by_path(bm25_chunks);

        let behavior_info = if processing_result.behavior.is_empty() {
            "none".to_string()
        } else {
            let entity_count_with_facts = parsed_file
                .entities
                .iter()
                .filter(|e| {
                    processing_result
                        .behavior
                        .get(e.id)
                        .is_some_and(|b| !b.is_empty())
                })
                .count();
            format!("{} entities have facts", entity_count_with_facts)
        };
        let cf_info = if processing_result.control_flow.is_empty() {
            "none".to_string()
        } else {
            let entity_count_with_cf = parsed_file
                .entities
                .iter()
                .filter(|e| {
                    processing_result
                        .control_flow
                        .get(e.id)
                        .is_some_and(|c| !c.is_empty())
                })
                .count();
            format!("{} entities have facts", entity_count_with_cf)
        };

        let mut out = String::new();
        out.push_str("# Pipeline Summary\n\n");

        let _ = writeln!(
            out,
            "Entities: {} ({} top-level, max_depth={})",
            entity_count, top_level, max_depth
        );
        let _ = writeln!(
            out,
            "Groups: {} (input_entities={}, output_groups={}, standalone={})",
            group_count, stats.input_entities, stats.output_groups, stats.standalone_entities
        );
        let _ = writeln!(
            out,
            "Chunks (Embedding): {} (in {} groups)",
            emb_count.0, emb_count.1
        );
        let _ = writeln!(
            out,
            "Chunks (BM25): {} (in {} groups)",
            bm25_count.0, bm25_count.1
        );
        let _ = writeln!(
            out,
            "Sidecar: behavior={} | control_flow={}",
            behavior_info, cf_info
        );

        out
    }

    fn count_chunks_by_path(chunks: &[ChunkedResult]) -> (usize, usize) {
        let total = chunks.len();
        let mut groups = std::collections::BTreeSet::new();
        for c in chunks {
            groups.insert(c.source_group_id.clone());
        }
        (total, groups.len())
    }

    /// Render group-by-group overview with header entity, member count and type.
    pub fn render_groups(processing_result: &ProcessingResult) -> String {
        let mut out = String::new();
        out.push_str("# Groups\n\n");

        if processing_result.groups.is_empty() {
            out.push_str("(no groups)\n");
            return out;
        }

        for (idx, group) in processing_result.groups.iter().enumerate() {
            let header_name = group
                .header
                .as_ref()
                .map(|h| h.name.clone())
                .unwrap_or_else(|| "(no header)".to_string());
            let header_kind = group
                .header
                .as_ref()
                .map(|h| h.kind.to_string())
                .unwrap_or_default();

            let _ = writeln!(
                out,
                "{:>2}. id={} | type={} | name={} | kind={} | members={} | nested={}",
                idx + 1,
                group.group_id,
                group.group_type,
                header_name,
                header_kind,
                group.members.len(),
                group.nested_groups.len(),
            );

            for (midx, member) in group.members.iter().enumerate() {
                let sig = if member.signature.is_empty() {
                    String::new()
                } else {
                    format!(" | sig=\"{}\"", member.signature)
                };
                let _ = writeln!(
                    out,
                    "     [{:>2}] id={} | kind={} | name={}{}",
                    midx + 1,
                    member.id,
                    member.kind,
                    member.name,
                    sig
                );
            }

            if !group.nested_groups.is_empty() {
                out.push_str("     nested_groups:\n");
                for (nidx, ng) in group.nested_groups.iter().enumerate() {
                    let _ = writeln!(
                        out,
                        "       [{:>2}] id={} | type={} | name={} | members={}",
                        nidx + 1,
                        ng.group_id,
                        ng.group_type,
                        ng.name,
                        ng.members.len(),
                    );
                }
            }

            out.push('\n');
        }

        out
    }

    /// Render chunk alignment between Embedding and BM25 paths.
    ///
    /// Groups chunks by source_group_id and shows both paths side-by-side.
    pub fn render_chunk_alignment(
        emb_chunks: &[ChunkedResult],
        bm25_chunks: &[ChunkedResult],
    ) -> String {
        let mut out = String::new();
        out.push_str("# Chunk Alignment\n\n");
        out.push_str("Groups chunks by source_group_id and aligns Embedding vs BM25 paths.\n\n");

        let emb_by_group: HashMap<&str, Vec<&ChunkedResult>> =
            Self::group_chunks_by_source(emb_chunks);
        let bm25_by_group: HashMap<&str, Vec<&ChunkedResult>> =
            Self::group_chunks_by_source(bm25_chunks);

        let mut all_group_ids: Vec<&str> = emb_by_group
            .keys()
            .chain(bm25_by_group.keys())
            .copied()
            .collect();
        all_group_ids.sort();
        all_group_ids.dedup();

        if all_group_ids.is_empty() {
            out.push_str("(no chunks)\n");
            return out;
        }

        for group_id in &all_group_ids {
            let emb = emb_by_group
                .get(group_id)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let bm25 = bm25_by_group
                .get(group_id)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);

            let label = if emb.is_empty() && !bm25.is_empty() {
                "BM25 only"
            } else if bm25.is_empty() && !emb.is_empty() {
                "Embedding only"
            } else {
                "both"
            };

            let group_type = emb
                .first()
                .or_else(|| bm25.first())
                .map(|c| c.group_type.to_string())
                .unwrap_or_default();

            let _ = writeln!(out, "Group: {} (type={}, {})", group_id, group_type, label);
            let _ = writeln!(
                out,
                "  Embedding: {} chunk(s) | BM25: {} chunk(s)",
                emb.len(),
                bm25.len()
            );

            let max_chunks = emb.len().max(bm25.len());
            for i in 0..max_chunks {
                let emb_text = emb.get(i).map(|c| c.text.trim()).unwrap_or("");
                let emb_tokens = emb.get(i).map(|c| c.token_count).unwrap_or(0);
                let bm25_text = bm25.get(i).map(|c| c.text.trim()).unwrap_or("");
                let bm25_keywords = bm25
                    .get(i)
                    .map(|c| c.bm25_keywords.join(", "))
                    .unwrap_or_default();

                let _ = writeln!(out, "  [{:>2}]", i);
                if !emb_text.is_empty() {
                    let truncated = Self::truncate_line(emb_text, 100);
                    let _ = writeln!(out, "       emb: {} ({} tokens)", truncated, emb_tokens);
                }
                if !bm25_text.is_empty() {
                    let truncated = Self::truncate_line(bm25_text, 100);
                    let kw = if bm25_keywords.is_empty() {
                        String::new()
                    } else {
                        format!(" [keywords: {}]", bm25_keywords)
                    };
                    let _ = writeln!(out, "       bm25: {}{}", truncated, kw);
                }
            }
            out.push('\n');
        }

        out
    }

    fn group_chunks_by_source(chunks: &[ChunkedResult]) -> HashMap<&str, Vec<&ChunkedResult>> {
        let mut map: HashMap<&str, Vec<&ChunkedResult>> = HashMap::new();
        for c in chunks {
            map.entry(c.source_group_id.as_str()).or_default().push(c);
        }
        map
    }

    fn truncate_line(text: &str, max: usize) -> String {
        let line = text.lines().next().unwrap_or(text);
        if line.len() > max {
            format!(
                "{}...",
                cce_utils::truncate_to_byte_len(line, max.saturating_sub(3))
            )
        } else {
            line.to_string()
        }
    }
}
