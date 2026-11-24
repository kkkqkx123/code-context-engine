use std::path::PathBuf;

use cce_e2e_tests::bench_data::{compute_bm25_scores, cosine_similarity, load_benchmark_data};
use cce_e2e_tests::judgments::evaluate::BASELINES;

const FIXTURE: &str = "once_cell";
const MODEL: &str = "bge-m3";

fn data_dir(baseline: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("benchmark")
        .join(baseline)
        .join(FIXTURE)
        .join(MODEL)
        .join("bench_data.rkyv")
}

fn output_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("outputs")
        .join("debug")
}

fn is_relevant(entity_name: &str, relevant_names: &[String]) -> bool {
    relevant_names.iter().any(|rn| {
        entity_name == rn
            || entity_name.ends_with(&format!("::{}", rn))
            || rn.ends_with(&format!("::{}", entity_name))
    })
}

fn main() {
    let out_root = output_dir();
    if out_root.exists() {
        std::fs::remove_dir_all(&out_root).expect("Failed to clean outputs/debug");
    }

    for baseline in BASELINES {
        let path = data_dir(baseline);
        if !path.exists() {
            eprintln!("SKIP {}: data not found at {}", baseline, path.display());
            continue;
        }

        let bench = match load_benchmark_data(&path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("ERROR loading {}: {}", path.display(), e);
                continue;
            }
        };

        let baseline_dir = out_root.join(baseline);
        std::fs::create_dir_all(&baseline_dir).expect("Failed to create baseline dir");

        // Use embedding chunks as reference (both paths share same entity structure for most baselines)
        let emb_chunk_names: Vec<String> = bench
            .embedding
            .chunks
            .iter()
            .map(|c| c.entity_name.clone())
            .collect();
        let bm25_chunk_names: Vec<String> = bench
            .bm25
            .chunks
            .iter()
            .map(|c| c.entity_name.clone())
            .collect();

        let emb_dim = bench.embedding.dimension as usize;
        let emb_n_chunks = bench.embedding.chunks.len();
        let n_queries = bench.queries.len();

        // Compute BM25 scores
        let bm25_all = if !bench.bm25_documents.is_empty() {
            compute_bm25_scores(
                &bench.bm25_documents,
                &bench.query_texts,
                cce_storage_bm25::TermOperator::Or,
            )
        } else {
            vec![]
        };

        let mut txt = String::new();
        txt.push_str(&format!("Baseline: {}\n", baseline));
        txt.push_str(&format!(
            "Embedding chunks: {}  BM25 chunks: {}  Queries: {}\n\n",
            emb_n_chunks,
            bench.bm25.chunks.len(),
            n_queries
        ));
        txt.push_str("Retriever comparison: BGE-M3 (cosine)  vs  BM25 (Okapi)\n");
        txt.push_str("Chunks marked with * are relevant to the query.\n");
        txt.push_str("Chunks marked with - are from distractor fixture (negative samples).\n\n");

        for q_idx in 0..n_queries {
            let query = &bench.queries[q_idx];

            // BGE-M3 scores
            let q_start = q_idx * emb_dim;
            let q_end = q_start + emb_dim;
            let query_vec = &bench.embedding.query_vectors[q_start..q_end];

            let mut bge_scores: Vec<(usize, f64)> = (0..emb_n_chunks)
                .map(|c_idx| {
                    let c_start = c_idx * emb_dim;
                    let c_end = c_start + emb_dim;
                    let chunk_vec = &bench.embedding.vectors[c_start..c_end];
                    (c_idx, cosine_similarity(chunk_vec, query_vec))
                })
                .collect();
            bge_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            // BM25 scores (may have different chunk count)
            let bm25_n_chunks = bench.bm25.chunks.len();
            let mut bm25_scores: Vec<(usize, f64)> = if !bm25_all.is_empty() {
                bm25_all[q_idx]
                    .iter()
                    .enumerate()
                    .map(|(c_idx, s)| (c_idx, *s))
                    .collect()
            } else {
                vec![]
            };
            bm25_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            let top_k = 10_usize.min(emb_n_chunks);

            txt.push_str(&format!(
                "--- [{}] \"{}\" (type={}) ---\n",
                query.id, query.text, query.query_type
            ));
            txt.push_str(&format!("  Relevant: {:?}\n\n", query.relevant_names));

            txt.push_str(&format!(
                "  {:>4}  {:>10}  {:>10}  {}\n",
                "rank", "bge-m3", "bm25", "chunk"
            ));
            txt.push_str(&format!(
                "  {:-<4}  {:-<10}  {:-<10}  {:-<20}\n",
                "", "", "", ""
            ));

            for (rank, (c_idx, bge_score)) in bge_scores.iter().take(top_k).enumerate() {
                // BM25 score for this embedding chunk (same index if chunk counts match)
                let bm25 = if emb_n_chunks == bm25_n_chunks && *c_idx < bm25_n_chunks {
                    bm25_scores
                        .iter()
                        .find(|(i, _)| *i == *c_idx)
                        .map(|(_, s)| *s)
                        .unwrap_or(0.0)
                } else {
                    bm25_scores
                        .iter()
                        .find(|(i, _)| *i == *c_idx)
                        .map(|(_, s)| *s)
                        .unwrap_or(0.0)
                };
                let marker = if is_relevant(&emb_chunk_names[*c_idx], &query.relevant_names) {
                    "*"
                } else {
                    " "
                };
                txt.push_str(&format!(
                    "  {:>4}  {:>10.6}  {:>10.6}  {}{}\n",
                    rank + 1,
                    bge_score,
                    bm25,
                    marker,
                    emb_chunk_names[*c_idx]
                ));
            }

            // Show relevant chunks with their ranks across both retrievers
            txt.push_str("\n  Relevant chunk ranks:\n");
            struct RelRank<'a> {
                name: &'a str,
                bge_pos: Option<usize>,
                bge_score: f64,
                bm25_pos: Option<usize>,
                bm25_score: f64,
            }
            let mut relevant_info: Vec<RelRank<'_>> = Vec::new();
            for name in &query.relevant_names {
                let bge_pos = bge_scores.iter().position(|(i, _)| {
                    is_relevant(&emb_chunk_names[*i], std::slice::from_ref(name))
                });
                let bm25_pos = bm25_scores.iter().position(|(i, _)| {
                    is_relevant(&bm25_chunk_names[*i], std::slice::from_ref(name))
                });

                let bge_score = bge_pos.map(|p| bge_scores[p].1).unwrap_or(0.0);
                let bm25_score = bm25_pos.map(|p| bm25_scores[p].1).unwrap_or(0.0);

                relevant_info.push(RelRank {
                    name,
                    bge_pos,
                    bge_score,
                    bm25_pos,
                    bm25_score,
                });
            }
            relevant_info.sort_by_key(|r| r.bge_pos.unwrap_or(usize::MAX));

            txt.push_str(&format!(
                "  {:>20}  {:>8}  {:>10}  {:>8}  {:>10}\n",
                "entity", "bge_rank", "bge_score", "bm25_rank", "bm25_score"
            ));
            for r in &relevant_info {
                let bge_r = r
                    .bge_pos
                    .map(|pos| (pos + 1).to_string())
                    .unwrap_or_else(|| "N/A".into());
                let bm25_r = r
                    .bm25_pos
                    .map(|pos| (pos + 1).to_string())
                    .unwrap_or_else(|| "N/A".into());
                txt.push_str(&format!(
                    "  {:>20}  {:>8}  {:>10.6}  {:>8}  {:>10.6}\n",
                    r.name, bge_r, r.bge_score, bm25_r, r.bm25_score
                ));
            }
            txt.push('\n');
        }

        std::fs::write(baseline_dir.join("comparison.txt"), &txt).ok();
        eprintln!("WROTE {}", baseline);
    }

    eprintln!("\n=== All comparison reports written to {:?} ===", out_root);
}
