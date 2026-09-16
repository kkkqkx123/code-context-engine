//! MCP (Model Context Protocol) configuration module
//!
//! Defines the user-facing configuration for the optional MCP server
//! (`cce-mcp`). The server is disabled by default so that enabling it is an
//! explicit opt-in that does not widen the default attack surface.

use serde::{Deserialize, Serialize};

/// Transport used by the MCP server.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    /// Serve MCP over stdio (local clients, IDE plugins, MCP Inspector).
    #[default]
    Stdio,
    /// Serve MCP over the Streamable HTTP transport on an independent port.
    StreamableHttp,
}

/// HTTP transport settings for the MCP server.
///
/// Only used when `transport = "streamable_http"`. The listener defaults to
/// loopback so the endpoint is not exposed beyond the local host unless the
/// operator explicitly changes `host`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct McpHttpConfig {
    /// Host to bind the MCP HTTP listener to.
    pub host: String,
    /// Port to bind the MCP HTTP listener to (independent of the HTTP server).
    pub port: u16,
    /// Allowed `Host` authorities for inbound validation (DNS-rebinding
    /// protection). Empty means "use the transport default" (loopback only).
    pub allowed_hosts: Vec<String>,
}

impl Default for McpHttpConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 9100,
            allowed_hosts: Vec::new(),
        }
    }
}

/// MCP server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct McpConfig {
    /// Whether the MCP server is enabled.
    pub enabled: bool,
    /// Transport to serve MCP over.
    pub transport: McpTransport,
    /// HTTP transport settings (only meaningful for `streamable_http`).
    pub http: McpHttpConfig,
    /// Optional allow-list of tool names to expose. When `None` every tool is
    /// exposed; when set, only the listed tools are registered. This is the
    /// hook for disabling write-capable tools without a code change.
    pub enabled_tools: Option<Vec<String>>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            transport: McpTransport::Stdio,
            http: McpHttpConfig::default(),
            enabled_tools: None,
        }
    }
}
