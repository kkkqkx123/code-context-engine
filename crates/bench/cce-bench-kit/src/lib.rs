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
use std::io::{self, BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

mod resource;
pub use resource::{ResourceDelta, ResourceSnapshot, cpu_ms, resource_delta, resource_snapshot};

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

/// Opens a report file and guarantees its first line matches `header`.
///
/// A schema change resets the trend file instead of appending rows under a
/// stale header; mixed-shape tables are not comparable and must not survive.
pub fn open_report_at_with_header(path: &Path, header: &str) -> fs::File {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create report directory");
    }
    let mut file = match fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path)
    {
        Ok(file) => file,
        Err(e) => panic!("open {}: {e}", path.display()),
    };
    let mut existing = String::new();
    file.seek(SeekFrom::Start(0)).expect("rewind report");
    if BufReader::new(&file)
        .read_line(&mut existing)
        .expect("read report header")
        > 0
        && existing.trim_end() == header
    {
        file.seek(SeekFrom::End(0)).expect("seek report end");
        return file;
    }
    file.set_len(0).expect("reset report with stale header");
    file.seek(SeekFrom::Start(0)).expect("rewind reset report");
    writeln!(file, "{header}").expect("write report header");
    file
}

/// Opens a benchmark report under `benches/results` with a checked header.
pub fn open_report_with_header(manifest_dir: &str, file_name: &str, header: &str) -> fs::File {
    open_report_at_with_header(&results_path(manifest_dir, file_name), header)
}

/// Header line for a TSV report declared by a single column list.
fn tsv_header(columns: &[&str]) -> String {
    format!("# {}", columns.join("\t"))
}

/// TSV report whose header and rows come from one column list.
///
/// Benchmarks declare columns once; [`write_row`](Self::write_row) validates
/// the value count before writing, so a schema change cannot silently shift
/// later columns or append rows under a stale header.
pub struct TsvReport {
    file: fs::File,
    columns: &'static [&'static str],
}

impl TsvReport {
    /// Open a report under `<manifest>/benches/results` with a checked header.
    pub fn open(manifest_dir: &str, file_name: &str, columns: &'static [&'static str]) -> Self {
        Self::open_at(&results_path(manifest_dir, file_name), columns)
    }

    /// Open a report at `path` with a checked header.
    pub fn open_at(path: &Path, columns: &'static [&'static str]) -> Self {
        let file = open_report_at_with_header(path, &tsv_header(columns));
        Self { file, columns }
    }

    /// Column names of this report, in row order.
    pub fn columns(&self) -> &'static [&'static str] {
        self.columns
    }

    /// Write one row. Values must be in the same order as `columns`.
    pub fn write_row(&mut self, values: &[String]) -> io::Result<()> {
        if values.len() != self.columns.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "tsv row has {} values but header has {} columns: {}",
                    values.len(),
                    self.columns.len(),
                    self.columns.join(",")
                ),
            ));
        }
        if let Some((index, _value)) = values
            .iter()
            .enumerate()
            .find(|(_, value)| value.chars().any(|c| matches!(c, '\t' | '\n' | '\r')))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "tsv value for '{}' contains a tab or newline",
                    self.columns[index]
                ),
            ));
        }
        writeln!(self.file, "{}", values.join("\t"))
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

/// Process-scoped scratch directory that removes itself when it goes out of scope.
///
/// Any previous run with the same process id is cleared first, so an
/// interrupted benchmark cannot leave a stale fixture behind for the next one.
#[derive(Debug)]
pub struct BenchTempDir(PathBuf);

impl BenchTempDir {
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for BenchTempDir {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

impl std::ops::Deref for BenchTempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        self.path()
    }
}

impl Drop for BenchTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Create a fresh scratch directory for one benchmark process.
pub fn fresh_temp_dir(prefix: &str) -> BenchTempDir {
    let dir = std::env::temp_dir().join(format!("{prefix}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create bench temp dir");
    BenchTempDir(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("cce-bench-kit-{}-{name}.tsv", std::process::id()))
    }

    #[test]
    fn tsv_report_writes_header_and_rows_from_one_schema() {
        let path = report_path("rows");
        let _ = fs::remove_file(&path);
        let mut report = TsvReport::open_at(&path, &["tier", "files"]);
        report
            .write_row(&["small".to_string(), "500".to_string()])
            .expect("row must be written");
        drop(report);

        let content = fs::read_to_string(&path).expect("report must be readable");
        assert_eq!(content, "# tier\tfiles\nsmall\t500\n");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn tsv_report_replaces_rows_when_schema_changes() {
        let path = report_path("schema");
        let _ = fs::remove_file(&path);
        let mut report = TsvReport::open_at(&path, &["a", "b"]);
        report
            .write_row(&["old".to_string(), "row".to_string()])
            .expect("old row must be written");
        drop(report);

        let mut report = TsvReport::open_at(&path, &["a", "b", "c"]);
        report
            .write_row(&["new".to_string(), "row".to_string(), "three".to_string()])
            .expect("new row must be written");
        drop(report);

        let content = fs::read_to_string(&path).expect("report must be readable");
        assert_eq!(content, "# a\tb\tc\nnew\trow\tthree\n");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn tsv_report_rejects_wrong_width_and_embedded_delimiters() {
        let path = report_path("validation");
        let _ = fs::remove_file(&path);
        let mut report = TsvReport::open_at(&path, &["a", "b"]);

        let width_error = report
            .write_row(&["only-one".to_string()])
            .expect_err("wrong width must fail");
        assert_eq!(width_error.kind(), io::ErrorKind::InvalidInput);

        let delimiter_error = report
            .write_row(&["has\ttab".to_string(), "ok".to_string()])
            .expect_err("embedded tab must fail");
        assert_eq!(delimiter_error.kind(), io::ErrorKind::InvalidInput);
        drop(report);

        let content = fs::read_to_string(&path).expect("report must be readable");
        assert_eq!(content, "# a\tb\n");
        let _ = fs::remove_file(&path);
    }
}
