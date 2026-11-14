//! Metrics command handler

use clap::Subcommand;

use anyhow::Result;
use chrono::{Duration, Utc};

use crate::output::{print_error, print_success};
use cce_api::models::{MetricsCleanupQuery, MetricsHistoryQuery};
use cce_client::Client;

pub enum MetricsFormat {
    Prometheus,
    Json,
}

pub async fn execute_with_client(
    format: MetricsFormat,
    client: &Client,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!(
            "Fetching metrics as {}",
            match &format {
                MetricsFormat::Prometheus => "Prometheus text",
                MetricsFormat::Json => "JSON",
            }
        );
    }

    let content = match format {
        MetricsFormat::Prometheus => match client.metrics_prometheus().await {
            Ok(content) => content,
            Err(error) => {
                print_error(&format!("Failed to fetch metrics: {error}"));
                return Ok(());
            }
        },
        MetricsFormat::Json => match client.metrics_json().await {
            Ok(snapshot) => serde_json::to_string_pretty(&snapshot).unwrap_or_default(),
            Err(error) => {
                print_error(&format!("Failed to fetch metrics: {error}"));
                return Ok(());
            }
        },
    };
    println!("{}", content);

    Ok(())
}

fn default_history_window() -> (String, String) {
    let to = Utc::now();
    let from = to - Duration::hours(1);
    (from.to_rfc3339(), to.to_rfc3339())
}

pub async fn execute_history_with_client(
    from: Option<&str>,
    to: Option<&str>,
    metric: Option<&str>,
    project_id: Option<i64>,
    operation_type: Option<&str>,
    client: &Client,
    verbose: bool,
) -> Result<()> {
    let (default_from, default_to) = default_history_window();
    let from = from.unwrap_or(&default_from);
    let to = to.unwrap_or(&default_to);

    let query = MetricsHistoryQuery {
        from: from.to_string(),
        to: to.to_string(),
        metric: metric.map(str::to_string),
        project_id,
        operation_type: operation_type.map(str::to_string),
    };

    if verbose {
        println!("Fetching metrics history from {} to {}...", from, to);
    }

    let response = client.metrics_history(&query).await?;

    if response.is_empty() {
        print_success("No historical records found");
        return Ok(());
    }

    print_success(&format!("Found {} historical records", response.len()));
    println!();

    for (i, record) in response.iter().enumerate() {
        println!(
            "  {:>3}. [{}] {}",
            i + 1,
            record.timestamp,
            record.metric_name
        );

        if let Some(labels_json) = &record.labels_json {
            println!("       labels: {}", labels_json);
        }
        println!("       count: {}", record.count);
        if let Some(avg) = record.avg {
            println!("       avg: {:.4}", avg);
        }
        if let Some(median) = record.median {
            println!("       median: {:.4}", median);
        }
        if let Some(max) = record.max {
            println!("       max: {:.4}", max);
        }
        if let Some(p90) = record.p90 {
            println!("       p90: {:.4}", p90);
        }
        if let Some(p99) = record.p99 {
            println!("       p99: {:.4}", p99);
        }
        if let Some(project_id) = record.project_id {
            println!("       project_id: {}", project_id);
        }
        if let Some(operation_type) = &record.operation_type {
            println!("       operation_type: {}", operation_type);
        }
    }

    Ok(())
}

pub async fn execute_cleanup_with_client(
    all: bool,
    before: Option<&str>,
    keep_days: u64,
    client: &Client,
    verbose: bool,
) -> Result<()> {
    if verbose {
        if all {
            println!("Cleaning up all metrics records...");
        } else if let Some(before) = before {
            println!("Cleaning up metrics before {}...", before);
        } else {
            println!("Cleaning up metrics older than {} days...", keep_days);
        }
    }

    let before = if all {
        None
    } else {
        Some(match before {
            Some(before) => before.to_string(),
            None => (Utc::now() - Duration::days(keep_days as i64)).to_rfc3339(),
        })
    };
    let response = client
        .cleanup_metrics(&MetricsCleanupQuery { all, before })
        .await?;

    if response.success {
        print_success(&format!(
            "Cleaned up {} metrics records",
            response.deleted_count
        ));
    } else {
        print_error("Failed to clean up metrics");
    }

    Ok(())
}

/// Metrics commands
#[derive(Subcommand)]
pub enum MetricsCommands {
    /// Export metrics in Prometheus format
    Prometheus,

    /// Export metrics in JSON format
    Json,

    /// Get metrics history
    History {
        /// Start time in RFC3339 format
        #[arg(long)]
        from: Option<String>,

        /// End time in RFC3339 format
        #[arg(long)]
        to: Option<String>,

        /// Metric name filter
        #[arg(long)]
        metric: Option<String>,

        /// Project ID filter
        #[arg(long)]
        project_id: Option<i64>,

        /// Operation type filter
        #[arg(long)]
        operation_type: Option<String>,
    },

    /// Clean up old metrics data
    Cleanup {
        /// Delete all historical metrics
        #[arg(long)]
        all: bool,

        /// Delete records before this RFC3339 timestamp
        #[arg(long)]
        before: Option<String>,

        /// Keep data for the last N days
        #[arg(short, long, default_value = "30")]
        keep_days: u64,
    },
}
