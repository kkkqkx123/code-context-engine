use std::path::PathBuf;

use cce_e2e_tests::bench_data::{compute_bm25_scores, cosine_similarity, load_benchmark_data};
use cce_e2e_tests::judgments::evaluate::BASELINES;
use cce_storage_bm25::TermOperator;

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

fn ranked_dir(baseline: &str, retriever: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("outputs")
        .join("benchmark")
        .join("results")
        .join(baseline)
        .join(retriever)
        .join("ranked")
}

fn top_k_scores(scores: &[f64], k: usize) -> Vec<(usize, f64)> {
    let mut indices: Vec<usize> = (0..scores.len()).collect();
    indices.sort_by(|a, b| {
        scores[*b]
            .partial_cmp(&scores[*a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    indices.truncate(k.min(indices.len()));
    indices.into_iter().map(|i| (i, scores[i])).collect()
}

fn is_relevant(entity_name: &str, relevant_names: &[String]) -> bool {
    relevant_names.iter().any(|rn| {
        entity_name == rn
            || entity_name.ends_with(&format!("::{}", rn))
            || rn.ends_with(&format!("::{}", entity_name))
    })
}

fn format_ranked_output(
    query: &cce_e2e_tests::bench_data::QueryData,
    chunk_names: &[String],
    chunk_texts: &[String],
    ranked: &[(usize, f64)],
) -> String {
    let mut txt = format!(
        "Query: {} | \"{}\" | type={}\nRelevant names: {:?}\n\n",
        query.id, query.text, query.query_type, query.relevant_names
    );

    txt.push_str(&format!(
        "{:>4}  {:>12}  {:>6}  {:>6}  chunk_text[..200]\n",
        "rank", "score", "match", "neg?"
    ));
    txt.push_str(&format!(
        "{:-<4}  {:-<12}  {:-<6}  {:-<6}  {:-<60}\n",
        "", "", "", "", ""
    ));

    for (rank, (c_idx, score)) in ranked.iter().enumerate() {
        let matched = if is_relevant(&chunk_names[*c_idx], &query.relevant_names) {
            "REL"
        } else {
            ""
        };
        let neg = if is_relevant(&chunk_names[*c_idx], &query.irrelevant_names) {
            "NEG"
        } else {
            ""
        };

        let content = chunk_texts[*c_idx].as_str();
        let preview: &str = if content.len() > 200 {
            cce_utils::truncate_to_byte_len(content, 200)
        } else {
            content
        };

        txt.push_str(&format!(
            "{:>4}  {:>12.6}  {:>6}  {:>6}  {}\n",
            rank + 1,
            score,
            matched,
            neg,
            preview.replace('\n', "\\n")
        ));
    }
    txt
}

fn main() {
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

        let top_k = 20;

        // ---- BGE-M3 ranked dump (embedding path) ----
        let bge_dir = ranked_dir(baseline, "bge-m3");
        std::fs::create_dir_all(&bge_dir).expect("Failed to create bge-m3 ranked dir");

        let emb_dim = bench.embedding.dimension as usize;
        let emb_n_chunks = bench.embedding.chunks.len();
        let emb_chunk_names: Vec<String> = bench
            .embedding
            .chunks
            .iter()
            .map(|c| c.entity_name.clone())
            .collect();

        for (q_idx, query) in bench.queries.iter().enumerate() {
            let q_start = q_idx * emb_dim;
            let q_end = q_start + emb_dim;
            let query_vec = &bench.embedding.query_vectors[q_start..q_end];

            let mut scores = Vec::with_capacity(emb_n_chunks);
            for c_idx in 0..emb_n_chunks {
                let c_start = c_idx * emb_dim;
                let c_end = c_start + emb_dim;
                let chunk_vec = &bench.embedding.vectors[c_start..c_end];
                scores.push(cosine_similarity(chunk_vec, query_vec));
            }

            let ranked = top_k_scores(&scores, top_k);
            let txt =
                format_ranked_output(query, &emb_chunk_names, &bench.embedding.texts, &ranked);
            std::fs::write(bge_dir.join(format!("{}.txt", query.id)), &txt).ok();
        }

        // ---- BM25 ranked dump ----
        let bm25_dir = ranked_dir(baseline, "bm25");
        std::fs::create_dir_all(&bm25_dir).expect("Failed to create bm25 ranked dir");

        if !bench.bm25.texts.is_empty() {
            let bm25_chunk_names: Vec<String> = bench
                .bm25
                .chunks
                .iter()
                .map(|c| c.entity_name.clone())
                .collect();
            let bm25_scores =
                compute_bm25_scores(&bench.bm25_documents, &bench.query_texts, TermOperator::Or);

            for (q_idx, query) in bench.queries.iter().enumerate() {
                let ranked = top_k_scores(&bm25_scores[q_idx], top_k);
                let txt =
                    format_ranked_output(query, &bm25_chunk_names, &bench.bm25.texts, &ranked);
                std::fs::write(bm25_dir.join(format!("{}.txt", query.id)), &txt).ok();
            }
        }

        eprintln!("WROTE {}", baseline);
    }

    eprintln!(
        "\n=== All ranked dumps in outputs/benchmark/results/{{baseline}}/{{retriever}}/ranked/ ==="
    );
}
