//! Shared benchmark support for code-context-engine performance benchmarks.
//!
//! Provides the fixed scale tiers so every benchmark speaks the same scale
//! vocabulary, plus a deterministic synthetic project generator, timing and
//! report helpers, and small on-disk fixtures.
//!
//! Everything here is dependency-free (std only): the generator produces plain
//! file paths, languages and content, and each benchmark converts the synthetic
//! project into the typed inputs its crate needs. This keeps the crate cheap
//! to compile and a single source of truth for tier sizes.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Fixed scale tiers aligned with the e2e fixture tiers.
///
/// Small ~500 files, Medium ~5_000 files, Large ~50_000 files. Each file
/// carries ~4 entities in the relation benchmarks' density model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectSize {
    Small,
    Medium,
    Large,
}

impl ProjectSize {
    /// File count for this tier.
    pub fn file_count(self) -> usize {
        match self {
            ProjectSize::Small => 500,
            ProjectSize::Medium => 5_000,
            ProjectSize::Large => 50_000,
        }
    }

    /// Approximate entity count assuming 4 entities per file, matching the
    /// relation benchmark's `FUNCTIONS_PER_FILE` density.
    pub fn entity_count(self) -> usize {
        self.file_count() * 4
    }

    /// Stable short label used in reports.
    pub fn label(self) -> &'static str {
        match self {
            ProjectSize::Small => "small",
            ProjectSize::Medium => "medium",
            ProjectSize::Large => "large",
        }
    }

    /// Iterate the three tiers in ascending size order.
    pub fn all() -> [ProjectSize; 3] {
        [ProjectSize::Small, ProjectSize::Medium, ProjectSize::Large]
    }
}

/// File-extension suffix per language, used when materializing to disk.
fn ext_for(language: &str) -> &'static str {
    match language {
        "rust" => "rs",
        "python" => "py",
        "go" => "go",
        "typescript" => "ts",
        "java" => "java",
        "cpp" => "cpp",
        "c" => "c",
        _ => "txt",
    }
}

/// A single synthetic source file.
#[derive(Debug, Clone)]
pub struct SyntheticFile {
    pub path: PathBuf,
    pub language: &'static str,
    pub content: String,
}

/// A generated synthetic project.
#[derive(Debug, Clone)]
pub struct SyntheticProject {
    pub size: ProjectSize,
    pub files: Vec<SyntheticFile>,
}

/// Default language distribution used when generating synthetic content.
pub const DEFAULT_LANG_DIST: &[(&str, f64)] = &[
    ("rust", 0.5),
    ("python", 0.2),
    ("go", 0.15),
    ("typescript", 0.15),
];

/// Deterministic xorshift so benchmark inputs are reproducible across runs.
fn xorshift(mut x: u64) -> u64 {
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

/// Pick a language from a weighted distribution using a deterministic state.
fn pick_language<'a>(state: u64, dist: &'a [(&'a str, f64)]) -> &'a str {
    let f = ((state >> 33) as f64) / (u32::MAX as f64 + 1.0);
    let mut acc = 0.0f64;
    for (lang, w) in dist {
        acc += w;
        if f <= acc {
            return lang;
        }
    }
    dist.last().map(|(l, _)| *l).unwrap_or("rust")
}

/// Generate deterministic, syntactically-plausible source content for a file.
///
/// Content parses with a real grammar (so file-based benchmarks exercise the
/// real parse and chunk path) while remaining cheap to produce. The number
/// of emitted functions scales with the file index so per-file entity density
/// stays roughly constant across the project.
pub fn gen_content(file_index: usize, language: &str) -> String {
    let fn_count = 8 + (file_index % 12);
    let mut out = String::with_capacity(256 + fn_count * 64);
    out.push_str(&format!(
        "// synthetic benchmark file {file_index} ({language})\n"
    ));
    match language {
        "python" => {
            out.push_str("from typing import List\n\n");
            for i in 0..fn_count {
                out.push_str(&format!(
                    "def fn_{i:05}(a: int) -> int:\n    return a + {i}\n\n"
                ));
            }
        }
        "typescript" => {
            out.push_str("export function helper(x: number): number { return x; }\n\n");
            for i in 0..fn_count {
                out.push_str(&format!(
                    "export function fn_{i:05}(a: number): number {{ return a + {i}; }}\n"
                ));
            }
        }
        "go" => {
            out.push_str("package main\n\n");
            for i in 0..fn_count {
                out.push_str(&format!("func fn_{i:05}(a int) int {{ return a + {i} }}\n"));
            }
        }
        _ => {
            // Rust / C / C++ / fallback: valid Rust function stubs.
            out.push_str("pub struct Ctx;\n\n");
            for i in 0..fn_count {
                out.push_str(&format!("pub fn fn_{i:05}(a: i32) -> i32 {{ a + {i} }}\n"));
            }
        }
    }
    out
}

impl SyntheticProject {
    /// Generate `size.file_count()` synthetic files using `dist`.
    ///
    /// `dist` is `'static` so the per-file `language` labels can be stored as
    /// `&'static str` (see [`SyntheticFile::language`]).
    pub fn generate(size: ProjectSize, dist: &'static [(&'static str, f64)]) -> Self {
        let n = size.file_count();
        let mut files = Vec::with_capacity(n);
        for i in 0..n {
            let lang = pick_language(xorshift(i as u64 + 1), dist);
            let content = gen_content(i, lang);
            let path = PathBuf::from(format!("src/{}/mod_{:06}.{}", lang, i, ext_for(lang)));
            files.push(SyntheticFile {
                path,
                language: lang,
                content,
            });
        }
        Self { size, files }
    }

    /// Materialize the project under `root`. Returns `root` for chaining.
    ///
    /// Intended to run once (via `std::sync::OnceLock`) before the timed
    /// loop; re-materializing on every iteration would dominate the
    /// measurement.
    pub fn materialize(&self, root: &Path) -> std::io::Result<PathBuf> {
        for f in &self.files {
            let p = root.join(&f.path);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&p, &f.content)?;
        }
        Ok(root.to_path_buf())
    }
}

/// Mean milliseconds per call: warms `f` once, then averages `iters` runs.
///
/// Every benchmark measures steady-state cost, so the single warmup call
/// keeps cold caches out of the reported number.
pub fn bench_ms(iters: usize, mut f: impl FnMut()) -> f64 {
    f();
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    start.elapsed().as_secs_f64() * 1000.0 / iters as f64
}

/// Report file path for the calling crate: `<manifest>/benches/results/<name>`.
///
/// Takes the caller's `env!("CARGO_MANIFEST_DIR")` because the macro would
/// otherwise resolve to this crate's own manifest directory.
pub fn results_path(manifest_dir: &str, file_name: &str) -> PathBuf {
    PathBuf::from(manifest_dir)
        .join("benches/results")
        .join(file_name)
}

/// Opens a report file for appending, creating the parent directory first.
///
/// Fails loudly when the file cannot be opened: a benchmark that cannot
/// record its own output is broken, and silently dropping rows hides that.
pub fn open_report(manifest_dir: &str, file_name: &str) -> fs::File {
    let path = results_path(manifest_dir, file_name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create benches/results");
    }
    match fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(file) => file,
        Err(e) => panic!("open {}: {e}", path.display()),
    }
}

/// Opens a report file and records the header line before any rows.
pub fn open_report_with_header(manifest_dir: &str, file_name: &str, header: &str) -> fs::File {
    let mut file = open_report(manifest_dir, file_name);
    use std::io::Write as _;
    match writeln!(file, "{header}") {
        Ok(()) => file,
        Err(e) => panic!("write report header {header:?}: {e}"),
    }
}

/// Single-function Rust stub shared by the small scanner fixtures.
///
/// Matches the historical per-file shape those benchmarks always used, so
/// adopting it keeps old and new baseline numbers comparable.
pub fn rust_single_fn(index: usize) -> String {
    format!("pub fn func_{index}(x: u64) -> u64 {{ x * 2 + {index} }}\n")
}

/// Writes `count` single-function files under `root/src/mod_XXX/`.
///
/// Covers the small fixture scale (a few hundred files). Larger scales use
/// [`SyntheticProject`] instead.
pub fn write_simple_tree(root: &Path, count: usize) -> std::io::Result<()> {
    for i in 0..count {
        let dir = root.join(format!("src/mod_{:03}", i % 10));
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(format!("file_{i:04}.rs")), rust_single_fn(i))?;
    }
    Ok(())
}

/// Fresh process-scoped scratch directory under the system temp directory.
///
/// Any previous run with the same process id is cleared first, then the
/// directory is created eagerly so callers never handle creation errors.
pub fn fresh_temp_dir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{prefix}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create bench temp dir");
    dir
}
