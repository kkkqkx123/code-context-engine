//! Temporary debug: dump pre-merge chunks for globset lib.rs groups.

use cce_config::modules::ChunkingConfig;
use cce_e2e_tests::{TestFixture, init_minimal_logging};
use cce_orchestrator::index::FileProcessor;
use cce_parser::ast_to_nl::chunker::GroupChunker;
use cce_scanner::{FSScanner, ScanOptions};
use cce_types::OutputMode;

fn lines(span: &cce_types::Span) -> (usize, usize) {
    span.line_range_opt().unwrap_or((0, 0))
}

#[tokio::main]
async fn main() {
    init_minimal_logging();

    let fixture = TestFixture::rust_ripgrep().expect("Failed to load ripgrep fixture");
    let project_root = fixture.root_path().to_path_buf();
    let project_root_str = project_root.to_string_lossy().to_string();

    let mut scanner = FSScanner::new();
    let scan_opts = ScanOptions {
        root_path: project_root_str.clone(),
        include_patterns: vec!["*.rs".to_string()],
        ..Default::default()
    };
    let file_entries = scanner.scan(&scan_opts).expect("Failed to scan");

    let mut file_processor = FileProcessor::new();

    for entry in &file_entries {
        let file_path = entry.path.to_string_lossy().to_string();
        if !file_path.contains("globset/src/lib.rs") {
            continue;
        }
        let result = file_processor
            .process_file_complete(entry, OutputMode::Embedding)
            .await
            .expect("process failed");
        let pr = result.processing_result.expect("no processing result");
        let parsed = &result.parsed_file;
        let source = &parsed.source;

        let gcs = file_processor
            .converter()
            .convert_entity_groups(
                &pr.groups,
                &parsed.path,
                Some(&cce_parser::ast_to_nl::ConversionRequest {
                    force_mode: Some(OutputMode::Embedding),
                }),
                Some(&pr),
                Some(source),
            )
            .groups;
        println!("== GROUPS ==");
        for g in &pr.groups {
            let ids: Vec<String> = g.all_entity_ids().iter().map(|i| i.0.to_string()).collect();
            println!(
                "GROUP {} kind={:?} name={} entities=[{}] members={}",
                g.group_id,
                g.header.as_ref().map(|h| h.kind),
                g.name,
                ids.join(","),
                g.members.len()
            );
        }
        let config = ChunkingConfig::default();
        let chunker = GroupChunker::new(config);
        for gc in &gcs {
            if gc.group.group_id == "merged_group_11"
                || gc.group.group_id == "merged_group_16"
                || gc.group.group_id == "merged_group_21"
                || gc.group.group_id == "merged_group_1"
                || gc.group.group_id == "merged_group_5"
                || gc.group.group_id == "group_20"
                || gc.group.group_id == "group_96"
                || gc.group.group_id == "group_50"
            {
                let pre = chunker
                    .chunk_group_with_conversions(&gc.group, gc, &parsed.path)
                    .expect("chunking")
                    .chunks;
                for c in &pre {
                    if c.path != cce_types::ChunkPath::Embedding {
                        continue;
                    }
                    let (sl, el) = lines(&c.metadata.source_span);
                    let ranges: Vec<String> = c
                        .metadata
                        .source_ranges
                        .iter()
                        .map(|s| format!("{:?}", lines(s)))
                        .collect();
                    let ids: Vec<String> = c
                        .metadata
                        .content_entity_ids()
                        .iter()
                        .map(|i| i.0.to_string())
                        .collect();
                    println!(
                        "PRE {} span=({},{}) ranges=[{}] entities=[{}]",
                        c.chunk_id,
                        sl,
                        el,
                        ranges.join(","),
                        ids.join(",")
                    );
                }
            }
        }

        println!("== FINAL CHUNKS ==");
        let mut chunks = result.chunks.clone();
        chunks.retain(|c| c.path == cce_types::ChunkPath::Embedding);
        chunks.sort_by_key(|c| c.metadata.source_span.line_range_opt().map(|r| r.0));
        for chunk in chunks {
            let (sl, el) = lines(&chunk.metadata.source_span);
            let ids: Vec<String> = chunk
                .metadata
                .content_entity_ids()
                .iter()
                .map(|i| i.0.to_string())
                .collect();
            println!(
                "FINAL {} span=({},{}) entities=[{}] merged_group_ids={:?}",
                chunk.chunk_id,
                sl,
                el,
                ids.join(","),
                chunk.metadata.merged_group_ids
            );
        }
    }
}
