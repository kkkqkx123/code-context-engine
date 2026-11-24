//! Debug dump of direct-chunking BM25 chunks for ripgrep regions of interest.
//!
//! Usage:
//! ```shell
//! cargo run --example dbg_direct_chunks -p cce-e2e-tests
//! ```
//! Output: `outputs/debug/direct_chunking/ripgrep_bm25.txt` (review only, no assertions).

use cce_e2e_tests::direct_chunker::EntityChunker;
use cce_parser::ast_to_nl::chunker::ChunkPath;

fn byte_to_line(content: &str, byte: usize) -> usize {
    let upto = content.len().min(byte);
    content[..upto].bytes().filter(|&b| b == b'\n').count() + 1
}

fn main() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("rust")
        .join("review")
        .join("ripgrep");
    let files = [
        "crates/regex/src/matcher.rs",
        "crates/pcre2/src/matcher.rs",
        "crates/searcher/src/line_buffer.rs",
        "crates/globset/src/lib.rs",
        "crates/searcher/src/searcher/mod.rs",
    ];
    let chunker = EntityChunker::new();
    let mut out = String::new();
    for rel in files {
        let path = root.join(rel);
        let content = std::fs::read_to_string(&path).expect("read fixture");
        let chunks = chunker.chunk_file(rel, &content);
        let (bm25, emb): (Vec<_>, Vec<_>) =
            chunks.into_iter().partition(|c| c.path == ChunkPath::Bm25);
        out.push_str(&format!(
            "##### FILE {rel} ({} lines): {} bm25 chunks, {} emb chunks\n",
            content.lines().count(),
            bm25.len(),
            emb.len()
        ));
        for c in &bm25 {
            let start_line = byte_to_line(&content, c.start_byte);
            let end_line = byte_to_line(&content, c.end_byte.max(c.start_byte));
            let head: String = c.text.chars().take(600).collect();
            out.push_str(&format!(
                "--- {} | {:?} | group_type={:?} | bytes={}-{} lines={}-{} | tokens={} | title={:?} | keywords={:?}\n{}\n",
                c.chunk_id,
                c.path,
                c.group_type,
                c.start_byte,
                c.end_byte,
                start_line,
                end_line,
                c.token_count,
                c.bm25_title,
                c.bm25_keywords,
                head,
            ));
        }
    }
    let out_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("outputs")
        .join("debug")
        .join("direct_chunking");
    std::fs::create_dir_all(&out_path).expect("create debug output dir");
    std::fs::write(out_path.join("ripgrep_bm25.txt"), &out).expect("write dump");
    eprintln!(
        "wrote outputs/debug/direct_chunking/ripgrep_bm25.txt ({} bytes)",
        out.len()
    );
}
