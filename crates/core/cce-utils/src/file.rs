//! File utilities

use std::path::Path;

use crate::encoding::{Detector, Encoder};

/// Common text file extensions
const TEXT_EXTENSIONS: &[&str] = &[
    // Programming languages
    "rs",
    "py",
    "js",
    "ts",
    "jsx",
    "tsx",
    "java",
    "c",
    "cpp",
    "h",
    "hpp",
    "cs",
    "go",
    "rb",
    "php",
    "swift",
    "kt",
    "scala",
    "r",
    "m",
    "mm",
    // Web
    "html",
    "htm",
    "css",
    "scss",
    "sass",
    "less",
    "xml",
    "svg",
    // Data formats
    "json",
    "yaml",
    "yml",
    "toml",
    "ini",
    "conf",
    "config",
    // Scripts
    "sh",
    "bash",
    "zsh",
    "fish",
    "ps1",
    "bat",
    "cmd",
    // Documentation
    "md",
    "txt",
    "rst",
    "adoc",
    "tex",
    "latex",
    // Configuration
    "dockerfile",
    "makefile",
    "cmake",
    "gradle",
    "properties",
    // Version control
    "gitignore",
    "gitattributes",
    "editorconfig",
];

/// Common binary file extensions (to exclude)
const BINARY_EXTENSIONS: &[&str] = &[
    // Images
    "png", "jpg", "jpeg", "gif", "bmp", "ico", "webp", "svgz", // Archives
    "zip", "tar", "gz", "bz2", "7z", "rar", "jar", "war", // Executables
    "exe", "dll", "so", "dylib", "bin", "msi", "pkg", "deb", "rpm", // Documents
    "pdf", "doc", "docx", "ppt", "pptx", "xls", "xlsx", // Media
    "mp3", "mp4", "avi", "mov", "wmv", "flv", "wav", "ogg", // Compiled
    "o", "obj", "a", "lib", "pyc", "class", // Database
    "db", "sqlite", "sqlite3", "mdb", // Other binary
    "iso", "img", "dmg", "nupkg",
];

/// Byte-level text classification shared by local and remote supply paths.
///
/// Empty input counts as text. Content without NUL bytes counts as text.
/// Content with NUL bytes counts as text only when it has the alternating-NUL
/// shape of BOM-less UTF-16 text with printable ASCII on the other parity,
/// matching the encoding detector so the pre-check and the decoder agree.
pub fn is_text_bytes(data: &[u8]) -> bool {
    if data.is_empty() {
        return true;
    }
    let check_len = data.len().min(8192);
    let sample = &data[..check_len];
    if !sample.contains(&0) {
        return true;
    }
    looks_like_utf16_without_bom(sample)
}

/// Alternating-NUL shape with printable ASCII on the other parity.
///
/// Thresholds mirror the encoding detector so wide text is not misclassified
/// as binary.
fn looks_like_utf16_without_bom(sample: &[u8]) -> bool {
    if sample.len() < 4 {
        return false;
    }
    let mut nul_even = 0usize;
    let mut nul_odd = 0usize;
    let mut printable = 0usize;
    let mut checked = 0usize;
    for pair in sample.chunks_exact(2) {
        let (a, b) = (pair[0], pair[1]);
        if a == 0 {
            nul_even += 1;
        } else if matches!(a, 0x09 | 0x0A | 0x0D | 0x20..=0x7E) {
            printable += 1;
        }
        if b == 0 {
            nul_odd += 1;
        } else if matches!(b, 0x09 | 0x0A | 0x0D | 0x20..=0x7E) {
            printable += 1;
        }
        checked += 1;
    }
    if checked == 0 {
        return false;
    }
    let nul_total = nul_even + nul_odd;
    if nul_total * 10 < checked * 4 {
        return false;
    }
    if nul_even.max(nul_odd) * 10 < nul_total * 9 {
        return false;
    }
    printable * 2 >= checked
}

/// Check if file is text file
///
/// This function uses two strategies:
/// 1. Check file extension against known text/binary extensions
/// 2. If extension is unknown, read the file once and classify the bytes
///    with [`is_text_bytes`].
///
/// # Arguments
///
/// * `path` - Path to the file
///
/// # Returns
///
/// `true` if the file appears to be text, `false` otherwise
pub fn is_text_file(path: &Path) -> bool {
    // First check: file extension
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();

        // If it's a known text extension, return true
        if TEXT_EXTENSIONS.contains(&ext.as_str()) {
            return true;
        }

        // If it's a known binary extension, return false
        if BINARY_EXTENSIONS.contains(&ext.as_str()) {
            return false;
        }
    }

    // Second check: read once and classify the bytes with the shared core.
    match std::fs::read(path) {
        Ok(content) => is_text_bytes(&content),
        Err(_) => false,
    }
}

/// Check if file extension indicates a text file
pub fn has_text_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| TEXT_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Get file size in human-readable format
pub fn format_file_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = size as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    format!("{:.2} {}", size, UNITS[unit_index])
}

/// Read file and convert to UTF-8 with automatic encoding detection
///
/// This function:
/// 1. Reads file bytes
/// 2. Detects encoding using Detector
/// 3. Converts to UTF-8 using Encoder
///
/// # Arguments
/// * `path` - Path to the file
///
/// # Returns
/// * `Ok(String)` - UTF-8 encoded content
/// * `Err(String)` - Error message if reading or conversion fails
pub fn read_file_to_utf8(path: &Path) -> Result<String, String> {
    // Read raw bytes
    let data = std::fs::read(path)
        .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e))?;

    read_bytes_to_utf8(&data, path)
}

/// Read file asynchronously and convert to UTF-8 with automatic encoding detection
///
/// This async version is suitable for use in async contexts.
///
/// # Arguments
/// * `path` - Path to the file
///
/// # Returns
/// * `Ok(String)` - UTF-8 encoded content
/// * `Err(String)` - Error message if reading or conversion fails
pub async fn read_file_to_utf8_async(path: &Path) -> Result<String, String> {
    // Read raw bytes asynchronously
    let data = tokio::fs::read(path)
        .await
        .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e))?;

    read_bytes_to_utf8(&data, path)
}

/// Read file asynchronously and convert to UTF-8, returning both content and detected encoding
///
/// This is useful when you need to know the original encoding (e.g., for API responses).
///
/// # Arguments
/// * `path` - Path to the file
///
/// # Returns
/// * `Ok((String, String))` - Tuple of (UTF-8 content, detected encoding name)
/// * `Err(String)` - Error message if reading or conversion fails
pub async fn read_file_to_utf8_with_encoding_async(
    path: &Path,
) -> Result<(String, String), String> {
    // Read raw bytes asynchronously
    let data = tokio::fs::read(path)
        .await
        .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e))?;

    read_bytes_to_utf8_with_encoding(&data, path)
}

/// Convert already-read bytes to UTF-8 with automatic encoding detection.
///
/// Counterpart of [`read_file_to_utf8`] for callers that need the raw bytes
/// first (e.g. content-hash verification before decoding).
///
/// # Arguments
/// * `data` - Raw file bytes
/// * `path` - Path to the file (for error messages)
pub fn decode_bytes_to_utf8(data: &[u8], path: &Path) -> Result<String, String> {
    read_bytes_to_utf8(data, path)
}

/// Convert bytes to UTF-8 with automatic encoding detection
///
/// Internal helper function used by both sync and async versions.
///
/// # Arguments
/// * `data` - Raw file bytes
/// * `path` - Path to the file (for error messages)
///
/// # Returns
/// * `Ok(String)` - UTF-8 encoded content
/// * `Err(String)` - Error message if conversion fails
fn read_bytes_to_utf8(data: &[u8], path: &Path) -> Result<String, String> {
    let (content, _) = read_bytes_to_utf8_with_encoding(data, path)?;
    Ok(content)
}

/// Convert bytes to UTF-8 with automatic encoding detection, returning encoding info
///
/// Internal helper function that returns both content and detected encoding.
///
/// # Arguments
/// * `data` - Raw file bytes
/// * `path` - Path to the file (for error messages)
///
/// # Returns
/// * `Ok((String, String))` - Tuple of (UTF-8 content, detected encoding name)
/// * `Err(String)` - Error message if conversion fails
fn read_bytes_to_utf8_with_encoding(data: &[u8], path: &Path) -> Result<(String, String), String> {
    // Detect encoding
    let detector = Detector::with_default_config();
    let encoding_result = detector
        .detect_bytes(data)
        .map_err(|e| format!("Failed to detect encoding for '{}': {}", path.display(), e))?;

    // Convert to UTF-8
    let encoder = Encoder::default();
    let content = encoder
        .to_utf8(data, &encoding_result.encoding)
        .map_err(|e| format!("Failed to convert '{}' to UTF-8: {}", path.display(), e))?;

    Ok((content, encoding_result.encoding))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_text_extension() {
        assert!(has_text_extension(Path::new("test.rs")));
        assert!(has_text_extension(Path::new("test.py")));
        assert!(has_text_extension(Path::new("test.md")));
        assert!(!has_text_extension(Path::new("test.exe")));
        assert!(!has_text_extension(Path::new("test")));
    }

    #[test]
    fn test_format_file_size() {
        assert_eq!(format_file_size(0), "0.00 B");
        assert_eq!(format_file_size(1024), "1.00 KB");
        assert_eq!(format_file_size(1024 * 1024), "1.00 MB");
    }

    #[test]
    fn test_read_file_to_utf8_with_utf8() {
        let temp_file = tempfile::NamedTempFile::new().expect("should create temp file");
        let content = "Hello World UTF-8 Content";
        std::fs::write(temp_file.path(), content).expect("should write file");

        let result = read_file_to_utf8(temp_file.path());
        assert!(result.is_ok());
        assert_eq!(result.expect("should read file"), content);
    }

    #[test]
    fn test_read_file_to_utf8_with_gbk() {
        let temp_file = tempfile::NamedTempFile::new().expect("should create temp file");
        // GBK bytes of a Chinese greeting (decodes to the assertion below).
        let gbk_data = [0xC4, 0xE3, 0xBA, 0xC3, 0xCA, 0xC0, 0xBD, 0xE7];
        std::fs::write(temp_file.path(), gbk_data).expect("should write file");

        let result = read_file_to_utf8(temp_file.path());
        assert!(result.is_ok());
        assert_eq!(result.expect("should read file"), "你好世界");
    }

    #[test]
    fn test_read_file_to_utf8_with_bom() {
        let temp_file = tempfile::NamedTempFile::new().expect("should create temp file");
        // UTF-8 with BOM
        let mut data = vec![0xEF, 0xBB, 0xBF];
        data.extend_from_slice(b"Hello World");
        std::fs::write(temp_file.path(), &data).expect("should write file");

        let result = read_file_to_utf8(temp_file.path());
        assert!(result.is_ok());
        assert_eq!(result.expect("should read file"), "Hello World");
    }

    #[test]
    fn test_read_file_to_utf8_not_exists() {
        let result = read_file_to_utf8(Path::new("nonexistent_file.txt"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to read file"));
    }

    #[test]
    fn test_is_text_bytes_core() {
        assert!(is_text_bytes(b""));
        assert!(is_text_bytes(b"Hello, World!\n"));
        assert!(!is_text_bytes(b"Hello\x00World"));
        assert!(!is_text_bytes(&[0u8; 100]));
    }
}
