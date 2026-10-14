//! Shared benchmark support for code-context-engine performance benchmarks.
//!
//! Provides a deterministic synthetic project generator and the fixed scale
//! tiers (small / medium / large) so every benchmark (T-1 .. T-10) speaks the
//! same scale vocabulary. See `docs/benchmark/design.md` §5 for the conventions.
//!
//! The generator is dependency-free (std only): it produces plain file paths,
//! languages and content. Each benchmark converts the synthetic project into
//! the typed inputs its crate needs, keeping this crate cheap to compile and
//! a single source of truth for tier sizes.

use std::fs;
use std::path::{Path, PathBuf};

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
/// Content is valid enough for tree-sitter to parse (so T-1/T-3/T-9 exercise
/// the real parse + chunk path) while remaining cheap to produce. The number
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
    /// criterion loop; re-materializing on every iteration would dominate the
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
