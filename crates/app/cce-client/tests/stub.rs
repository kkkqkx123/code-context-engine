//! Loopback HTTP stub recording what the client actually sent.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// One recorded request.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub target: String,
    pub authorization: Option<String>,
    pub user_agent: Option<String>,
    pub body: String,
}

/// One canned response.
#[derive(Debug, Clone)]
pub struct Reply {
    status: u16,
    content_type: &'static str,
    body: String,
}

impl Reply {
    pub fn json(status: u16, body: serde_json::Value) -> Self {
        Self {
            status,
            content_type: "application/json",
            body: body.to_string(),
        }
    }

    pub fn text(status: u16, body: &str) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8",
            body: body.to_string(),
        }
    }
}

/// Loopback server answering the given replies in order.
pub struct Stub {
    url: String,
    recorded: Arc<Mutex<Vec<Recorded>>>,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Stub {
    pub fn spawn(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("stub binds loopback");
        let url = format!("http://{}", listener.local_addr().expect("stub addr"));
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let replies = Arc::new(Mutex::new(replies));
        listener.set_nonblocking(true).expect("stub nonblocking");

        let thread_recorded = Arc::clone(&recorded);
        let thread_shutdown = Arc::clone(&shutdown);
        let handle = std::thread::spawn(move || {
            while !thread_shutdown.load(Ordering::Relaxed) {
                let (stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(_) => break,
                };
                stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
                let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).is_err() {
                    continue;
                }
                let mut parts = request_line.split_whitespace();
                let method = parts.next().unwrap_or_default().to_string();
                let target = parts.next().unwrap_or_default().to_string();
                let mut content_length = 0usize;
                let mut authorization = None;
                let mut user_agent = None;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() {
                        break;
                    }
                    let line = line.trim_end();
                    if line.is_empty() {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':') {
                        if name.trim().eq_ignore_ascii_case("content-length") {
                            content_length = value.trim().parse().unwrap_or(0);
                        }
                        if name.trim().eq_ignore_ascii_case("authorization") {
                            authorization = Some(value.trim().to_string());
                        }
                        if name.trim().eq_ignore_ascii_case("user-agent") {
                            user_agent = Some(value.trim().to_string());
                        }
                    }
                }
                let mut body = vec![0u8; content_length];
                if content_length > 0 {
                    let _ = reader.read_exact(&mut body);
                }
                thread_recorded.lock().expect("recorded").push(Recorded {
                    method,
                    target,
                    authorization,
                    user_agent,
                    body: String::from_utf8_lossy(&body).into_owned(),
                });

                let reply = replies.lock().expect("replies").pop();
                let Some(reply) = reply else { continue };
                let mut stream = reader.into_inner();
                let _ = write!(
                    stream,
                    "HTTP/1.1 {} CCE\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    reply.status,
                    reply.content_type,
                    reply.body.len(),
                    reply.body
                );
                let _ = stream.flush();
            }
        });

        Self {
            url,
            recorded,
            shutdown,
            handle: Some(handle),
        }
    }

    pub fn url(&self) -> String {
        self.url.clone()
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.recorded.lock().expect("recorded").clone()
    }
}

impl Drop for Stub {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
