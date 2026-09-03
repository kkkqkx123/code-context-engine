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

/// Format score with color
pub fn format_score(score: f32) -> String {
    if score >= 0.8 {
        format!("{:.3}", score).green().to_string()
    } else if score >= 0.5 {
        format!("{:.3}", score).yellow().to_string()
    } else {
        format!("{:.3}", score).red().to_string()
    }
}

/// Truncate string to max length
pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
