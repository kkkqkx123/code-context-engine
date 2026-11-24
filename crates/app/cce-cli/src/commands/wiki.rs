//! Wiki context export command handlers

use clap::Subcommand;

use anyhow::Result;

use crate::error::{fail, fail_client};
use crate::output::{print_output, print_success};
use cce_api::models::{WikiContextQuery, WikiDiffRequest, WikiFileHash};

/// Execute wiki commands through the client abstraction.
pub async fn execute_with_client(
    cmd: &WikiCommands,
    client: &cce_client::Client,
    verbose: bool,
    format: crate::cli::OutputFormat,
) -> Result<()> {
    match cmd {
        WikiCommands::Context {
            project_id,
            external_deps_mode,
            include_snippets,
            no_mermaid,
            output,
        } => {
            if verbose {
                println!("Exporting wiki context package: project {project_id}");
            }
            let query = WikiContextQuery {
                external_deps_mode: external_deps_mode.clone(),
                include_snippets: if *include_snippets { Some(true) } else { None },
                include_mermaid: if *no_mermaid { Some(false) } else { None },
            };
            match client.wiki_context(*project_id, &query).await {
                Ok(response) => {
                    if !response.success {
                        return Err(fail("Wiki context export failed"));
                    }
                    if let Some(path) = output {
                        let json = serde_json::to_string_pretty(&response)
                            .map_err(|e| fail(format!("Wiki context serialization failed: {e}")))?;
                        std::fs::write(path, json)
                            .map_err(|e| fail(format!("Wiki context write failed: {e}")))?;
                        print_success(&format!(
                            "Wrote wiki context package to {path} ({} files, {} entities)",
                            response.manifest.file_count, response.manifest.entity_count
                        ));
                    } else {
                        print_output(format, &response);
                    }
                }
                Err(e) => return Err(fail_client("Wiki context export failed", &e)),
            }
        }
        WikiCommands::Diff {
            project_id,
            baseline,
        } => {
            if verbose {
                println!("Computing wiki invalidation diff: project {project_id}");
            }
            let raw = std::fs::read_to_string(baseline)
                .map_err(|e| fail(format!("Baseline read failed: {e}")))?;
            let entries: Vec<WikiFileHash> = serde_json::from_str(&raw).map_err(|e| {
                fail(format!(
                    "Baseline must be a JSON array of {{path, content_hash}}: {e}"
                ))
            })?;
            let request = WikiDiffRequest { baseline: entries };
            match client.wiki_diff(*project_id, &request).await {
                Ok(response) => {
                    if !response.success {
                        return Err(fail("Wiki diff failed"));
                    }
                    if matches!(format, crate::cli::OutputFormat::Json) {
                        print_output(format, &response);
                    } else {
                        print_success(&format!(
                            "{} change(s), {} affected file(s), {} affected module(s)",
                            response.changes.len(),
                            response.affected_files.len(),
                            response.affected_modules.len()
                        ));
                        for change in &response.changes {
                            println!("  [{}] {}", change.change, change.path);
                        }
                        for module in &response.affected_modules {
                            println!("  module: {module}");
                        }
                    }
                }
                Err(e) => return Err(fail_client("Wiki diff failed", &e)),
            }
        }
    }
    Ok(())
}

/// Wiki context export operations
#[derive(Subcommand)]
pub enum WikiCommands {
    /// Export the Wiki Context Package of a project
    Context {
        /// Project ID
        #[arg(long)]
        project_id: i64,

        /// External dependency presentation mode: symbol or package
        #[arg(long)]
        external_deps_mode: Option<String>,

        /// Include short source snippets in entity entries
        #[arg(long, default_value_t = false)]
        include_snippets: bool,

        /// Omit the deterministic Mermaid graph skeletons
        #[arg(long, default_value_t = false)]
        no_mermaid: bool,

        /// Write the package to this JSON file instead of stdout
        #[arg(long)]
        output: Option<String>,
    },

    /// Diff baseline file hashes into a wiki page invalidation list
    Diff {
        /// Project ID
        #[arg(long)]
        project_id: i64,

        /// Baseline JSON file: array of {path, content_hash}
        #[arg(long)]
        baseline: String,
    },
}
