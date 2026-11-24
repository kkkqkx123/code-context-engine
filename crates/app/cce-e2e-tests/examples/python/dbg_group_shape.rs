//! Temporary diagnostic: live chunk shape for the Flask benchmark fixture.

use std::collections::HashMap;
use std::path::Path;

use cce_e2e_tests::FixtureSpec;
use cce_e2e_tests::baselines::{entity_based, full_pipeline};
use cce_e2e_tests::bench_data::ChunkData;
use cce_e2e_tests::bench_gen::GeneratedChunks;

fn group_of(chunk: &ChunkData) -> String {
    format!("{}::{}", chunk.file_path, chunk.entity_name)
}

fn source_lines(root: &Path, file: &str) -> Vec<String> {
    let path = root.join(file);
    std::fs::read_to_string(path)
        .map(|text| text.lines().map(|line| line.to_string()).collect())
        .unwrap_or_default()
}

fn report(label: &str, chunks: &GeneratedChunks) {
    let emb = &chunks.embedding_chunks;
    let emb_texts = &chunks.embedding_texts;
    println!(
        "\n================ {label} ================\nemb: {} | bm25: {}",
        emb.len(),
        chunks.bm25_chunks.len()
    );

    let root = Path::new("fixtures/python/review/flask");
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, chunk) in emb.iter().enumerate() {
        groups.entry(group_of(chunk)).or_default().push(index);
    }
    let mut histogram: HashMap<usize, usize> = HashMap::new();
    for indices in groups.values() {
        *histogram.entry(indices.len()).or_insert(0) += 1;
    }
    let mut histogram: Vec<_> = histogram.into_iter().collect();
    histogram.sort();
    println!("groups: {} | size histogram: {histogram:?}", groups.len());

    let mut text_bytes = 0usize;
    let mut covered_lines = 0usize;
    let mut source_bytes = 0usize;
    let mut cached: HashMap<String, Vec<String>> = HashMap::new();
    for (index, chunk) in emb.iter().enumerate() {
        text_bytes += emb_texts.get(index).map_or(0, String::len);
        let lines = cached
            .entry(chunk.file_path.clone())
            .or_insert_with(|| source_lines(root, &chunk.file_path));
        for range in &chunk.source_ranges {
            for line in range.start_line..=range.end_line {
                if let Some(text) = lines.get(line.saturating_sub(1)) {
                    covered_lines += 1;
                    source_bytes += text.len();
                }
            }
        }
    }
    println!(
        "text/source: {:.2}x ({} vs {}), lines/chunk {:.1}, bytes/chunk {}",
        text_bytes as f64 / source_bytes.max(1) as f64,
        text_bytes,
        source_bytes,
        covered_lines as f64 / emb.len().max(1) as f64,
        text_bytes / emb.len().max(1)
    );

    let mut worst: Vec<(String, usize)> = groups
        .iter()
        .map(|(group, indices)| (group.clone(), indices.len()))
        .filter(|(_, count)| *count >= 3)
        .collect();
    worst.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    println!("groups with >=3 chunks: {}", worst.len());
    for (group, count) in worst.iter().take(10) {
        let indices = &groups[group];
        let bytes: usize = indices
            .iter()
            .map(|i| emb_texts.get(*i).map_or(0, String::len))
            .sum();
        let lines: usize = indices
            .iter()
            .flat_map(|i| emb[*i].source_ranges.iter())
            .map(|r| r.end_line.saturating_sub(r.start_line) + 1)
            .sum();
        println!(
            "  {group} chunks={count} bytes={bytes} lines={lines} ({:.1} lines/chunk)",
            lines as f64 / *count as f64
        );
    }

    for (index, chunk) in emb.iter().enumerate() {
        if chunk.entity_name == "send_file" {
            println!(
                "  send_file fragment {}/{}: bytes={} ranges={:?}",
                index + 1,
                emb.len(),
                emb_texts[index].len(),
                chunk.source_ranges
            );
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let full = full_pipeline::gen_full_pipeline_chunks(
        FixtureSpec::python_flask(),
        FixtureSpec::python_basic(),
    )
    .await?;
    report("full_pipeline", &full);

    let direct = entity_based::gen_entity_based_chunks(
        FixtureSpec::python_flask(),
        FixtureSpec::python_basic(),
    )
    .await?;
    report("direct_chunking", &direct);
    Ok(())
}
