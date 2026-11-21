//! CCE CLI - Command Line Interface for Code Context Engine
//!
//! Two responsibilities live side by side here:
//!
//! - **HTTP client**: a standalone CLI that talks to a running CCE server
//!   through the API client crate.
//! - **Local operations entrypoint**: process supervision for the server
//!   binary and the embedded gateway. This is a controlled local-deployment
//!   exception, not a place to add business logic; domain behavior belongs to
//!   the shared application services.

mod cli;
mod commands;
mod error;
mod icons;
mod output;

use clap::Parser;
use colored::Colorize;

use crate::cli::{Cli, OutputFormat};
use crate::error::CliError;

#[tokio::main]
async fn main() {
    // Parse command line arguments
    let cli = Cli::parse();
    let format = cli.format;

    // Initialize logger from configuration
    let _ = cce_config::Settings::init_with_fallback(None);
    if let Ok(cfg) = cce_config::Settings::logger() {
        let _ = cce_logging::init(&cfg);
    } else {
        tracing_subscriber::fmt::init();
    }

    // Execute command, then render any failure and set the process status.
    // Success payloads stay on stdout; failures are reported on stderr only.
    if let Err(failure) = cli.execute().await {
        report_failure(&failure, format);
        let status = failure
            .downcast_ref::<CliError>()
            .map_or(1, CliError::exit_code);
        std::process::exit(status);
    }
}

/// Extract the wire code and message from a command failure. Failures that did
/// not originate from `CliError` fall back to `INTERNAL_ERROR` with their text.
fn failure_parts(failure: &anyhow::Error) -> (&'static str, String) {
    if let Some(error) = failure.downcast_ref::<CliError>() {
        return (error.code, error.message.clone());
    }
    (cce_types::error_codes::INTERNAL_ERROR, failure.to_string())
}

/// Render a command failure to stderr. In JSON mode emit a structured envelope
/// so agents can parse the wire code without reading human-oriented prose.
fn report_failure(failure: &anyhow::Error, format: OutputFormat) {
    let (code, message) = failure_parts(failure);

    if matches!(format, OutputFormat::Json) {
        eprintln!(
            "{}",
            serde_json::json!({
                "error": { "code": code, "message": message }
            })
        );
    } else {
        eprintln!("{} {}", icons::ERROR.red(), message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{fail, fail_input};

    #[test]
    fn domain_failure_exposes_its_wire_code() {
        let failure = fail_input("bad project id");
        let (code, message) = failure_parts(&failure);
        assert_eq!(code, cce_types::error_codes::INVALID_INPUT);
        assert_eq!(message.as_str(), "bad project id");
        assert_eq!(
            failure
                .downcast_ref::<CliError>()
                .map_or(1, CliError::exit_code),
            2
        );
    }

    #[test]
    fn plain_failure_falls_back_to_internal_error() {
        let failure = anyhow::anyhow!("plain failure");
        let (code, message) = failure_parts(&failure);
        assert_eq!(code, cce_types::error_codes::INTERNAL_ERROR);
        assert_eq!(message.as_str(), "plain failure");
    }

    #[test]
    fn query_failure_maps_to_generic_exit_status() {
        let failure = fail("query failed");
        assert_eq!(
            failure
                .downcast_ref::<CliError>()
                .map_or(1, CliError::exit_code),
            1
        );
    }
}
