//! Metrics command handler

use anyhow::Result;
use chrono::{Duration, Utc};

use crate::client::ApiClient;
use crate::output::{print_error, print_success};
use cce_api::models::MetricsHistoryResponse;

pub enum MetricsFormat {
    Prometheus,
    Json,
}

pub async fn execute(format: MetricsFormat, server: &str, verbose: bool) -> Result<()> {
    let path = match format {
        MetricsFormat::Prometheus => "/api/metrics",
        MetricsFormat::Json => "/api/metrics/json",
    };

    if verbose {
        println!("Fetching metrics from: {}", path);
    }

    let response = reqwest::get(format!("{}{}", server, path)).await?;

    if !response.status().is_success() {
        print_error(&format!("Failed to fetch metrics: {}", response.status()));
        return Ok(());
    }

    let content = response.text().await?;
    println!("{}", content);

    Ok(())
}

fn default_history_window() -> (String, String) {
    let to = Utc::now();
    let from = to - Duration::hours(1);
    (from.to_rfc3339(), to.to_rfc3339())
}

pub async fn execute_history(
    from: Option<&str>,
    to: Option<&str>,
    metric: Option<&str>,
    project_id: Option<i64>,
    operation_type: Option<&str>,
    server: &str,
    verbose: bool,
) -> Result<()> {
    let client = ApiClient::new(server)?;

    let (default_from, default_to) = default_history_window();
    let from = from.unwrap_or(&default_from);
    let to = to.unwrap_or(&default_to);

    let mut path = format!(
        "/api/metrics/history?from={}&to={}",
        urlencoding::encode(from),
        urlencoding::encode(to)
    );
    if let Some(metric) = metric {
        path.push_str(&format!("&metric={}", urlencoding::encode(metric)));
    }
    if let Some(project_id) = project_id {
        path.push_str(&format!("&project_id={}", project_id));
    }
    if let Some(operation_type) = operation_type {
        path.push_str(&format!(
            "&operation_type={}",
            urlencoding::encode(operation_type)
        ));
    }

    if verbose {
        println!("Fetching metrics history from {} to {}...", from, to);
    }

    let response: Vec<MetricsHistoryResponse> = client.get(&path).await?;

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

pub async fn execute_cleanup(
    all: bool,
    before: Option<&str>,
    keep_days: u64,
    server: &str,
    verbose: bool,
) -> Result<()> {
    let client = ApiClient::new(server)?;

    if verbose {
        if all {
            println!("Cleaning up all metrics records...");
        } else if let Some(before) = before {
            println!("Cleaning up metrics before {}...", before);
        } else {
            println!("Cleaning up metrics older than {} days...", keep_days);
        }
    }

    let response: serde_json::Value = if all {
        client.delete("/api/metrics/cleanup?all=true").await?
    } else {
        let before_timestamp = if let Some(before) = before {
            before.to_string()
        } else {
            (Utc::now() - Duration::days(keep_days as i64)).to_rfc3339()
        };
        client
            .delete(&format!(
                "/api/metrics/cleanup?before={}",
                urlencoding::encode(&before_timestamp)
            ))
            .await?
    };

    if response["success"].as_bool().unwrap_or(false) {
        if let Some(deleted) = response["deleted_count"].as_u64() {
            print_success(&format!("Cleaned up {} metrics records", deleted));
        } else {
            print_success("Metrics cleanup completed");
        }
    } else {
        print_error("Failed to clean up metrics");
    }

    Ok(())
}
