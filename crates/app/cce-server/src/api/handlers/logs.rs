//! Log management handlers (viewing, searching, file management, config).

use axum::{
    Json,
    extract::{Path, Query, WebSocketUpgrade},
    http::{StatusCode, header},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use std::path::Path as StdPath;

use cce_config::Settings;
use cce_logging::{LogEvent, recent, subscribe};

#[derive(Deserialize)]
pub struct LogsQuery {
    pub level: Option<String>,
    pub module: Option<String>,
    pub query: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

#[derive(Serialize)]
pub struct LogsResponse {
    pub entries: Vec<LogEvent>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
}

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
    let page_entries: Vec<LogEvent> = entries
        .into_iter()
        .skip(start_idx)
        .take(page_size)
        .collect();

    Json(LogsResponse {
        entries: page_entries,
        total,
        page,
        page_size,
    })
}

#[derive(Serialize)]
pub struct LogFileInfo {
    pub name: String,
    pub size: u64,
    pub modified: String,
}

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

pub async fn handle_rotate_logs() -> StatusCode {
    if let Ok(cfg) = Settings::logger() {
        cce_logging::cleanup_logs(&cfg);
    }
    StatusCode::NO_CONTENT
}

#[derive(Serialize)]
pub struct LogConfigResponse {
    pub level: String,
    pub output: String,
    pub format: String,
    pub file: Option<String>,
    pub filter: Option<String>,
    pub max_file_size_mb: Option<u64>,
    pub max_files: Option<usize>,
    pub max_total_size_mb: Option<u64>,
    pub enable_http_api: bool,
    pub ws_capacity: usize,
    pub memory_buffer_size: usize,
}

pub async fn handle_get_log_config() -> Json<LogConfigResponse> {
    let cfg = Settings::logger().unwrap_or_default();
    Json(LogConfigResponse {
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

#[derive(Deserialize)]
pub struct UpdateLogConfigRequest {
    pub level: Option<String>,
    pub format: Option<String>,
    pub filter: Option<String>,
}

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
            if let Ok(text) = serde_json::to_string(&ev) {
                if socket.send(Message::Text(text.into())).await.is_err() {
                    return;
                }
            }
        }
        while let Ok(ev) = rx.recv().await {
            if let Ok(text) = serde_json::to_string(&ev) {
                if socket.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
        }
    })
}
