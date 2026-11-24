//! Diagnostic dump for the media-server benchmark baselines.
//!
//! Reads the persisted `bench_data.rkyv` of every baseline and prints the
//! retrieval facts behind the aggregate metrics: per-chunk ranks and scores,
//! the token/length profile of the indexed corpora, and duplicate-document
//! statistics. It never mutates the benchmark data.
//!
//! Usage:
//! - `locate <query_id> <file-substring> <start-end>`: every chunk of the
//!   named file with its rank, score and indexed text.
//! - `top <query_id> [k]`: the top-k BM25 and embedding hits.
//! - `strong`: the best-ranked chunk covering each Strong judgment range.
//! - `stats`: corpus token profile (p50/p90/p99) per baseline.
//! - `dups`: exact-duplicate share of each corpus.
//! - `coverage`: source-span width vs indexed text size per chunk.

use std::path::PathBuf;

use cce_e2e_tests::bench_data::{compute_bm25_scores, cosine_similarity, load_benchmark_data};
use cce_storage_bm25::TermOperator;

const FIXTURE: &str = "media_server";
const MODEL: &str = "bge-m3";
const BASELINES: &[&str] = &[
    "direct_chunking",
    "full_pipeline",
    "full_pipeline_raw_source",
];

fn data_dir(baseline: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("benchmark")
        .join(baseline)
        .join(FIXTURE)
        .join(MODEL)
        .join("bench_data.rkyv")
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.remove(0);

    match mode.as_str() {
        // locate <query_id> <file-substring> <start-end>
        "locate" => {
            let query_id = args.remove(0);
            let file = args.remove(0);
            let range = args.remove(0);
            let (a, b) = range.split_once('-').expect("range start-end");
            let a: usize = a.parse().expect("start");
            let b: usize = b.parse().expect("end");
            for baseline in BASELINES {
                let bench = load_benchmark_data(&data_dir(baseline)).expect("load");
                let bm25_scores = compute_bm25_scores(
                    &bench.bm25_documents,
                    &bench.query_texts,
                    TermOperator::Or,
                );
                let emb_dim = bench.embedding.dimension as usize;
                let q_idx = bench
                    .queries
                    .iter()
                    .position(|q| q.id == query_id)
                    .expect("query id");
                println!("\n######## {baseline} / {query_id} ########");
                println!("query: {}", bench.queries[q_idx].text);

                // BM25 rank map
                let mut order: Vec<usize> = (0..bench.bm25.chunks.len()).collect();
                order.sort_by(|x, y| {
                    bm25_scores[q_idx][*y]
                        .partial_cmp(&bm25_scores[q_idx][*x])
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let mut rank_of = vec![usize::MAX; bench.bm25.chunks.len()];
                for (r, i) in order.iter().enumerate() {
                    rank_of[*i] = r + 1;
                }
                println!("--- BM25 chunks in {file} ---");
                for (i, c) in bench.bm25.chunks.iter().enumerate() {
                    if !c.file_path.contains(&file) {
                        continue;
                    }
                    let doc = &bench.bm25_documents[i];
                    let ctoks = cce_text::MixedTokenizer::new().tokenize(&doc.content).len();
                    let ttoks = cce_text::MixedTokenizer::new().tokenize(&doc.title).len();
                    let ktoks = cce_text::MixedTokenizer::new()
                        .tokenize(&doc.keywords)
                        .len();
                    let hit = c.start_line <= b && c.end_line >= a;
                    println!(
                        "  rank={:>5} score={:>9.4} id={} file={}:{}-{} hit={} title={} kw={} toks(t/k/c)={}/{}/{}",
                        rank_of[i],
                        bm25_scores[q_idx][i],
                        c.chunk_id,
                        c.file_path,
                        c.start_line,
                        c.end_line,
                        hit,
                        doc.title,
                        doc.keywords,
                        ttoks,
                        ktoks,
                        ctoks
                    );
                    if hit {
                        println!("      >>> {}", doc.content.replace('\n', "\\n"));
                    }
                }

                // EMB rank map
                let q_start = q_idx * emb_dim;
                let q_end = q_start + emb_dim;
                let query_vec = &bench.embedding.query_vectors[q_start..q_end];
                let mut scores = Vec::with_capacity(bench.embedding.chunks.len());
                for c_idx in 0..bench.embedding.chunks.len() {
                    let cs = c_idx * emb_dim;
                    scores.push(cosine_similarity(
                        &bench.embedding.vectors[cs..cs + emb_dim],
                        query_vec,
                    ));
                }
                let mut eorder: Vec<usize> = (0..scores.len()).collect();
                eorder.sort_by(|x, y| {
                    scores[*y]
                        .partial_cmp(&scores[*x])
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let mut erank = vec![usize::MAX; scores.len()];
                for (r, i) in eorder.iter().enumerate() {
                    erank[*i] = r + 1;
                }
                println!("--- EMB chunks in {file} ---");
                for (i, c) in bench.embedding.chunks.iter().enumerate() {
                    if !c.file_path.contains(&file) {
                        continue;
                    }
                    let etoks = cce_text::MixedTokenizer::new()
                        .tokenize(&bench.embedding.texts[i])
                        .len();
                    let est =
                        cce_utils::token_estimation::estimate_tokens(&bench.embedding.texts[i]);
                    let hit = c.start_line <= b && c.end_line >= a;
                    let ranges: Vec<String> = c
                        .source_ranges
                        .iter()
                        .map(|r| format!("{}-{}", r.start_line, r.end_line))
                        .collect();
                    println!(
                        "  rank={:>5} score={:>9.4} id={} file={}:{}-{} hit={} entity={} toks={} est_tokens={} ranges=[{}]",
                        erank[i],
                        scores[i],
                        c.chunk_id,
                        c.file_path,
                        c.start_line,
                        c.end_line,
                        hit,
                        c.entity_name,
                        etoks,
                        est,
                        ranges.join(", ")
                    );
                    if hit {
                        println!(
                            "      >>> {}",
                            bench.embedding.texts[i].replace('\n', "\\n")
                        );
                    }
                }
            }
        }
        // top <query_id> [k]
        "top" => {
            let query_id = args.remove(0);
            let k: usize = args.first().and_then(|s| s.parse().ok()).unwrap_or(10);
            for baseline in BASELINES {
                let bench = load_benchmark_data(&data_dir(baseline)).expect("load");
                let bm25_scores = compute_bm25_scores(
                    &bench.bm25_documents,
                    &bench.query_texts,
                    TermOperator::Or,
                );
                let emb_dim = bench.embedding.dimension as usize;
                let q_idx = bench
                    .queries
                    .iter()
                    .position(|q| q.id == query_id)
                    .expect("query id");
                println!("\n######## {baseline} / {query_id} ########");
                println!("query: {}", bench.queries[q_idx].text);
                let mut order: Vec<usize> = (0..bench.bm25.chunks.len()).collect();
                order.sort_by(|x, y| {
                    bm25_scores[q_idx][*y]
                        .partial_cmp(&bm25_scores[q_idx][*x])
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                println!("--- BM25 top {k} ---");
                for (r, i) in order.iter().take(k).enumerate() {
                    let c = &bench.bm25.chunks[*i];
                    let d = &bench.bm25_documents[*i];
                    println!(
                        "  #{} score={:>9.4} id={} file={}:{}-{} title={}",
                        r + 1,
                        bm25_scores[q_idx][*i],
                        c.chunk_id,
                        c.file_path,
                        c.start_line,
                        c.end_line,
                        d.title
                    );
                }
                let q_start = q_idx * emb_dim;
                let query_vec = &bench.embedding.query_vectors[q_start..q_start + emb_dim];
                let mut scores = Vec::with_capacity(bench.embedding.chunks.len());
                for c_idx in 0..bench.embedding.chunks.len() {
                    let cs = c_idx * emb_dim;
                    scores.push(cosine_similarity(
                        &bench.embedding.vectors[cs..cs + emb_dim],
                        query_vec,
                    ));
                }
                let mut eorder: Vec<usize> = (0..scores.len()).collect();
                eorder.sort_by(|x, y| {
                    scores[*y]
                        .partial_cmp(&scores[*x])
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                println!("--- EMB top {k} ---");
                for (r, i) in eorder.iter().take(k).enumerate() {
                    let c = &bench.embedding.chunks[*i];
                    println!(
                        "  #{} score={:>9.4} id={} file={}:{}-{} entity={} toks={}",
                        r + 1,
                        scores[*i],
                        c.chunk_id,
                        c.file_path,
                        c.start_line,
                        c.end_line,
                        c.entity_name,
                        cce_text::MixedTokenizer::new()
                            .tokenize(&bench.embedding.texts[*i])
                            .len()
                    );
                }
            }
        }
        // strong: rank+score of the chunk that best covers each Strong judgment range
        "strong" => {
            use cce_e2e_tests::bench_data::RelevanceLevel;
            use cce_e2e_tests::judgments::media_server::media_server_relevance_judgments;

            let judgments = media_server_relevance_judgments();
            for baseline in BASELINES {
                let bench = load_benchmark_data(&data_dir(baseline)).expect("load");
                let bm25_scores = compute_bm25_scores(
                    &bench.bm25_documents,
                    &bench.query_texts,
                    TermOperator::Or,
                );
                let emb_dim = bench.embedding.dimension as usize;
                let mut emb_ranks: Vec<Vec<usize>> = Vec::new();
                let mut emb_scores: Vec<Vec<f64>> = Vec::new();
                for q_idx in 0..bench.queries.len() {
                    let q_start = q_idx * emb_dim;
                    let query_vec = &bench.embedding.query_vectors[q_start..q_start + emb_dim];
                    let mut scores = Vec::with_capacity(bench.embedding.chunks.len());
                    for c_idx in 0..bench.embedding.chunks.len() {
                        let cs = c_idx * emb_dim;
                        scores.push(cosine_similarity(
                            &bench.embedding.vectors[cs..cs + emb_dim],
                            query_vec,
                        ));
                    }
                    let mut order: Vec<usize> = (0..scores.len()).collect();
                    order.sort_by(|x, y| {
                        scores[*y]
                            .partial_cmp(&scores[*x])
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    let mut rank = vec![usize::MAX; scores.len()];
                    let mut sc = vec![0.0f64; scores.len()];
                    for (r, i) in order.iter().enumerate() {
                        rank[*i] = r + 1;
                        sc[*i] = scores[*i];
                    }
                    emb_ranks.push(rank);
                    emb_scores.push(sc);
                }
                let mut bm25_ranks: Vec<Vec<usize>> = Vec::new();
                for query_scores in bm25_scores.iter().take(bench.queries.len()) {
                    let mut order: Vec<usize> = (0..bench.bm25.chunks.len()).collect();
                    order.sort_by(|x, y| {
                        query_scores[*y]
                            .partial_cmp(&query_scores[*x])
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    let mut rank = vec![usize::MAX; bench.bm25.chunks.len()];
                    for (r, i) in order.iter().enumerate() {
                        rank[*i] = r + 1;
                    }
                    bm25_ranks.push(rank);
                }
                println!(
                    "\n######## {baseline} : best-ranked chunk covering each Strong range ########"
                );
                println!(
                    "{:28} {:6} {:>10} {:>10.4} {:>8} {:>8.4}",
                    "query", "path", "emb_rank", "emb_score", "bm25_rank", "bm25_score"
                );
                for j in &judgments {
                    let q_idx = match bench.queries.iter().position(|q| q.id == j.id) {
                        Some(i) => i,
                        None => continue,
                    };
                    let mut emb_best = (usize::MAX, 0.0f64);
                    let mut bm25_best = (usize::MAX, 0.0f64);
                    for (ci, c) in bench.embedding.chunks.iter().enumerate() {
                        for (rg, level) in &j.relevant_ranges {
                            if *level != RelevanceLevel::Strong || c.file_path != rg.file {
                                continue;
                            }
                            let covers = c.source_ranges.iter().any(|r| {
                                r.start_line <= rg.end_line && r.end_line >= rg.start_line
                            }) || (c.start_line <= rg.end_line
                                && c.end_line >= rg.start_line);
                            if !covers {
                                continue;
                            }
                            if emb_ranks[q_idx][ci] < emb_best.0 {
                                emb_best = (emb_ranks[q_idx][ci], emb_scores[q_idx][ci]);
                            }
                        }
                    }
                    for (ci, c) in bench.bm25.chunks.iter().enumerate() {
                        for (rg, level) in &j.relevant_ranges {
                            if *level != RelevanceLevel::Strong || c.file_path != rg.file {
                                continue;
                            }
                            let covers = c.source_ranges.iter().any(|r| {
                                r.start_line <= rg.end_line && r.end_line >= rg.start_line
                            }) || (c.start_line <= rg.end_line
                                && c.end_line >= rg.start_line);
                            if !covers {
                                continue;
                            }
                            if bm25_ranks[q_idx][ci] < bm25_best.0 {
                                bm25_best = (bm25_ranks[q_idx][ci], bm25_scores[q_idx][ci]);
                            }
                        }
                    }
                    println!(
                        "{:28} {:>10} {:>10.4} {:>8} {:>8.4}",
                        j.id,
                        if emb_best.0 == usize::MAX {
                            "-".to_string()
                        } else {
                            emb_best.0.to_string()
                        },
                        emb_best.1,
                        if bm25_best.0 == usize::MAX {
                            "-".to_string()
                        } else {
                            bm25_best.0.to_string()
                        },
                        bm25_best.1,
                    );
                }
            }
        }
        // coverage: source-span width per chunk vs indexed text size
        "coverage" => {
            use std::collections::HashMap;
            for baseline in BASELINES {
                let bench = load_benchmark_data(&data_dir(baseline)).expect("load");
                let tok = cce_text::MixedTokenizer::new();
                for (label, chunks, texts) in [
                    (
                        "bm25",
                        &bench.bm25.chunks,
                        bench
                            .bm25_documents
                            .iter()
                            .map(|d| d.content.clone())
                            .collect::<Vec<_>>(),
                    ),
                    (
                        "emb",
                        &bench.embedding.chunks,
                        bench.embedding.texts.clone(),
                    ),
                ] {
                    let mut per_file: HashMap<&str, (u64, u64)> = HashMap::new();
                    let mut widths = Vec::new();
                    let mut nranges = Vec::new();
                    let mut ratios = Vec::new();
                    for (c, t) in chunks.iter().zip(texts.iter()) {
                        let w = (c.end_line - c.start_line) as u64;
                        let n = c.source_ranges.len() as u64;
                        let tk = tok.tokenize(t).len() as u64;
                        widths.push(w);
                        nranges.push(n);
                        if w > 0 {
                            ratios.push(tk as f64 / w as f64);
                        }
                        let e = per_file.entry(c.file_path.as_str()).or_insert((0, 0));
                        e.0 += w;
                        e.1 += 1;
                    }
                    let mean = |v: &Vec<u64>| v.iter().sum::<u64>() as f64 / v.len() as f64;
                    let meanf = |v: &Vec<f64>| v.iter().sum::<f64>() / v.len() as f64;
                    println!(
                        "{baseline:26} {label:5} n={:5} mean_span_lines={:7.1} mean_ranges={:5.2} mean_tokens/line={:6.2}",
                        chunks.len(),
                        mean(&widths),
                        mean(&nranges),
                        meanf(&ratios)
                    );
                }
            }
        }
        // files: per-file chunk counts, to spot files that produced no chunk
        "files" => {
            use std::collections::BTreeMap;
            for baseline in BASELINES {
                let bench = load_benchmark_data(&data_dir(baseline)).expect("load");
                let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
                for c in bench
                    .embedding
                    .chunks
                    .iter()
                    .chain(bench.bm25.chunks.iter())
                {
                    *counts.entry(c.file_path.as_str()).or_default() += 1;
                }
                let mut files: Vec<_> = counts.iter().collect();
                files.sort_by_key(|(_, n)| **n);
                println!(
                    "\n######## {baseline}: {} distinct files, {} chunks ########",
                    files.len(),
                    files.iter().map(|(_, n)| **n).sum::<usize>()
                );
                for (f, n) in files.iter().take(12) {
                    println!("   {n:>4}  {f}");
                }
            }
        }
        // dups: count duplicate chunk contents per baseline
        "dups" => {
            for baseline in BASELINES {
                let bench = load_benchmark_data(&data_dir(baseline)).expect("load");
                for (label, texts) in [
                    (
                        "bm25",
                        &bench
                            .bm25_documents
                            .iter()
                            .map(|d| d.content.clone())
                            .collect::<Vec<_>>(),
                    ),
                    ("emb", &bench.embedding.texts),
                ] {
                    let mut counts: std::collections::HashMap<&str, usize> =
                        std::collections::HashMap::new();
                    for t in texts.iter() {
                        *counts.entry(t.as_str()).or_default() += 1;
                    }
                    let dup_groups: Vec<(&str, usize)> = counts
                        .iter()
                        .filter(|(_, c)| **c > 1)
                        .map(|(t, c)| (*t, *c))
                        .collect();
                    let dup_docs: usize = dup_groups.iter().map(|(_, c)| c).sum();
                    let max_dup = dup_groups.iter().map(|(_, c)| *c).max().unwrap_or(0);
                    println!(
                        "{baseline:26} {label:5} docs={:5} dup_groups={:4} dup_docs={:4} max_copies={} dup_ratio={:.3}",
                        texts.len(),
                        dup_groups.len(),
                        dup_docs,
                        max_dup,
                        dup_docs as f64 / texts.len() as f64
                    );
                }
            }
        }
        // stats: print aggregate token stats per baseline
        "stats" => {
            for baseline in BASELINES {
                let bench = load_benchmark_data(&data_dir(baseline)).expect("load");
                let tok = cce_text::MixedTokenizer::new();
                let mut bt: Vec<usize> = bench
                    .bm25_documents
                    .iter()
                    .map(|d| tok.tokenize(&d.content).len())
                    .collect();
                let mut et: Vec<usize> = bench
                    .embedding
                    .texts
                    .iter()
                    .map(|t| tok.tokenize(t).len())
                    .collect();
                bt.sort_unstable();
                et.sort_unstable();
                let pct = |v: &Vec<usize>, p: usize| v[v.len() * p / 100];
                println!(
                    "{baseline}: bm25 n={} content_tokens p50={} p90={} p99={} mean={:.0} | emb n={} tokens p50={} p90={} p99={} mean={:.0}",
                    bt.len(),
                    pct(&bt, 50),
                    pct(&bt, 90),
                    pct(&bt, 99),
                    bt.iter().sum::<usize>() as f64 / bt.len() as f64,
                    et.len(),
                    pct(&et, 50),
                    pct(&et, 90),
                    pct(&et, 99),
                    et.iter().sum::<usize>() as f64 / et.len() as f64,
                );
            }
        }
        other => panic!("unknown mode {other}"),
    }
}
