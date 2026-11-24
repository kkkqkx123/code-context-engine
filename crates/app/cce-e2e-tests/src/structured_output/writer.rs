//! High-level writer for structured output.
//!
//! Orchestrates `OutputManager` to emit hierarchical structured outputs:
//! summary, per-file reports, and per-directory overviews.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use cce_codegraph::RelationIndex;
use cce_codegraph::type_inference::types::ScopedTypeContext;
use cce_types::{Entity, EntityId, ParsedFile};

use crate::output_manager::{OutputCategory, OutputManager};
use crate::review_filter::ReviewFilterOptions;

use super::relations::render_relations;
use super::reports::{render_directory_report, render_file_report};
use super::summary::render_summary;
use super::symbol_table::render_symbol_table;
use super::type_inference::render_type_inference;
use super::types::ordered_files;

/// High-level writer that emits hierarchical structured outputs.
///
/// Reuses `OutputManager` for directory management so the layout stays
/// consistent with the rest of the E2E `outputs/` tree.
pub struct StructuredOutputWriter {
    manager: OutputManager,
    project_name: String,
}

impl StructuredOutputWriter {
    /// Create a writer that targets `outputs/scenarios/<language>/structured/<project>`
    /// via `OutputManager`.
    pub fn for_scenarios(language: &str, project_name: &str) -> Self {
        let manager = OutputManager::builder()
            .category(OutputCategory::Scenarios)
            .language(language)
            .scenario(format!("structured/{project_name}"))
            .build();
        Self {
            manager,
            project_name: project_name.to_string(),
        }
    }

    /// Create with a custom `OutputManager`.
    pub fn new(manager: OutputManager, project_name: impl Into<String>) -> Self {
        Self {
            manager,
            project_name: project_name.into(),
        }
    }

    /// Ensure the output directory exists and return its path.
    pub fn ensure_dir(&self) -> std::io::Result<std::path::PathBuf> {
        self.manager.ensure_output_dir()
    }

    /// Write all hierarchical files from an index + parsed files pair.
    ///
    /// Produces `SUMMARY.md` plus per-file `<path>.txt` reports (preserving the
    /// original directory tree) and per-directory `<dir>.dir.txt` overviews
    /// colocated sibling to each directory. Relations are colocated inside each
    /// per-file report rather than aggregated into a global `RELATIONS.md`.
    ///
    /// `filter` applies at write time only: the index stays exhaustive, but
    /// excluded files get no report and edges into excluded files are dropped
    /// from the retained reports (same opt-in semantics as the main query path).
    pub fn write_all(
        &self,
        index: &RelationIndex,
        files: &[ParsedFile],
        filter: &ReviewFilterOptions,
    ) -> std::io::Result<Vec<std::path::PathBuf>> {
        let base_dir = self.manager.ensure_output_dir()?;

        // Clean up legacy aggregated files and JSON left from previous runs.
        for legacy in [
            "SYMBOL_TABLE.md",
            "RELATIONS.md",
            "TYPE_INFERENCE.md",
            "symbols.json",
            "relations.json",
            "summary.json",
        ] {
            let p = base_dir.join(legacy);
            let _ = std::fs::remove_file(p);
        }

        let mut paths = Vec::new();
        let summary = render_summary(index, files, &self.project_name, filter);
        paths.push(self.manager.write("SUMMARY.md", &summary)?);

        let ordered = ordered_files(index);

        let mut pf_map: BTreeMap<String, &ParsedFile> = BTreeMap::new();
        for pf in files {
            pf_map.insert(pf.path.clone(), pf);
        }

        let mut global_id_to_entity: BTreeMap<EntityId, Entity> = BTreeMap::new();
        for ents in ordered.values() {
            for (id, e) in ents {
                global_id_to_entity.insert(*id, e.clone());
            }
        }

        let mut all_files_set: BTreeSet<String> = BTreeSet::new();
        for k in ordered.keys() {
            all_files_set.insert(k.clone());
        }
        for k in pf_map.keys() {
            all_files_set.insert(k.clone());
        }
        for path in index.file_records().read().keys() {
            all_files_set.insert(path.clone());
        }
        all_files_set.retain(|f| !filter.is_excluded_path(f));

        // Distinct directories present in the retained file set.
        let mut dirs: BTreeSet<String> = BTreeSet::new();
        for fp in &all_files_set {
            let mut p = Path::new(fp);
            while let Some(parent) = p.parent() {
                if parent == Path::new("") || parent == Path::new(".") {
                    break;
                }
                dirs.insert(parent.to_string_lossy().to_string());
                p = parent;
            }
        }
        dirs.retain(|d| !filter.is_excluded_path(d));

        // Single project-wide inference shared by the aggregated markdown and
        // every per-file report, so `.txt` and `TYPE_INFERENCE.md` agree.
        let project_contexts = crate::type_inference_assert::infer_project_contexts(files);
        let mut ctx_map: BTreeMap<String, &ScopedTypeContext> = BTreeMap::new();
        for (pf, ctx) in files.iter().zip(project_contexts.iter()) {
            ctx_map.insert(pf.path.clone(), ctx);
        }

        for file_path in &all_files_set {
            let entities = ordered.get(file_path).map(|v| v.as_slice()).unwrap_or(&[]);
            let owned: Vec<(EntityId, Entity)> = entities.to_vec();
            let pf_opt = pf_map.get(file_path).copied();
            let inferred = ctx_map.get(file_path).copied();
            let content = render_file_report(
                file_path,
                &owned,
                index,
                pf_opt,
                &global_id_to_entity,
                inferred,
                filter,
            );
            let rel_path = format!("{file_path}.txt");
            let full_path = base_dir.join(&rel_path);
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&full_path, content)?;
            paths.push(full_path);
        }

        for dir_path in &dirs {
            let content = render_directory_report(dir_path, &ordered, index);
            let rel_path = format!("{dir_path}.dir.txt");
            let full_path = base_dir.join(&rel_path);
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&full_path, content)?;
            paths.push(full_path);
        }

        Ok(paths)
    }

    /// Write symbol table only (legacy single-file aggregated).
    pub fn write_symbol_table(&self, index: &RelationIndex) -> std::io::Result<std::path::PathBuf> {
        let content = render_symbol_table(index, &self.project_name);
        self.manager.write("SYMBOL_TABLE.md", &content)
    }

    /// Write relations only (legacy single-file aggregated).
    pub fn write_relations(&self, index: &RelationIndex) -> std::io::Result<std::path::PathBuf> {
        let content = render_relations(index, &self.project_name);
        self.manager.write("RELATIONS.md", &content)
    }

    /// Write type inference only (from parsed files, legacy aggregated).
    pub fn write_type_inference(
        &self,
        files: &[ParsedFile],
    ) -> std::io::Result<std::path::PathBuf> {
        let content = render_type_inference(files, &self.project_name);
        self.manager.write("TYPE_INFERENCE.md", &content)
    }
}
