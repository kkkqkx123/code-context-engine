use super::*;
use std::io::Write;
use tempfile::TempDir;

fn create_test_scanner() -> FSScanner {
    FSScanner::new()
}

#[test]
fn test_scan_streaming_empty_directory() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let opts = ScanOptions {
        root_path: temp_dir.path().to_str().unwrap().to_string(),
        ..Default::default()
    };

    let mut scanner = create_test_scanner();
    let mut batch_count = 0;
    let mut total_files = 0;

    let count = scanner
        .scan_streaming(&opts, 10, |batch| {
            batch_count += 1;
            total_files += batch.len();
            batch.clear();
        })
        .expect("Scan failed");

    assert_eq!(count, 0);
    assert_eq!(batch_count, 0);
    assert_eq!(total_files, 0);
}

#[test]
fn test_scan_streaming_single_batch() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    for i in 0..5 {
        let file_path = temp_dir.path().join(format!("test{}.txt", i));
        std::fs::write(&file_path, format!("content {}", i)).expect("Failed to write");
    }

    let opts = ScanOptions {
        root_path: temp_dir.path().to_str().unwrap().to_string(),
        ..Default::default()
    };

    let mut scanner = create_test_scanner();
    let mut batch_count = 0;
    let mut total_files = 0;

    let count = scanner
        .scan_streaming(&opts, 10, |batch| {
            batch_count += 1;
            total_files += batch.len();
            batch.clear();
        })
        .expect("Scan failed");

    assert_eq!(count, 5);
    assert_eq!(batch_count, 1);
    assert_eq!(total_files, 5);
}

#[test]
fn test_scan_streaming_multiple_batches() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    for i in 0..25 {
        let file_path = temp_dir.path().join(format!("file{:03}.txt", i));
        std::fs::write(&file_path, format!("content {}", i)).expect("Failed to write");
    }

    let opts = ScanOptions {
        root_path: temp_dir.path().to_str().unwrap().to_string(),
        ..Default::default()
    };

    let mut scanner = create_test_scanner();
    let mut batch_sizes: Vec<usize> = Vec::new();

    let count = scanner
        .scan_streaming(&opts, 10, |batch| {
            batch_sizes.push(batch.len());
            batch.clear();
        })
        .expect("Scan failed");

    assert_eq!(count, 25);
    assert_eq!(batch_sizes.len(), 3);
    assert_eq!(batch_sizes[0], 10);
    assert_eq!(batch_sizes[1], 10);
    assert_eq!(batch_sizes[2], 5);
}

#[test]
fn test_scan_streaming_memory_efficient() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    for i in 0..50 {
        let file_path = temp_dir.path().join(format!("test{:03}.txt", i));
        std::fs::write(&file_path, format!("content {}", i)).expect("Failed to write");
    }

    let opts = ScanOptions {
        root_path: temp_dir.path().to_str().unwrap().to_string(),
        ..Default::default()
    };

    let mut scanner = create_test_scanner();
    let mut max_batch_size = 0;

    scanner
        .scan_streaming(&opts, 10, |batch| {
            max_batch_size = max_batch_size.max(batch.len());
            batch.clear();
        })
        .expect("Scan failed");

    assert!(max_batch_size <= 10);
}

#[test]
fn test_scan_empty_directory() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let opts = ScanOptions {
        root_path: temp_dir.path().to_str().unwrap().to_string(),
        ..Default::default()
    };

    let mut scanner = create_test_scanner();
    let entries = scanner.scan(&opts).expect("Scan failed");
    assert!(entries.is_empty());
}

#[test]
fn test_scan_with_files() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("test.txt");
    let mut file = std::fs::File::create(&file_path).expect("Failed to create file");
    file.write_all(b"Hello, World!").expect("Failed to write");

    let opts = ScanOptions {
        root_path: temp_dir.path().to_str().unwrap().to_string(),
        ..Default::default()
    };

    let mut scanner = create_test_scanner();
    let entries = scanner.scan(&opts).expect("Scan failed");
    assert_eq!(entries.len(), 1);
    assert!(entries[0].is_text());
    assert!(entries[0].content_hash.is_some());
}

#[test]
fn test_incremental_scan_reuses_hashes_for_unchanged_files() {
    use crate::file_processor::compute_content_hash;
    use crate::models::FileEntry;
    use cce_metrics::MetricsRegistry;
    use std::collections::HashMap;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    std::fs::write(temp_dir.path().join("a.rs"), "fn a() {}").expect("write a.rs");
    std::fs::write(temp_dir.path().join("b.rs"), "fn b() {}").expect("write b.rs");

    let opts = ScanOptions {
        root_path: temp_dir.path().to_str().unwrap().to_string(),
        ..Default::default()
    };

    let registry = MetricsRegistry::new();
    let metrics = ScannerMetrics::new(&registry, 1);
    let mut scanner = FSScanner::new().with_scanner_metrics(metrics.clone());

    let first = scanner.scan(&opts).expect("full scan failed");
    assert_eq!(first.len(), 2);
    let previous: HashMap<PathBuf, FileEntry> = first
        .iter()
        .map(|entry| (entry.relative_path.clone(), entry.clone()))
        .collect();
    let a_old = first
        .iter()
        .find(|e| e.relative_path.ends_with("a.rs"))
        .expect("a.rs must be scanned")
        .content_hash
        .clone();

    std::fs::write(temp_dir.path().join("b.rs"), "fn b() {}\nfn extra() {}").expect("modify b.rs");

    let second = scanner
        .scan_incremental(&opts, &previous)
        .expect("incremental scan failed");
    assert_eq!(second.len(), 2);
    assert_eq!(
        metrics.files_hash_reused_total.get(),
        1,
        "exactly the unchanged file must skip re-hashing"
    );

    let a_new = second
        .iter()
        .find(|e| e.relative_path.ends_with("a.rs"))
        .expect("a.rs must be scanned");
    assert_eq!(
        a_new.content_hash, a_old,
        "unchanged file reuses its previous hash"
    );

    let b_new = second
        .iter()
        .find(|e| e.relative_path.ends_with("b.rs"))
        .expect("b.rs must be scanned");
    let expected = compute_content_hash(b"fn b() {}\nfn extra() {}");
    assert_eq!(
        b_new.content_hash.as_deref(),
        Some(expected.as_str()),
        "changed file is re-hashed with fresh content"
    );
}

#[test]
fn test_scan_with_nested_directories() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    let nested_dir = temp_dir.path().join("src/components");
    std::fs::create_dir_all(&nested_dir).expect("Failed to create dirs");

    std::fs::write(temp_dir.path().join("root.txt"), "root content").expect("Failed to write");
    std::fs::write(nested_dir.join("button.rs"), "fn button() {}").expect("Failed to write");

    let opts = ScanOptions {
        root_path: temp_dir.path().to_str().unwrap().to_string(),
        ..Default::default()
    };

    let mut scanner = create_test_scanner();
    let entries = scanner.scan(&opts).expect("Scan failed");
    assert_eq!(entries.len(), 2);
}

#[test]
fn test_scan_with_include_pattern() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    std::fs::write(temp_dir.path().join("test.rs"), "code").expect("Failed to write");
    std::fs::write(temp_dir.path().join("test.txt"), "text").expect("Failed to write");

    let opts = ScanOptions {
        root_path: temp_dir.path().to_str().unwrap().to_string(),
        include_patterns: vec!["*.rs".to_string()],
        ..Default::default()
    };

    let mut scanner = create_test_scanner();
    let entries = scanner.scan(&opts).expect("Scan failed");
    assert_eq!(entries.len(), 1);
    assert!(entries[0].relative_path.to_str().unwrap().ends_with(".rs"));
}

#[test]
fn test_scan_with_exclude_pattern() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    std::fs::write(temp_dir.path().join("keep.rs"), "code").expect("Failed to write");
    std::fs::write(temp_dir.path().join("skip.log"), "log").expect("Failed to write");

    let opts = ScanOptions {
        root_path: temp_dir.path().to_str().unwrap().to_string(),
        exclude_patterns: vec!["*.log".to_string()],
        ..Default::default()
    };

    let mut scanner = create_test_scanner();
    let entries = scanner.scan(&opts).expect("Scan failed");
    assert_eq!(entries.len(), 1);
    assert!(entries[0].relative_path.to_str().unwrap().contains("keep"));
}

#[test]
fn test_scan_with_gitignore() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    std::fs::write(temp_dir.path().join(".gitignore"), "*.ignored\n")
        .expect("Failed to write gitignore");
    std::fs::write(temp_dir.path().join("keep.rs"), "code").expect("Failed to write");
    std::fs::write(temp_dir.path().join("skip.ignored"), "ignored").expect("Failed to write");

    let opts = ScanOptions {
        root_path: temp_dir.path().to_str().unwrap().to_string(),
        respect_gitignore: true,
        ..Default::default()
    };

    let mut scanner = create_test_scanner();
    let entries = scanner.scan(&opts).expect("Scan failed");

    let has_ignored = entries
        .iter()
        .any(|e| e.relative_path.to_string_lossy().contains("skip.ignored"));
    let has_keep = entries
        .iter()
        .any(|e| e.relative_path.to_string_lossy().contains("keep.rs"));

    assert!(!has_ignored);
    assert!(has_keep);
}

#[test]
fn test_scan_binary_file_detection() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    std::fs::write(temp_dir.path().join("text.txt"), "Hello, World!").expect("Failed to write");
    std::fs::write(temp_dir.path().join("binary.bin"), vec![0u8, 1, 2, 0, 3])
        .expect("Failed to write");

    let opts = ScanOptions {
        root_path: temp_dir.path().to_str().unwrap().to_string(),
        ..Default::default()
    };

    let mut scanner = create_test_scanner();
    let entries = scanner.scan(&opts).expect("Scan failed");
    assert_eq!(entries.len(), 2);

    let text_entry = entries
        .iter()
        .find(|e| e.relative_path.to_str().unwrap().ends_with(".txt"))
        .unwrap();
    let binary_entry = entries
        .iter()
        .find(|e| e.relative_path.to_str().unwrap().ends_with(".bin"))
        .unwrap();

    assert!(text_entry.is_text());
    assert!(!binary_entry.is_text());
}

#[test]
fn test_scan_nonexistent_directory() {
    let opts = ScanOptions {
        root_path: "/nonexistent/path/12345".to_string(),
        ..Default::default()
    };

    let mut scanner = create_test_scanner();
    let result = scanner.scan(&opts);
    assert!(result.is_err());
}

#[test]
fn test_scan_file_instead_of_directory() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_path = temp_dir.path().join("not_a_dir.txt");
    std::fs::write(&file_path, "content").expect("Failed to write");

    let opts = ScanOptions {
        root_path: file_path.to_str().unwrap().to_string(),
        ..Default::default()
    };

    let mut scanner = create_test_scanner();
    let result = scanner.scan(&opts);
    assert!(result.is_err());
}

#[test]
fn test_plugin_filter_directory_prefix_cache() {
    use cce_plugin::{CodePlugin, PluginMetadata, PluginRegistry};
    use cce_types::FileFilterDecision;
    use std::sync::Arc;

    struct FilterPlugin;

    impl CodePlugin for FilterPlugin {
        fn metadata(&self) -> &PluginMetadata {
            static META: std::sync::OnceLock<PluginMetadata> = std::sync::OnceLock::new();
            META.get_or_init(|| PluginMetadata {
                id: "filter".into(),
                name: "filter".into(),
                version: "0.1.0".into(),
                priority: 10,
                capability_priorities: std::collections::HashMap::new(),
                description: None,
                capabilities: vec!["file_filter".into()],
            })
        }

        fn supports_file_filter(&self) -> bool {
            true
        }

        fn filter_file(
            &self,
            file_path: &str,
            is_directory: bool,
            _size: u64,
        ) -> std::result::Result<Option<FileFilterDecision>, cce_plugin::PluginError> {
            if is_directory && file_path.contains("scratch") {
                return Ok(Some(FileFilterDecision::Exclude));
            }
            if is_directory && file_path.contains("allowlist") {
                return Ok(Some(FileFilterDecision::Include));
            }
            if file_path.ends_with(".cconf") {
                return Ok(Some(FileFilterDecision::Include));
            }
            Ok(None)
        }
    }

    let mut registry = PluginRegistry::new();
    registry.register(Arc::new(FilterPlugin));

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let scratch = temp_dir.path().join("scratch/sub");
    std::fs::create_dir_all(&scratch).expect("Failed to create dirs");
    std::fs::write(scratch.join("file.txt"), "content").expect("Failed to write");
    let allowlist = temp_dir.path().join("allowlist");
    std::fs::create_dir_all(&allowlist).expect("Failed to create dirs");
    std::fs::write(allowlist.join("keep.txt"), "content").expect("Failed to write");
    std::fs::write(allowlist.join("keep.cconf"), "content").expect("Failed to write");
    std::fs::write(temp_dir.path().join("root.txt"), "content").expect("Failed to write");

    let opts = ScanOptions {
        root_path: temp_dir.path().to_str().unwrap().to_string(),
        exclude_patterns: vec!["*.txt".to_string()],
        ..Default::default()
    };

    let mut scanner = FSScanner::new().with_plugin_registry(Arc::new(registry));
    let entries = scanner.scan(&opts).expect("Scan failed");

    let has_allowlist_txt = entries.iter().any(|e| {
        e.relative_path
            .to_string_lossy()
            .ends_with("allowlist/keep.txt")
    });
    let has_allowlist_cconf = entries.iter().any(|e| {
        e.relative_path
            .to_string_lossy()
            .ends_with("allowlist/keep.cconf")
    });
    let has_scratch = entries
        .iter()
        .any(|e| e.relative_path.to_string_lossy().contains("scratch"));
    let has_root_txt = entries
        .iter()
        .any(|e| e.relative_path.to_string_lossy() == "root.txt");

    assert!(
        has_allowlist_txt,
        "cached Include must force-include subtree"
    );
    assert!(has_allowlist_cconf, ".cconf file must be force-included");
    assert!(
        !has_scratch,
        "Exclude decision must prune the whole subtree"
    );
    assert!(
        !has_root_txt,
        "files outside any decided dir defer to built-in matcher"
    );
}

#[test]
fn test_scan_excluded_directory() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    let excluded_dir = temp_dir.path().join("node_modules");
    std::fs::create_dir(&excluded_dir).expect("Failed to create dir");
    std::fs::write(excluded_dir.join("package.json"), "{}").expect("Failed to write");
    std::fs::write(temp_dir.path().join("main.rs"), "fn main() {}").expect("Failed to write");

    let opts = ScanOptions {
        root_path: temp_dir.path().to_str().unwrap().to_string(),
        respect_gitignore: false,
        gitignore_patterns: vec!["node_modules/".to_string()],
        ..Default::default()
    };

    let mut scanner = create_test_scanner();
    let entries = scanner.scan(&opts).expect("Scan failed");

    let has_node_modules = entries
        .iter()
        .any(|e| e.relative_path.to_string_lossy().contains("node_modules"));
    let has_main_rs = entries
        .iter()
        .any(|e| e.relative_path.to_string_lossy().contains("main.rs"));

    assert!(!has_node_modules);
    assert!(has_main_rs);
}

#[test]
fn test_scan_options_default() {
    let opts = ScanOptions::default();
    assert!(opts.root_path.is_empty());
    assert!(opts.include_patterns.is_empty());
    assert!(opts.exclude_patterns.is_empty());
    assert!(!opts.follow_symlinks);
    assert!(!opts.respect_gitignore);
    assert!(opts.gitignore_patterns.is_empty());
    assert!(opts.gitignore_path.is_none());
    assert!(opts.max_content_size.is_none());
}

#[test]
fn test_scanner_reset() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    std::fs::write(temp_dir.path().join("test.txt"), "Hello, World!").expect("Failed to write");

    let opts = ScanOptions {
        root_path: temp_dir.path().to_str().unwrap().to_string(),
        ..Default::default()
    };

    let mut scanner = create_test_scanner();
    let entries1 = scanner.scan(&opts).expect("Scan failed");
    assert_eq!(entries1.len(), 1);

    scanner.reset();

    let entries2 = scanner.scan(&opts).expect("Scan failed");
    assert_eq!(entries2.len(), 1);
}
