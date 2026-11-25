//! MCP (Model Context Protocol) configuration module
//!
//! Defines the user-facing configuration for the optional MCP server
//! (`cce-mcp`). The server is disabled by default so that enabling it is an
//! explicit opt-in that does not widen the default attack surface.

use serde::{Deserialize, Serialize};

use crate::validation::{Validate, ValidationResult};
use cce_types::error::config::ConfigValidationError;

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

/// Capability profile that decides which tools the MCP surface exposes.
///
/// The default is `ReadOnly`: MCP stays read-only and budget-capped unless
/// `full` is explicitly requested for tool-assisted generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpProfile {
    /// Expose all registered tools, subject only to `enabled_tools`.
    Full,
    /// Expose only read-capable tools and force the call budget on.
    #[default]
    ReadOnly,
}

impl McpProfile {
    /// Whether this profile forbids every write-capable tool.
    pub fn is_read_only(self) -> bool {
        matches!(self, Self::ReadOnly)
    }
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

/// Server-enforced limits for the read-only tool-call budget.
///
/// Budget only ever applies to read-capable tools; write tools are excluded by
/// the capability profile. All values have conservative defaults and are
/// validated to be non-zero where a zero would be meaningless.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct McpBudgetConfig {
    /// Master switch. Forced on when `profile = "read_only"`.
    pub enabled: bool,
    /// Maximum number of billable tool calls within one run.
    pub max_calls_per_run: usize,
    /// Maximum number of billable tool calls within one page of a run.
    pub max_calls_per_page: usize,
    /// Maximum number of returned items from a single read query.
    pub max_return_items: usize,
    /// Maximum number of lines a single file read may return.
    pub max_read_file_lines: usize,
    /// Upper bound on concurrent runs tracked in memory.
    pub max_active_runs: usize,
    /// Whether repeated calls within a run reuse a cached result.
    pub cache_enabled: bool,
    /// Seconds of inactivity after which a run's cache entry expires.
    pub cache_ttl_secs: u64,
}

impl Default for McpBudgetConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_calls_per_run: 200,
            max_calls_per_page: 20,
            max_return_items: 500,
            max_read_file_lines: 2000,
            max_active_runs: 256,
            cache_enabled: true,
            cache_ttl_secs: 900,
        }
    }
}

impl Validate for McpBudgetConfig {
    fn validate_structured(&self) -> ValidationResult {
        let positive = [
            ("max_calls_per_run", self.max_calls_per_run),
            ("max_calls_per_page", self.max_calls_per_page),
            ("max_return_items", self.max_return_items),
            ("max_read_file_lines", self.max_read_file_lines),
            ("max_active_runs", self.max_active_runs),
        ];
        for (name, value) in positive {
            if value == 0 {
                return Err(ConfigValidationError::invalid_field(
                    format!("mcp.budget.{name}"),
                    "must be greater than 0",
                ));
            }
        }
        if self.cache_ttl_secs == 0 {
            return Err(ConfigValidationError::invalid_field(
                "mcp.budget.cache_ttl_secs",
                "must be greater than 0",
            ));
        }
        Ok(())
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
    /// exposed (subject to the capability profile); when set, only the listed
    /// tools are eligible. This narrows, but can never widen, the profile.
    pub enabled_tools: Option<Vec<String>>,
    /// Capability profile applied to the tool surface.
    pub profile: McpProfile,
    /// Server-enforced read-only call budget.
    pub budget: McpBudgetConfig,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            transport: McpTransport::Stdio,
            http: McpHttpConfig::default(),
            enabled_tools: None,
            profile: McpProfile::ReadOnly,
            budget: McpBudgetConfig::default(),
        }
    }
}

impl McpConfig {
    /// Effective budget switch: the read-only profile forces the budget on.
    pub fn budget_enabled(&self) -> bool {
        self.budget.enabled || self.profile.is_read_only()
    }
}

impl Validate for McpConfig {
    fn validate_structured(&self) -> ValidationResult {
        self.budget.validate_structured()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_defaults_to_read_only() {
        assert_eq!(McpProfile::default(), McpProfile::ReadOnly);
        assert!(McpProfile::ReadOnly.is_read_only());
        assert!(!McpProfile::Full.is_read_only());
    }

    #[test]
    fn profile_parses_from_snake_case() {
        let full: McpProfile = toml::from_str("profile = \"full\"")
            .map(|v: ProfileProbe| v.profile)
            .expect("full profile parses");
        let read_only: McpProfile = toml::from_str("profile = \"read_only\"")
            .map(|v: ProfileProbe| v.profile)
            .expect("read_only profile parses");
        assert_eq!(full, McpProfile::Full);
        assert_eq!(read_only, McpProfile::ReadOnly);
    }

    #[derive(Deserialize)]
    struct ProfileProbe {
        profile: McpProfile,
    }

    #[test]
    fn budget_defaults_are_consistent() {
        let budget = McpBudgetConfig::default();
        assert!(budget.enabled);
        assert!(budget.max_calls_per_run > budget.max_calls_per_page);
        assert_eq!(budget.max_read_file_lines, 2000);
        assert!(budget.validate_structured().is_ok());
    }

    #[test]
    fn read_only_profile_forces_budget_on() {
        let mut config = McpConfig {
            profile: McpProfile::Full,
            ..McpConfig::default()
        };
        config.budget.enabled = false;
        assert!(!config.budget_enabled());
        config.profile = McpProfile::ReadOnly;
        assert!(config.budget_enabled());
    }

    #[test]
    fn budget_rejects_zero_limits() {
        let budget = McpBudgetConfig {
            max_calls_per_page: 0,
            ..McpBudgetConfig::default()
        };
        assert!(budget.validate_structured().is_err());
    }
}
