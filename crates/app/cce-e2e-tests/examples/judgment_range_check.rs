//! Judgment range hygiene check across all benchmark projects.
//!
//! Iterates the central project registry and verifies every relevance range
//! against its fixture file. Only boundary conditions are reported: a range
//! starting or ending on a blank line can never be recovered, because chunk
//! source spans always start and end on code lines. Interior blank lines
//! are intentionally ignored: a single entity span covers them contiguously,
//! so they do not affect range coverage.

use cce_e2e_tests::BenchmarkProject;

struct Finding {
    project: &'static str,
    query: String,
    file: String,
    start_line: usize,
    end_line: usize,
    level: String,
    status: &'static str,
    detail: String,
}

fn check_project(project: BenchmarkProject, findings: &mut Vec<Finding>) {
    let root = project.target_spec().source_root();
    for judgment in project.judgments() {
        for (range, level) in &judgment.relevant_ranges {
            let path = root.join(&range.file);
            let file_label = range.file.clone();
            let level_label = format!("{level:?}");
            let content = match std::fs::read(&path) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                Err(_) => {
                    findings.push(Finding {
                        project: project.name(),
                        query: judgment.id.clone(),
                        file: file_label,
                        start_line: range.start_line,
                        end_line: range.end_line,
                        level: level_label,
                        status: "ERROR",
                        detail: format!("missing fixture file {}", path.display()),
                    });
                    continue;
                }
            };
            let lines: Vec<&str> = content.lines().collect();
            if range.start_line < 1
                || range.end_line < range.start_line
                || range.end_line > lines.len()
            {
                findings.push(Finding {
                    project: project.name(),
                    query: judgment.id.clone(),
                    file: file_label,
                    start_line: range.start_line,
                    end_line: range.end_line,
                    level: level_label,
                    status: "ERROR",
                    detail: format!("out of bounds, file has {} lines", lines.len()),
                });
                continue;
            }
            let start_blank = lines[range.start_line - 1].trim().is_empty();
            let end_blank = lines[range.end_line - 1].trim().is_empty();
            if start_blank || end_blank {
                let mut parts = Vec::new();
                if start_blank {
                    parts.push(format!("start {} blank", range.start_line));
                }
                if end_blank {
                    parts.push(format!("end {} blank", range.end_line));
                }
                findings.push(Finding {
                    project: project.name(),
                    query: judgment.id.clone(),
                    file: file_label,
                    start_line: range.start_line,
                    end_line: range.end_line,
                    level: level_label,
                    status: "ERROR",
                    detail: parts.join(", "),
                });
                continue;
            }
            findings.push(Finding {
                project: project.name(),
                query: judgment.id.clone(),
                file: file_label,
                start_line: range.start_line,
                end_line: range.end_line,
                level: level_label,
                status: "OK",
                detail: String::new(),
            });
        }
    }
}

fn main() -> anyhow::Result<()> {
    let mut findings = Vec::new();
    for project in BenchmarkProject::all() {
        check_project(project, &mut findings);
    }

    println!("| project | query | file | range | level | status | detail |");
    println!("|---|---|---|---|---|---|---|");
    for finding in &findings {
        println!(
            "| {} | {} | {} | {}-{} | {} | {} | {} |",
            finding.project,
            finding.query,
            finding.file,
            finding.start_line,
            finding.end_line,
            finding.level,
            finding.status,
            finding.detail
        );
    }

    let errors = findings
        .iter()
        .filter(|finding| finding.status == "ERROR")
        .count();
    eprintln!("checked {} ranges: {} errors", findings.len(), errors);
    if errors > 0 {
        std::process::exit(1);
    }
    Ok(())
}
