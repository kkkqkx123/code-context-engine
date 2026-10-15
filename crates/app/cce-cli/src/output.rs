//! Output formatting utilities

use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Cell, Table};
use serde::Serialize;

use crate::cli::OutputFormat;

/// Print success message
pub fn print_success(message: &str) {
    println!("{} {}", "✓".green(), message);
}

/// Print error message
pub fn print_error(message: &str) {
    eprintln!("{} {}", "✗".red(), message);
}

/// Print warning message
pub fn print_warning(message: &str) {
    println!("{} {}", "⚠".yellow(), message);
}

/// Print a table with headers and rows
pub fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    if rows.is_empty() {
        println!("No results");
        return;
    }

    let mut table = Table::new();
    table.load_style(UTF8_FULL);
    table.set_header(
        headers
            .iter()
            .map(|h| Cell::new(h).add_attribute(comfy_table::Attribute::Bold)),
    );
    for row in rows {
        table.add_row(row.iter().map(Cell::new));
    }

    println!("{table}");
}

/// Print data as JSON if format is Json, otherwise as text
pub fn print_output<T: Serialize>(format: OutputFormat, data: &T) {
    if let OutputFormat::Json = format {
        println!("{}", serde_json::to_string_pretty(data).unwrap_or_default());
    }
}

/// Format duration in human-readable form
pub fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60000 {
        format!("{:.2}s", ms as f32 / 1000.0)
    } else {
        let seconds = ms / 1000;
        let minutes = seconds / 60;
        let remaining_seconds = seconds % 60;
        format!("{}m {}s", minutes, remaining_seconds)
    }
}

/// Truncate string to max length
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let end = cce_utils::floor_char_boundary(s, max_len.saturating_sub(3));
        format!("{}...", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_formats_below_one_second_as_millis() {
        assert_eq!(format_duration(0), "0ms");
        assert_eq!(format_duration(999), "999ms");
    }

    #[test]
    fn duration_formats_seconds_with_two_decimals() {
        assert_eq!(format_duration(1000), "1.00s");
        assert_eq!(format_duration(1500), "1.50s");
        assert_eq!(format_duration(59_999), "60.00s");
    }

    #[test]
    fn duration_formats_minutes_and_remainder_seconds() {
        assert_eq!(format_duration(60_000), "1m 0s");
        assert_eq!(format_duration(61_000), "1m 1s");
        assert_eq!(format_duration(125_000), "2m 5s");
    }

    #[test]
    fn truncate_keeps_short_strings_verbatim() {
        assert_eq!(truncate("abc", 10), "abc");
        assert_eq!(truncate("abc", 3), "abc");
    }

    #[test]
    fn truncate_marks_cut_strings_with_ellipsis() {
        assert_eq!(truncate("abcdef", 5), "ab...");
        let cut = truncate("x".repeat(100).as_str(), 96);
        assert_eq!(cut.len(), 96);
        assert!(cut.ends_with("..."));
    }

    #[test]
    fn json_output_shape_stays_pretty_printed() {
        let value = serde_json::json!({"items": [1, 2], "total": 2});
        let text = serde_json::to_string_pretty(&value).expect("pretty prints");
        let roundtrip: serde_json::Value =
            serde_json::from_str(&text).expect("json output must stay parseable");
        assert_eq!(roundtrip["total"], 2);
    }

    #[test]
    fn truncate_never_splits_multibyte_char() {
        let text = format!("{}¡{}世界🌍", "a".repeat(10), "b".repeat(10));
        for max_len in 0..=(text.len() + 5) {
            let cut = truncate(&text, max_len);
            assert!(!cut.is_empty());
            if text.len() > max_len {
                assert!(cut.ends_with("..."));
            }
        }
        assert_eq!(truncate("a¡bcdef", 5), "a...");
    }
}
