//! Generate rerank sidecars for the once_cell fixture.
//!
//! Reads `data/benchmark/{baseline}/once_cell/bge-m3/bench_data.rkyv` and
//! calls the real rerank model from `[llm.rerank_models]` (see workspace
//! `config.toml` + `.env`) once per (baseline, method, query). Scores land in
//! `rerank_{model}_{method}_depth{depth}.rkyv` next to each `bench_data.rkyv`;
//! the offline benchmark consumes only the sidecars.

use std::path::PathBuf;

use cce_config::ConfigLoader;
use cce_config::modules::RerankMode;
use cce_e2e_tests::rerank_benchmark::{
    RERANK_CANDIDATE_DEPTH, RerankRuntime, RerankTextSource, generate_all_sidecars,
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let root = workspace_root();
    let _ = dotenvy::from_path(root.join(".env"));
    cce_e2e_tests::init_minimal_logging();

    let config = ConfigLoader::new()
        .with_path(root.join("config.toml"))
        .load()
        .map_err(|e| anyhow::anyhow!("failed to load workspace config: {e:?}"))?;
    let model_key = config.rerank.model.clone();
    let model_config = config.llm.rerank_models.get(&model_key).ok_or_else(|| {
        anyhow::anyhow!("rerank model '{model_key}' not found in llm.rerank_models")
    })?;
    if model_config.mode != RerankMode::CrossEncoder {
        anyhow::bail!(
            "gen_rerank_oncecell targets the cross-encoder endpoint; '{model_key}' uses {:?}",
            model_config.mode
        );
    }

    let provider = cce_llm_client::build_cohere_rerank_provider(&config, &model_key)
        .map_err(|e| anyhow::anyhow!("failed to build rerank client: {e:?}"))?;
    println!(
        "Rerank model: {model_key} -> {} (cross-encoder)",
        model_config.model
    );

    let text_source = match std::env::args().nth(1).as_deref() {
        None | Some("emb-text") => RerankTextSource::EmbText,
        Some("raw-code") => RerankTextSource::RawCode,
        Some(other) => anyhow::bail!("unknown text source '{other}': expected emb-text|raw-code"),
    };
    let runtime = RerankRuntime {
        model_key,
        model_name: model_config.model.clone(),
        mode: "cross_encoder".to_string(),
        depth: RERANK_CANDIDATE_DEPTH,
        fusion: config.rerank.score_fusion_strategy,
        timeout_ms: config.rerank.timeout_ms,
        text_source,
    };
    let written = generate_all_sidecars("once_cell", &provider, &runtime)
        .await
        .map_err(|e| anyhow::anyhow!("rerank sidecar generation failed: {e}"))?;
    println!("\n✓ Wrote {} sidecar files", written.len());
    Ok(())
}
