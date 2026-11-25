//! Log management handlers (viewing, searching, file management, config).

use axum::{
    Json,
    extract::{Path, Query, WebSocketUpgrade},
    http::{StatusCode, header},
    response::IntoResponse,
};
use std::path::Path as StdPath;

use cce_api::models::{
    ErrorResponse, LogConfig, LogEntry, LogFileInfo, LogsQuery, LogsResponse,
    UpdateLogConfigRequest,
};
use cce_config::Settings;
use cce_logging::{recent, subscribe};

/// GET /api/logs
///
/// One page of the in-memory log buffer, filtered server side.
#[utoipa::path(
    get, path = "/api/logs", tag = "Logs",
    params(LogsQuery),
    responses(
        (status = 200, body = LogsResponse, description = "Success"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_get_logs(Query(q): Query<LogsQuery>) -> Json<LogsResponse> {
    let mut entries = recent(q.level.as_deref(), q.module.as_deref(), q.query.as_deref());

    // Time range filter
    if let Some(start) = &q.start_time {
        entries.retain(|e| e.timestamp.as_str() >= start.as_str());
    }
    if let Some(end) = &q.end_time {
        entries.retain(|e| e.timestamp.as_str() <= end.as_str());
    }

    let total = entries.len();
    let page = q.page.unwrap_or(1).max(1);
    let page_size = q.page_size.unwrap_or(100).clamp(1, 1000);
    let start_idx = (page - 1) * page_size;
    let page_entries: Vec<LogEntry> = entries
        .into_iter()
        .skip(start_idx)
        .take(page_size)
        .map(|event| LogEntry {
            timestamp: event.timestamp,
            level: event.level,
            target: event.target,
            message: event.message,
            fields: event.fields,
        })
        .collect();

    Json(LogsResponse {
        entries: page_entries,
        total,
        page,
        page_size,
    })
}

/// GET /api/logs/files
///
/// Files currently present in the configured log directory.
#[utoipa::path(
    get, path = "/api/logs/files", tag = "Logs",
    responses(
        (status = 200, body = Vec<LogFileInfo>, description = "Success"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_list_log_files() -> Result<Json<Vec<LogFileInfo>>, StatusCode> {
    let cfg = Settings::logger().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let file_path = cfg.file.clone().unwrap_or_default();
    let dir = StdPath::new(&file_path)
        .parent()
        .unwrap_or(StdPath::new("."))
        .to_path_buf();
    let prefix = StdPath::new(&file_path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            if !name.starts_with(&prefix) {
                continue;
            }
            let meta = std::fs::metadata(&path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let modified = meta
                .modified()
                .ok()
                .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
                .unwrap_or_default();
            files.push(LogFileInfo {
                name,
                size: meta.len(),
                modified,
            });
        }
    }
    files.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(Json(files))
}

/// GET /api/logs/files/{name}/download
///
/// Raw bytes of one log file as an attachment.
#[utoipa::path(
    get, path = "/api/logs/files/{name}/download", tag = "Logs",
    params(("name" = String, Path, description = "Log file name")),
    responses(
        (status = 200, description = "File contents", content_type = "application/octet-stream"),
        (status = 400, body = ErrorResponse, description = "Invalid file name"),
        (status = 404, body = ErrorResponse, description = "File not found")
    )
)]
pub async fn handle_download_log_file(
    Path(name): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let cfg = Settings::logger().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let file_path = cfg.file.clone().unwrap_or_default();
    let dir = StdPath::new(&file_path)
        .parent()
        .unwrap_or(StdPath::new("."))
        .to_path_buf();
    let target = dir.join(&name);
    // Prevent path traversal
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return Err(StatusCode::BAD_REQUEST);
    }
    let bytes = std::fs::read(&target).map_err(|_| StatusCode::NOT_FOUND)?;
    let disposition = format!("attachment; filename=\"{}\"", name);
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        bytes,
    ))
}

/// DELETE /api/logs/files/{name}
///
/// Removes one log file; the active file is refused.
#[utoipa::path(
    delete, path = "/api/logs/files/{name}", tag = "Logs",
    params(("name" = String, Path, description = "Log file name")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 400, body = ErrorResponse, description = "Invalid file name"),
        (status = 403, body = ErrorResponse, description = "Refused for the active file"),
        (status = 404, body = ErrorResponse, description = "File not found")
    )
)]
pub async fn handle_delete_log_file(Path(name): Path<String>) -> StatusCode {
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return StatusCode::BAD_REQUEST;
    }
    let cfg = match Settings::logger() {
        Ok(c) => c,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };
    let file_path = cfg.file.clone().unwrap_or_default();
    let dir = StdPath::new(&file_path)
        .parent()
        .unwrap_or(StdPath::new("."))
        .to_path_buf();
    let target = dir.join(&name);
    // Do not allow deleting the active file
    let active_name = StdPath::new(&file_path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if name == active_name {
        return StatusCode::FORBIDDEN;
    }
    match std::fs::remove_file(&target) {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::NOT_FOUND,
    }
}

/// POST /api/logs/rotate
///
/// Applies the retention policy now instead of waiting for the next write.
#[utoipa::path(
    post, path = "/api/logs/rotate", tag = "Logs",
    responses(
        (status = 204, description = "Retention applied"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_rotate_logs() -> StatusCode {
    if let Ok(cfg) = Settings::logger() {
        cce_logging::cleanup_logs(&cfg);
    }
    StatusCode::NO_CONTENT
}

/// GET /api/logs/config
///
/// Effective logger configuration of the running process.
#[utoipa::path(
    get, path = "/api/logs/config", tag = "Logs",
    responses(
        (status = 200, body = LogConfig, description = "Success"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_get_log_config() -> Json<LogConfig> {
    let cfg = Settings::logger().unwrap_or_default();
    Json(LogConfig {
        level: cfg.level.to_string(),
        output: cfg.output.to_string(),
        format: cfg.format.to_string(),
        file: cfg.file,
        filter: cfg.filter,
        max_file_size_mb: cfg.max_file_size_mb,
        max_files: cfg.max_files,
        max_total_size_mb: cfg.max_total_size_mb,
        enable_http_api: cfg.enable_http_api,
        ws_capacity: cfg.ws_capacity,
        memory_buffer_size: cfg.memory_buffer_size,
    })
}

/// PUT /api/logs/config
///
/// Changes the level, format or filter of the running logger.
#[utoipa::path(
    put, path = "/api/logs/config", tag = "Logs",
    request_body = UpdateLogConfigRequest,
    responses(
        (status = 204, description = "Applied"),
        (status = 500, body = ErrorResponse, description = "Internal error")
    )
)]
pub async fn handle_update_log_config(Json(req): Json<UpdateLogConfigRequest>) -> StatusCode {
    let mut cfg = match Settings::logger() {
        Ok(c) => c,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };
    if let Some(level) = &req.level {
        match level.to_lowercase().as_str() {
            "trace" => cfg.level = cce_config::global::LogLevel::Trace,
            "debug" => cfg.level = cce_config::global::LogLevel::Debug,
            "info" => cfg.level = cce_config::global::LogLevel::Info,
            "warn" => cfg.level = cce_config::global::LogLevel::Warn,
            "error" => cfg.level = cce_config::global::LogLevel::Error,
            _ => {}
        }
    }
    if let Some(format) = &req.format {
        match format.to_lowercase().as_str() {
            "pretty" => cfg.format = cce_config::global::LogFormat::Pretty,
            "compact" => cfg.format = cce_config::global::LogFormat::Compact,
            "json" => cfg.format = cce_config::global::LogFormat::Json,
            _ => {}
        }
    }
    if let Some(filter) = &req.filter {
        cfg.filter = Some(filter.clone());
        let _ = cce_logging::reload_level(filter);
    } else if req.level.is_some() {
        let _ = cce_logging::reload_level(&cfg.level.to_string());
    }
    let _ = Settings::set_logger(cfg);
    StatusCode::NO_CONTENT
}

// WebSocket handler
pub async fn handle_ws_logs(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(|mut socket| async move {
        use axum::extract::ws::Message;
        let mut rx = match subscribe() {
            Some(r) => r,
            None => return,
        };
        // Send recent history first
        for ev in recent(None, None, None) {
            if let Ok(text) = serde_json::to_string(&ev)
                && socket.send(Message::Text(text.into())).await.is_err()
            {
                return;
            }
        }
        while let Ok(ev) = rx.recv().await {
            if let Ok(text) = serde_json::to_string(&ev)
                && socket.send(Message::Text(text.into())).await.is_err()
            {
                break;
            }
        }
    })
}
