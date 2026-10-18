//! Log capture: ring buffer + broadcast channel + tracing layer.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::Subscriber;
use tracing_subscriber::Layer;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
    pub fields: serde_json::Value,
}

pub struct CaptureState {
    buffer: Arc<Mutex<VecDeque<LogEvent>>>,
    capacity: usize,
    sender: broadcast::Sender<LogEvent>,
}

static STATE: std::sync::OnceLock<Arc<CaptureState>> = std::sync::OnceLock::new();

pub fn init(config: &cce_config::global::LoggingConfig) {
    let capacity = config.memory_buffer_size.max(1);
    let (tx, _) = broadcast::channel(config.ws_capacity.max(1));
    let state = Arc::new(CaptureState {
        buffer: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
        capacity,
        sender: tx,
    });
    let _ = STATE.set(state);
}

pub fn state() -> Option<Arc<CaptureState>> {
    STATE.get().cloned()
}

pub fn push(event: LogEvent) {
    if let Some(st) = STATE.get() {
        if let Ok(mut buf) = st.buffer.lock() {
            if buf.len() >= st.capacity {
                buf.pop_front();
            }
            buf.push_back(event.clone());
        }
        let _ = st.sender.send(event);
    }
}

pub fn recent(level: Option<&str>, module: Option<&str>, query: Option<&str>) -> Vec<LogEvent> {
    let Some(st) = STATE.get() else {
        return Vec::new();
    };
    let Ok(buf) = st.buffer.lock() else {
        return Vec::new();
    };
    buf.iter()
        .filter(|e| {
            if let Some(l) = level {
                if e.level.to_lowercase() < l.to_lowercase() {
                    return false;
                }
            }
            if let Some(m) = module {
                if !e.target.contains(m) {
                    return false;
                }
            }
            if let Some(q) = query {
                if !e.message.contains(q) && !e.target.contains(q) {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect()
}

pub fn subscribe() -> Option<broadcast::Receiver<LogEvent>> {
    STATE.get().map(|st| st.sender.subscribe())
}

pub struct CaptureLayer;

impl<S: Subscriber> Layer<S> for CaptureLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        use tracing::field::Visit;
        struct V {
            message: String,
            fields: serde_json::Map<String, serde_json::Value>,
        }
        impl Visit for V {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.message = format!("{:?}", value);
                } else {
                    self.fields.insert(
                        field.name().to_string(),
                        serde_json::json!(format!("{:?}", value)),
                    );
                }
            }
            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                if field.name() == "message" {
                    self.message = value.to_string();
                } else {
                    self.fields
                        .insert(field.name().to_string(), serde_json::json!(value));
                }
            }
        }
        let meta = event.metadata();
        let mut v = V {
            message: String::new(),
            fields: serde_json::Map::new(),
        };
        event.record(&mut v);
        let ev = LogEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            level: meta.level().to_string(),
            target: meta.target().to_string(),
            message: v.message,
            fields: serde_json::Value::Object(v.fields),
        };
        push(ev);
    }
}
