//! Rerank benchmark for the once_cell fixture.
//!
//! Fully offline: reads `data/benchmark/{baseline}/once_cell/bge-m3/`
//! (`bench_data.rkyv` + `rerank_*.rkyv` sidecars), fuses stored rerank scores
//! with recall scores, and writes reports to
//! `outputs/benchmark/once_cell/rerank_{variant}/`. No model calls.
//!
//! Usage:
//! - `benchmark_rerank_oncecell` — config fusion from `config.toml`
//! - `benchmark_rerank_oncecell rerank_only` — pure cross-encoder order
//! - `benchmark_rerank_oncecell multiplicative` — product fusion
//! - `benchmark_rerank_oncecell linear_weighted --normalize-initial` —
//!   config linear blend over per-query min-max normalized initial scores

use std::path::PathBuf;

use cce_config::ConfigLoader;
use cce_config::modules::search::RerankFusionStrategy;
use cce_e2e_tests::bench_data::EvaluationScope;
use cce_e2e_tests::judgments::once_cell::once_cell_relevance_judgments;
use cce_e2e_tests::rerank_benchmark::{
    RERANK_CANDIDATE_DEPTH, RerankScoring, RerankTextSource, run_rerank_benchmark,
};

fn workspace_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join("config.toml").exists() {
            return dir;
        }
        if !dir.pop() {
            panic!("workspace root with config.toml not found");
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let all_judgments = once_cell_relevance_judgments();
    let scope = EvaluationScope::CoreRetrieval;
    let judgments: Vec<_> = all_judgments
        .iter()
        .filter(|j| scope.includes_query_type(j.query_type))
        .cloned()
        .collect();

    println!("=== Rerank Benchmark Configuration ===");
    println!("Evaluation scope: {scope}");
    println!(
        "Total judgments: {} ({} excluded)",
        judgments.len(),
        all_judgments.len() - judgments.len()
    );

    let root = workspace_root();
    let _ = dotenvy::from_path(root.join(".env"));
    let config = ConfigLoader::new()
        .with_path(root.join("config.toml"))
        .load()
        .map_err(|e| format!("failed to load workspace config: {e:?}"))?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let fusion = match args
        .iter()
        .find(|arg| !arg.starts_with("--"))
        .map(String::as_str)
    {
        None | Some("config") => config.rerank.score_fusion_strategy,
        Some("rerank_only") => RerankFusionStrategy::RerankOnly,
        Some("multiplicative") => RerankFusionStrategy::Multiplicative,
        Some("linear_weighted") => RerankFusionStrategy::LinearWeighted { alpha: 0.7 },
        Some(other) => {
            return Err(format!(
                "unknown fusion '{other}': expected config|rerank_only|multiplicative|linear_weighted"
            )
            .into());
        }
    };
    let normalize_initial = args.iter().any(|arg| arg == "--normalize-initial");
    let text_source = match args
        .iter()
        .find(|arg| arg.starts_with("--text-source="))
        .map(|arg| arg.trim_start_matches("--text-source=").to_string())
        .as_deref()
    {
        None | Some("emb-text") => RerankTextSource::EmbText,
        Some("raw-code") => RerankTextSource::RawCode,
        Some(other) => {
            return Err(
                format!("unknown text source '{other}': expected emb-text|raw-code").into(),
            );
        }
    };
    let scoring = RerankScoring {
        model_key: config.rerank.model.clone(),
        depth: RERANK_CANDIDATE_DEPTH,
        fusion,
        normalize_initial,
        text_source,
    };
    println!(
        "Sidecars: model={} depth={} fusion={} output=rerank_{}",
        scoring.model_key,
        scoring.depth,
        scoring.fusion_label(),
        scoring.variant_dir(),
    );

    run_rerank_benchmark("once_cell", &judgments, &scoring)?;
    Ok(())
}
