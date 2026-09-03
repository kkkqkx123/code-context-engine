//! Operation event bus for event-driven architecture
//!
//! Provides pub-sub infrastructure for operation lifecycle events.
//! - Decouples event publishers from listeners
//! - Supports multiple concurrent listeners
//! - Non-blocking async event dispatch

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;

use super::events::OperationEvent;

/// Event type classification (for listener filtering)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    Started,
    ProgressUpdated,
    FileCompleted,
    FileFailed,
    BatchCompleted,
    Completed,
    Failed,
    Paused,
    Resumed,
}

impl EventType {
    pub fn from_event(event: &OperationEvent) -> Self {
        match event {
            OperationEvent::Started { .. } => EventType::Started,
            OperationEvent::ProgressUpdated { .. } => EventType::ProgressUpdated,
            OperationEvent::FileCompleted { .. } => EventType::FileCompleted,
            OperationEvent::FileFailed { .. } => EventType::FileFailed,
            OperationEvent::BatchCompleted { .. } => EventType::BatchCompleted,
            OperationEvent::Completed { .. } => EventType::Completed,
            OperationEvent::Failed { .. } => EventType::Failed,
            OperationEvent::Paused { .. } => EventType::Paused,
            OperationEvent::Resumed { .. } => EventType::Resumed,
        }
    }
}

/// Event listener trait for subscribers
#[async_trait]
pub trait EventListener: Send + Sync {
    /// Handle the event
    async fn on_event(&self, event: &OperationEvent) -> Result<()>;

    /// Get listener name for logging
    fn name(&self) -> &str;

    /// Event type filter (None = listen to all)
    fn filter(&self) -> Option<Vec<EventType>> {
        None
    }

    /// Check if this listener should handle the event
    fn should_handle(&self, event_type: EventType) -> bool {
        if let Some(ref filter) = self.filter() {
            filter.contains(&event_type)
        } else {
            true
        }
    }

    /// Listener execution priority (lower = earlier execution)
    /// Default: 50 (medium priority)
    fn priority(&self) -> u32 {
        50
    }

    /// Optional timeout for this listener (milliseconds)
    /// If None, uses event bus default timeout
    fn timeout_ms(&self) -> Option<u64> {
        None
    }
}

/// Event bus statistics
#[derive(Debug, Clone, Default)]
pub struct EventBusStats {
    pub total_events: u64,
    pub total_listeners: u64,
    pub queue_size: usize,
    pub dispatch_errors: u64,
}

/// Operation event bus - pub/sub pattern
pub struct OperationEventBus {
    /// Registered listeners
    listeners: Arc<RwLock<Vec<Arc<dyn EventListener>>>>,

    /// Event queue for async dispatch
    event_queue: Arc<Mutex<VecDeque<OperationEvent>>>,

    /// Background dispatch task
    dispatch_handle: Arc<Mutex<Option<JoinHandle<()>>>>,

    /// Statistics
    stats: Arc<Mutex<EventBusStats>>,

    /// Max queue size
    max_queue_size: usize,

    /// Timeout for individual listener
    listener_timeout: Duration,

    /// Dispatcher running flag
    running: Arc<AtomicBool>,
}

impl OperationEventBus {
    /// Create new event bus with custom queue size
    pub fn new(max_queue_size: usize) -> Self {
        Self {
            listeners: Arc::new(RwLock::new(Vec::new())),
            event_queue: Arc::new(Mutex::new(VecDeque::new())),
            dispatch_handle: Arc::new(Mutex::new(None)),
            stats: Arc::new(Mutex::new(EventBusStats::default())),
            max_queue_size,
            listener_timeout: Duration::from_secs(30),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(10000)
    }

    /// Register an event listener
    pub async fn subscribe(&self, listener: Arc<dyn EventListener>) -> Result<()> {
        let mut listeners = self.listeners.write().await;
        let listener_name = listener.name().to_string();
        listeners.push(listener);

        let mut stats = self.stats.lock().await;
        stats.total_listeners += 1;

        tracing::info!(
            listener = %listener_name,
            total = stats.total_listeners,
            "Event listener registered"
        );

        Ok(())
    }

    /// Publish an event
    pub async fn publish(&self, event: OperationEvent) -> Result<()> {
        let mut queue = self.event_queue.lock().await;

        if queue.len() >= self.max_queue_size {
            return Err(anyhow!(
                "Event queue full: {} >= {}",
                queue.len(),
                self.max_queue_size
            ));
        }

        // Warn if queue is getting full (80% capacity)
        if queue.len() as f32 / self.max_queue_size as f32 > 0.8 {
            tracing::warn!(
                queue_size = queue.len(),
                max_size = self.max_queue_size,
                "Event queue approaching capacity"
            );
        }

        queue.push_back(event);

        let mut stats = self.stats.lock().await;
        stats.total_events += 1;
        stats.queue_size = queue.len();

        Ok(())
    }

    /// Publish event with blocking behavior (waits for queue space)
    ///
    /// Unlike `publish()`, this method will wait for queue space to become
    /// available rather than returning an error immediately.
    ///
    /// # Arguments
    ///
    /// * `event` - Event to publish
    /// * `max_wait` - Maximum time to wait for queue space
    ///
    /// # Returns
    ///
    /// Ok if event was enqueued, Err if timeout or other error
    pub async fn publish_blocking(&self, event: OperationEvent, max_wait: Duration) -> Result<()> {
        let deadline = tokio::time::Instant::now() + max_wait;

        loop {
            let mut queue = self.event_queue.lock().await;

            // Check if there's space
            if queue.len() < self.max_queue_size {
                // Warn if queue is getting full (80% capacity)
                if queue.len() as f32 / self.max_queue_size as f32 > 0.8 {
                    tracing::warn!(
                        queue_size = queue.len(),
                        max_size = self.max_queue_size,
                        "Event queue approaching capacity"
                    );
                }

                queue.push_back(event);

                let mut stats = self.stats.lock().await;
                stats.total_events += 1;
                stats.queue_size = queue.len();

                return Ok(());
            }

            // Check timeout
            if tokio::time::Instant::now() >= deadline {
                return Err(anyhow!(
                    "Timeout waiting for queue space: {} >= {}",
                    queue.len(),
                    self.max_queue_size
                ));
            }

            // Release lock and wait a bit
            drop(queue);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Start background dispatcher task
    pub async fn start_dispatcher(&self) -> Result<()> {
        // Check if already running
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(anyhow!("Dispatcher already running"));
        }

        let listeners = self.listeners.clone();
        let queue = self.event_queue.clone();
        let stats = self.stats.clone();
        let default_timeout = self.listener_timeout;
        let running = self.running.clone();

        let handle = tokio::spawn(async move {
            tracing::info!("Event dispatcher started");

            loop {
                // Check if should continue
                if !running.load(Ordering::Relaxed) {
                    break;
                }

                tokio::time::sleep(Duration::from_millis(10)).await;

                // Dequeue an event
                let event = {
                    let mut q = queue.lock().await;
                    q.pop_front()
                };

                match event {
                    Some(evt) => {
                        let listeners_guard = listeners.read().await;
                        let event_type = EventType::from_event(&evt);

                        // Collect matching listeners with their metadata, then sort
                        let mut listener_info: Vec<_> = listeners_guard
                            .iter()
                            .filter(|listener| listener.should_handle(event_type))
                            .map(|listener| {
                                (
                                    listener.clone(),
                                    listener.priority(),
                                    listener
                                        .timeout_ms()
                                        .map(Duration::from_millis)
                                        .unwrap_or(default_timeout),
                                    listener.name().to_string(),
                                )
                            })
                            .collect();

                        // Sort by priority (lower value = higher priority)
                        listener_info.sort_by_key(|(_, priority, _, _)| *priority);

                        // Release the read guard before spawning tasks
                        drop(listeners_guard);

                        // Dispatch to all matching listeners concurrently
                        let handles: Vec<_> = listener_info
                            .into_iter()
                            .map(|(listener, _, timeout, listener_name)| {
                                let evt_clone = evt.clone();

                                tokio::spawn(async move {
                                    match tokio::time::timeout(
                                        timeout,
                                        listener.on_event(&evt_clone),
                                    )
                                    .await
                                    {
                                        Ok(Ok(())) => true,
                                        Ok(Err(e)) => {
                                            tracing::error!(
                                                listener = %listener_name,
                                                error = %e,
                                                "Event listener failed"
                                            );
                                            false
                                        }
                                        Err(_) => {
                                            tracing::error!(
                                                listener = %listener_name,
                                                ?event_type,
                                                timeout_ms = timeout.as_millis(),
                                                "Event listener timeout"
                                            );
                                            false
                                        }
                                    }
                                })
                            })
                            .collect();

                        // Wait for all listeners to complete
                        if !handles.is_empty() {
                            let results = futures::future::join_all(handles).await;
                            let failed = results
                                .iter()
                                .filter(|r| match r {
                                    Ok(success) => !success,
                                    Err(_) => true,
                                })
                                .count();

                            if failed > 0 {
                                let mut s = stats.lock().await;
                                s.dispatch_errors += failed as u64;

                                tracing::warn!(
                                    failed = failed,
                                    total = results.len(),
                                    "Some event listeners failed"
                                );
                            }
                        }
                    }
                    None => {
                        // Queue empty, wait
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }

            tracing::info!("Event dispatcher stopped");
        });

        let mut dispatch = self.dispatch_handle.lock().await;
        *dispatch = Some(handle);

        tracing::info!("Event dispatcher task spawned");
        Ok(())
    }

    /// Stop the dispatcher
    pub async fn stop_dispatcher(&self) -> Result<()> {
        self.running.store(false, Ordering::SeqCst);

        let mut handle = self.dispatch_handle.lock().await;
        if let Some(h) = handle.take() {
            let _ = tokio::time::timeout(Duration::from_secs(5), h).await;
        }

        tracing::info!("Event dispatcher stopped");
        Ok(())
    }

    /// Get statistics
    pub async fn stats(&self) -> EventBusStats {
        self.stats.lock().await.clone()
    }

    /// Get listener count
    pub async fn listener_count(&self) -> usize {
        self.listeners.read().await.len()
    }

    /// Get queue size
    pub async fn queue_size(&self) -> usize {
        self.event_queue.lock().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestListener {
        events_received: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl EventListener for TestListener {
        async fn on_event(&self, event: &OperationEvent) -> Result<()> {
            let mut events = self.events_received.lock().await;
            events.push(event.operation_id().to_string());
            Ok(())
        }

        fn name(&self) -> &str {
            "TestListener"
        }
    }

    #[tokio::test]
    async fn test_event_bus_creation() {
        let bus = OperationEventBus::with_defaults();
        assert_eq!(bus.listener_count().await, 0);
        assert_eq!(bus.queue_size().await, 0);
    }

    #[tokio::test]
    async fn test_event_bus_publish() {
        let bus = OperationEventBus::with_defaults();

        let event = OperationEvent::Started {
            project_id: 1,
            operation_id: "op1".to_string(),
            operation_type: "HotUpdate".to_string(),
            total_files: 100,
            timestamp_ms: 0,
        };

        assert!(bus.publish(event).await.is_ok());
        assert_eq!(bus.queue_size().await, 1);
    }

    #[tokio::test]
    async fn test_event_bus_subscribe() {
        let bus = OperationEventBus::with_defaults();
        let listener = Arc::new(TestListener {
            events_received: Arc::new(Mutex::new(Vec::new())),
        });

        assert!(bus.subscribe(listener).await.is_ok());
        assert_eq!(bus.listener_count().await, 1);
    }
}
