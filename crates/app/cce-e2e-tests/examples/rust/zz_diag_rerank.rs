//! Temporary diagnostic: dump exact rerank inputs for one query.
use cce_e2e_tests::bench_data::BenchmarkData;
use cce_e2e_tests::rerank_benchmark::{
    RERANK_CANDIDATE_DEPTH, RERANK_MODEL_KEY, RerankTextSource, decode_sidecar, sidecar_path,
};

fn main() {
    let baseline = std::env::args().nth(1).unwrap_or("full_pipeline".into());
    let text_source = match std::env::args().nth(2).as_deref() {
        None | Some("emb-text") => RerankTextSource::EmbText,
        Some("raw-code") => RerankTextSource::RawCode,
        Some(other) => panic!("unknown text source '{other}': expected emb-text|raw-code"),
    };
    let qid = std::env::args()
        .nth(3)
        .unwrap_or("FZ-G1Q1-naming_case".into());
    let sidecar_file = sidecar_path(
        "once_cell",
        &baseline,
        "emb",
        text_source,
        RERANK_MODEL_KEY,
        RERANK_CANDIDATE_DEPTH,
    );
    let bench: BenchmarkData = rkyv::from_bytes::<BenchmarkData, rkyv::rancor::Error>(
        &std::fs::read(
            sidecar_file
                .parent()
                .expect("sidecar parent")
                .join("bench_data.rkyv"),
        )
        .expect("bench"),
    )
    .expect("decode bench");
    let sidecar =
        decode_sidecar(&std::fs::read(&sidecar_file).expect("sidecar")).expect("decode sidecar");
    let qi = bench
        .queries
        .iter()
        .position(|q| q.id == qid)
        .expect("query");
    println!(
        "QUERY [{qid}] (text_source={}): {}",
        text_source.label(),
        bench.query_texts.get(qi).map(String::as_str).unwrap_or("?")
    );
    let outcome = sidecar
        .queries
        .iter()
        .find(|o| o.query_id == qid)
        .expect("outcome");
    let judgment = cce_e2e_tests::judgments::once_cell::once_cell_relevance_judgments()
        .into_iter()
        .find(|j| j.id == qid)
        .expect("judgment");
    println!("JUDGMENT: {:?}", judgment.relevant_ranges);
    for c in outcome.candidates.iter().take(10) {
        let chunk_idx = bench
            .embedding
            .chunks
            .iter()
            .position(|ch| ch.chunk_id == c.chunk_id);
        let text = chunk_idx
            .and_then(|idx| text_source.resolve(&bench, idx))
            .unwrap_or("?".into());
        let loc = chunk_idx
            .and_then(|idx| bench.embedding.chunks.get(idx))
            .map(|ch| format!("{}:{}-{}", ch.file_path, ch.start_line, ch.end_line))
            .unwrap_or("?".into());
        let truncated: String = if text.chars().count() > 500 {
            let end = text
                .char_indices()
                .nth(500)
                .map(|(i, _)| i)
                .unwrap_or(text.len());
            format!("{}...", &text[..end])
        } else {
            text.to_string()
        };
        println!(
            "rank={:>2} init={:.4} rerank={:.4} {loc} (full_len={} sent_len={})\n  SENT: {}",
            c.recall_rank,
            c.initial_score,
            c.rerank_score,
            text.chars().count(),
            truncated.chars().count(),
            truncated.replace('\n', " "),
        );
    }
}
