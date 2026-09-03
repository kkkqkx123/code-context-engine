use crate::merge::Mergeable;
use crate::project::ProjectAppConfig;

use super::AppConfig;

impl AppConfig {
    /// Merge with project-level configuration
    ///
    /// This method creates a new configuration by merging the global config
    /// with project-specific overrides. Only the fields present in the project
    /// config are overridden; all other fields retain their global values.
    ///
    /// # Design Principles
    ///
    /// - Project config can override: scanner, grouper, orchestrator, relation, ast_to_nl, summary, embedder (model only), llm (chat/rerank models)
    /// - Project config CANNOT override: server, database, logger, export
    /// - Embedder override is limited to model selection and preprocessing; API keys and base_url remain from global
    /// - LLM override allows projects to specify different chat/rerank models while keeping provider credentials global
    /// - Sensitive settings (API keys, URLs) remain in global config
    pub fn merge_with_project(&self, project: &ProjectAppConfig) -> AppConfig {
        let mut merged = self.clone();

        // --- Embedder: single-field override ---
        if let Some(ref project_embedder) = project.embedder {
            if let Some(ref model) = project_embedder.model {
                merged.embedder.default_model = model.clone();
            }
        }

        // --- LLM: field-by-field override ---
        if let Some(ref project_llm) = project.llm {
            merged.llm.defaults.chat.merge(&project_llm.chat_model);
            merged.llm.defaults.rerank.merge(&project_llm.rerank_model);
            if let Some(enable) = project_llm.enable_rerank {
                merged.rerank.enabled = enable;
            }
            if let Some(max) = project_llm.rerank_max_candidates {
                merged.rerank.max_candidates = max;
            }
        }

        // --- Whole-section replacement ---
        if let Some(ref scanner) = project.scanner {
            merged.scanner = scanner.clone();
        }
        if let Some(ref grouper) = project.grouper {
            merged.grouper = grouper.clone();
        }
        if let Some(ref relation) = project.relation {
            merged.relation = relation.clone();
        }
        if let Some(ref ast_to_nl) = project.ast_to_nl {
            merged.ast_to_nl = ast_to_nl.clone();
        }
        if let Some(ref summary) = project.summary {
            merged.summary = summary.clone();
        }

        // --- Orchestrator: deep merge via Mergeable trait ---
        if let Some(ref project_orchestrator) = project.orchestrator {
            merged.orchestrator.merge(project_orchestrator);
        }

        // --- Storage: complex merge (unchanged, too specialized for generic trait) ---
        merge_storage(&mut merged, project);

        merged
    }
}

/// Storage merge helper — kept separate because Qdrant uses patch/apply patterns.
fn merge_storage(merged: &mut AppConfig, project: &ProjectAppConfig) {
    if let Some(ref project_storage) = project.storage {
        // Qdrant
        if let Some(ref project_qdrant) = project_storage.qdrant {
            if let Some(preset) = project_qdrant.preset {
                merged.database.qdrant.preset = preset;
            }
            if let Some(ref hnsw_patch) = project_qdrant.hnsw {
                let base = merged
                    .database
                    .qdrant
                    .hnsw
                    .clone()
                    .or_else(|| merged.database.qdrant.preset.hnsw_config())
                    .unwrap_or_default();
                merged.database.qdrant.hnsw = Some(base.apply(hnsw_patch));
            }
            if let Some(ref vs_patch) = project_qdrant.vector_storage {
                let base = merged
                    .database
                    .qdrant
                    .vector_storage
                    .clone()
                    .unwrap_or_default();
                merged.database.qdrant.vector_storage = Some(base.apply(vs_patch));
            }
            if let Some(ref quant) = project_qdrant.quantization {
                merged.database.qdrant.quantization = Some(quant.clone());
            }
            if let Some(ref wal_patch) = project_qdrant.wal {
                let base = merged
                    .database
                    .qdrant
                    .wal
                    .clone()
                    .unwrap_or_else(|| merged.database.qdrant.preset.wal_config());
                merged.database.qdrant.wal = Some(base.apply(wal_patch));
            }
        }
        // BM25
        if let Some(ref project_bm25) = project_storage.bm25 {
            if let Some(enabled) = project_bm25.enabled {
                merged.database.bm25.enabled = enabled;
            }
            if let Some(ref index_path) = project_bm25.index_path {
                merged.database.bm25.index_path = Some(index_path.clone());
            }
            if let Some(ref algorithm) = project_bm25.algorithm {
                merged.database.bm25.algorithm = algorithm.clone();
            }
        }
        // IndexManager
        if let Some(ref index_manager) = project_storage.index_manager {
            merged.database.bm25.index_manager.writer_memory_budget =
                index_manager.writer_memory_budget;
            if let Some(threads) = index_manager.writer_num_threads {
                merged.database.bm25.index_manager.writer_num_threads = Some(threads);
            }
            merged.database.bm25.index_manager.reload_policy = index_manager.reload_policy.clone();
        }
    }
}
