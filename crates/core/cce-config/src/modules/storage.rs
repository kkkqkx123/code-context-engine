//! Storage configuration
//!
//! Configuration for Qdrant vector storage and BM25 index.

use serde::{Deserialize, Serialize};

use crate::validation::{Validate, ValidationResult};
use cce_types::error::config::ConfigValidationError;

/// Distance metric for vector comparison
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DistanceMetric {
    /// Cosine similarity (normalized dot product)
    #[default]
    Cosine,
    /// Euclidean distance
    Euclid,
    /// Dot product
    Dot,
}

impl DistanceMetric {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Cosine => "Cosine",
            Self::Euclid => "Euclid",
            Self::Dot => "Dot",
        }
    }
}

/// Vector backend selection (local-first, remote optional).
///
/// Local is the default: single-process embedded engine with no external
/// service. Qdrant keeps the existing remote behaviour for large-scale
/// deployments. Switching backends requires a full reindex; old data is
/// never migrated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum VectorBackend {
    /// Embedded local engine (simvec, mmap plus WAL plus HNSW).
    #[default]
    Local,
    /// External Qdrant service (keeps process management and breaker).
    Qdrant,
}

impl VectorBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Qdrant => "qdrant",
        }
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }
}

/// Relation storage backend selection.
///
/// `Local` is the default embedded SQLite branch; `Remote` selects the
/// PostgreSQL branch. Switching backends requires a full reindex; old data
/// is never migrated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RelationBackend {
    /// Embedded SQLite repositories (current behavior).
    #[default]
    Local,
    /// PostgreSQL branch (forward scaffolding: configurable but not wired
    /// into indexing, search, or hot-update paths yet).
    Remote,
}

impl RelationBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
        }
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }
}

/// Fulltext storage backend selection.
///
/// `Local` is the default embedded Tantivy branch; `Remote` selects the
/// Elasticsearch branch. Switching backends requires a full reindex; old
/// data is never migrated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FulltextBackend {
    /// Embedded Tantivy index (current behavior).
    #[default]
    Local,
    /// Elasticsearch branch (forward scaffolding: configurable but not
    /// wired into indexing, search, or hot-update paths yet).
    Remote,
}

impl FulltextBackend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Remote => "remote",
        }
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }
}

/// Remote parameters for the relation branch (PostgreSQL).
///
/// Local parameters and remote parameters are isolated: SQLite tuning stays
/// in [`SqliteConfig`], pool and timeout tuning lives here. Secrets stay
/// overridable via environment variables.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RelationRemoteConfig {
    /// PostgreSQL connection URL (e.g. `postgres://host:5432/cce`).
    /// Individual credential fields below take precedence over the
    /// credentials embedded in the URL.
    pub url: Option<String>,
    /// Database user name.
    pub username: Option<String>,
    /// Database password (prefer environment override).
    pub password: Option<String>,
    /// Connection pool size.
    pub pool_size: u32,
    /// Timeout for establishing a new connection, in milliseconds.
    pub connect_timeout_ms: u64,
    /// Timeout for acquiring a pooled connection, in milliseconds.
    pub acquire_timeout_ms: u64,
    /// Per-statement timeout, in milliseconds (0 disables).
    pub statement_timeout_ms: u64,
}

impl Default for RelationRemoteConfig {
    fn default() -> Self {
        Self {
            url: None,
            username: None,
            password: None,
            pool_size: 8,
            connect_timeout_ms: 5_000,
            acquire_timeout_ms: 5_000,
            statement_timeout_ms: 30_000,
        }
    }
}

impl Validate for RelationRemoteConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();
        if self.pool_size == 0 || self.pool_size > 128 {
            errors.push(ConfigValidationError::out_of_range(
                "pool_size",
                self.pool_size.to_string(),
                "1",
                "128",
            ));
        }
        if self.connect_timeout_ms == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "connect_timeout_ms",
                "must be greater than 0",
            ));
        }
        if self.acquire_timeout_ms == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "acquire_timeout_ms",
                "must be greater than 0",
            ));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl RelationRemoteConfig {
    /// Effective database user: the explicit username, empty means no
    /// authentication.
    pub fn effective_username(&self) -> Option<&str> {
        self.username.as_deref().filter(|s| !s.is_empty())
    }
}

/// Remote parameters for the fulltext branch (Elasticsearch).
///
/// Local parameters (writer budget, reader policy) stay in [`Bm25Config`];
/// endpoint, authentication, refresh and batching tuning lives here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FulltextRemoteConfig {
    /// Search-service base URL (e.g. `http://localhost:9200`).
    pub url: Option<String>,
    /// API key (reserved, prefer environment override).
    pub api_key: Option<String>,
    /// Basic-auth user name.
    pub username: Option<String>,
    /// Basic-auth password (prefer environment override).
    pub password: Option<String>,
    /// Index name; required, no fallback to the local index name.
    pub index_name: Option<String>,
    /// Bulk batch size for write requests.
    pub bulk_size: usize,
    /// Per-request timeout in milliseconds.
    pub request_timeout_ms: u64,
    /// Desired index refresh interval (e.g. `1s`, `-1` to disable).
    pub refresh_interval: Option<String>,
}

impl Default for FulltextRemoteConfig {
    fn default() -> Self {
        Self {
            url: None,
            api_key: None,
            username: None,
            password: None,
            index_name: None,
            bulk_size: 500,
            request_timeout_ms: 30_000,
            refresh_interval: None,
        }
    }
}

impl Validate for FulltextRemoteConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();
        if self.bulk_size == 0 || self.bulk_size > 10_000 {
            errors.push(ConfigValidationError::out_of_range(
                "bulk_size",
                self.bulk_size.to_string(),
                "1",
                "10000",
            ));
        }
        if self.request_timeout_ms == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "request_timeout_ms",
                "must be greater than 0",
            ));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl FulltextRemoteConfig {
    /// Explicit index name, if configured (blank counts as unset).
    pub fn explicit_index_name(&self) -> Option<&str> {
        self.index_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }
}

/// Local vector engine configuration (embedded simvec).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalVectorConfig {
    /// Data directory for mmap files plus WAL. None resolves to
    /// `<sqlite_parent>/vectors` at engine build time.
    pub data_dir: Option<String>,
    /// Vector dimension, must match the embedder model output.
    pub vector_size: usize,
    /// Distance metric, same semantics as the Qdrant branch.
    pub distance_metric: DistanceMetric,
    /// HNSW graph degree (m). None uses the engine default.
    pub hnsw_m: Option<u32>,
    /// HNSW build candidate list size. None uses the engine default.
    pub hnsw_ef_construct: Option<u32>,
    /// HNSW query-time candidate list size. None uses the engine default.
    pub hnsw_ef_search: Option<usize>,
    /// Live-point threshold above which exact scan promotes to HNSW.
    /// None uses the engine default (10000, Qdrant parity).
    pub full_scan_threshold: Option<usize>,
}

impl Default for LocalVectorConfig {
    fn default() -> Self {
        Self {
            data_dir: None,
            vector_size: 1024,
            distance_metric: DistanceMetric::Cosine,
            hnsw_m: None,
            hnsw_ef_construct: None,
            hnsw_ef_search: None,
            full_scan_threshold: None,
        }
    }
}

impl Validate for LocalVectorConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();
        if self.vector_size == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "vector_size",
                "must be greater than 0",
            ));
        }
        if let Some(m) = self.hnsw_m
            && !(2..=128).contains(&m)
        {
            errors.push(ConfigValidationError::out_of_range(
                "hnsw_m",
                m.to_string(),
                "2",
                "128",
            ));
        }
        if let Some(ef) = self.hnsw_ef_construct
            && !(10..=1000).contains(&ef)
        {
            errors.push(ConfigValidationError::out_of_range(
                "hnsw_ef_construct",
                ef.to_string(),
                "10",
                "1000",
            ));
        }
        if let Some(ef) = self.hnsw_ef_search
            && ef == 0
        {
            errors.push(ConfigValidationError::invalid_field(
                "hnsw_ef_search",
                "must be greater than 0",
            ));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl LocalVectorConfig {
    pub fn with_data_dir(mut self, dir: impl Into<String>) -> Self {
        self.data_dir = Some(dir.into());
        self
    }

    pub fn with_vector_size(mut self, size: usize) -> Self {
        self.vector_size = size;
        self
    }

    /// Resolve the effective data directory against the SQLite path.
    pub fn resolved_data_dir(&self, sqlite_path: &str) -> String {
        if let Some(ref dir) = self.data_dir {
            return dir.clone();
        }
        let parent = std::path::Path::new(sqlite_path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| ".".to_string());
        format!("{parent}/vectors")
    }
}

/// Collection configuration preset
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CollectionPreset {
    /// Tiny: <= 2000 vectors, no HNSW
    Tiny,
    /// Small: 2000 - 10000 vectors
    Small,
    /// Medium: 10000 - 100000 vectors
    #[default]
    Medium,
    /// Large: > 100000 vectors
    Large,
}

impl CollectionPreset {
    pub fn from_vector_count(count: usize) -> Self {
        if count <= 2000 {
            Self::Tiny
        } else if count <= 10000 {
            Self::Small
        } else if count <= 100000 {
            Self::Medium
        } else {
            Self::Large
        }
    }

    pub fn hnsw_config(&self) -> Option<HnswConfig> {
        match self {
            Self::Tiny => HnswConfig::tiny(),
            Self::Small => Some(HnswConfig::small()),
            Self::Medium => Some(HnswConfig::medium()),
            Self::Large => Some(HnswConfig::large()),
        }
    }

    pub fn wal_config(&self) -> WalConfig {
        match self {
            Self::Tiny => WalConfig::small(),
            Self::Small => WalConfig::small(),
            Self::Medium => WalConfig::medium(),
            Self::Large => WalConfig::large(),
        }
    }
}

/// HNSW (Hierarchical Navigable Small World) index configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HnswConfig {
    /// Number of neighbors per node (2-128)
    pub m: u32,

    /// Search range during index construction (10-1000)
    pub ef_construct: u32,

    /// Store HNSW index on disk
    pub on_disk: bool,

    /// Additional HNSW connections for payload-aware routing
    pub payload_m: Option<u32>,

    /// Store vector copies directly in HNSW index files (v1.16.0+)
    /// Requires quantization to be enabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_storage: Option<bool>,
}

impl Default for HnswConfig {
    fn default() -> Self {
        // Medium preset defaults
        Self {
            m: 32,
            ef_construct: 256,
            on_disk: true,
            payload_m: Some(32),
            inline_storage: None,
        }
    }
}

impl HnswConfig {
    /// Create a new HNSW config from explicit parameters
    pub fn new(m: u32, ef_construct: u32, on_disk: bool) -> Self {
        Self {
            m,
            ef_construct,
            on_disk,
            payload_m: Some(m),
            inline_storage: None,
        }
    }

    /// Create tiny preset (no HNSW for very small datasets)
    pub fn tiny() -> Option<Self> {
        None // No HNSW for tiny datasets
    }

    /// Create small preset
    pub fn small() -> Self {
        Self {
            m: 16,
            ef_construct: 128,
            on_disk: true,
            payload_m: Some(16),
            inline_storage: None,
        }
    }

    /// Create medium preset
    pub fn medium() -> Self {
        Self {
            m: 32,
            ef_construct: 256,
            on_disk: true,
            payload_m: Some(32),
            inline_storage: None,
        }
    }

    /// Create large preset
    pub fn large() -> Self {
        Self {
            m: 64,
            ef_construct: 512,
            on_disk: true,
            payload_m: Some(64),
            inline_storage: None,
        }
    }
}

impl Validate for HnswConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        if self.m < 2 || self.m > 128 {
            errors.push(ConfigValidationError::out_of_range(
                "m",
                self.m.to_string(),
                "2",
                "128",
            ));
        }
        if self.ef_construct < 10 || self.ef_construct > 1000 {
            errors.push(ConfigValidationError::out_of_range(
                "ef_construct",
                self.ef_construct.to_string(),
                "10",
                "1000",
            ));
        }
        if let Some(payload_m) = self.payload_m {
            if !(2..=128).contains(&payload_m) {
                errors.push(ConfigValidationError::out_of_range(
                    "payload_m",
                    payload_m.to_string(),
                    "2",
                    "128",
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

/// Vector storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VectorStorageConfig {
    /// Store vectors on disk
    pub on_disk: bool,
}

impl Default for VectorStorageConfig {
    fn default() -> Self {
        Self { on_disk: true }
    }
}

/// Scalar quantization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScalarQuantizationConfig {
    /// Quantization type: int8 or int16
    #[serde(rename = "type")]
    pub quant_type: String,

    /// Quantile for excluding outliers (0.0-1.0)
    pub quantile: f32,

    /// Keep quantized vectors always in RAM
    pub always_ram: bool,
}

impl Default for ScalarQuantizationConfig {
    fn default() -> Self {
        Self {
            quant_type: "int8".to_string(),
            quantile: 0.99,
            always_ram: false,
        }
    }
}

impl Validate for ScalarQuantizationConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        if self.quant_type != "int8" && self.quant_type != "int16" {
            errors.push(ConfigValidationError::invalid_field(
                "quant_type",
                format!("must be 'int8' or 'int16', got '{}'", self.quant_type),
            ));
        }
        if self.quantile < 0.0 || self.quantile > 1.0 {
            errors.push(ConfigValidationError::out_of_range(
                "quantile",
                self.quantile.to_string(),
                "0.0",
                "1.0",
            ));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

/// Product quantization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProductQuantizationConfig {
    /// Compression ratio: x8, x16, x32, x64, x128
    pub compression: String,

    /// Keep quantized vectors always in RAM
    pub always_ram: bool,
}

impl Default for ProductQuantizationConfig {
    fn default() -> Self {
        Self {
            compression: "x32".to_string(),
            always_ram: false,
        }
    }
}

impl Validate for ProductQuantizationConfig {
    fn validate_structured(&self) -> ValidationResult {
        let valid_compressions = ["x8", "x16", "x32", "x64", "x128"];
        if !valid_compressions.contains(&self.compression.as_str()) {
            return Err(ConfigValidationError::invalid_field(
                "compression",
                format!(
                    "must be one of {:?}, got '{}'",
                    valid_compressions, self.compression
                ),
            ));
        }
        Ok(())
    }
}

/// Quantization configuration (enum with type discriminator)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum QuantizationConfig {
    /// Scalar quantization (int8/int16)
    Scalar(ScalarQuantizationConfig),
    /// Product quantization
    Product(ProductQuantizationConfig),
    /// Disabled quantization
    #[default]
    Disabled,
}

impl Validate for QuantizationConfig {
    fn validate_structured(&self) -> ValidationResult {
        match self {
            Self::Scalar(config) => config.validate_structured(),
            Self::Product(config) => config.validate_structured(),
            Self::Disabled => Ok(()),
        }
    }
}

/// WAL (Write-Ahead Log) configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WalConfig {
    /// WAL capacity in MB
    pub capacity_mb: u32,

    /// Number of WAL segments
    pub segments: u32,
}

impl Default for WalConfig {
    fn default() -> Self {
        Self {
            capacity_mb: 32,
            segments: 2,
        }
    }
}

impl WalConfig {
    /// Create tiny/small preset
    pub fn small() -> Self {
        Self {
            capacity_mb: 32,
            segments: 2,
        }
    }

    /// Create medium preset
    pub fn medium() -> Self {
        Self {
            capacity_mb: 64,
            segments: 4,
        }
    }

    /// Create large preset
    pub fn large() -> Self {
        Self {
            capacity_mb: 256,
            segments: 8,
        }
    }
}

/// Qdrant client configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct QdrantConfig {
    /// Qdrant server URL
    pub url: String,
    /// API key for authentication
    pub api_key: Option<String>,
    /// Vector dimension
    pub vector_size: usize,
    /// Distance metric
    pub distance_metric: DistanceMetric,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Initial retry delay in milliseconds
    pub retry_delay_ms: u64,
    /// Whether the client is enabled
    pub enabled: bool,
    /// Collection configuration preset
    pub preset: CollectionPreset,

    // --- Subprocess Management ---
    /// Automatically start and manage the Qdrant process (no external startup needed)
    pub auto_start: bool,
    /// Path to the Qdrant binary (optional, auto-searched in PATH and common locations)
    pub binary_path: Option<String>,
    /// Qdrant data directory (only effective when auto_start is true)
    pub data_dir: Option<String>,
    /// Startup timeout in seconds (default: 60)
    pub startup_timeout_secs: u64,
    /// Auto-restart Qdrant process on unexpected exit
    pub auto_restart: bool,

    // --- Advanced Qdrant Configuration ---
    /// HNSW index configuration (overrides preset defaults)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hnsw: Option<HnswConfig>,

    /// Vector storage configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector_storage: Option<VectorStorageConfig>,

    /// Quantization configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantization: Option<QuantizationConfig>,

    /// WAL configuration (overrides preset defaults)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wal: Option<WalConfig>,
}

impl Default for QdrantConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:6333".to_string(),
            api_key: None,
            vector_size: 1024,
            distance_metric: DistanceMetric::Cosine,
            timeout_ms: 30000,
            max_retries: 3,
            retry_delay_ms: 1000,
            enabled: true,
            preset: CollectionPreset::Medium,
            auto_start: false,
            binary_path: None,
            data_dir: None,
            startup_timeout_secs: 60,
            auto_restart: false,
            hnsw: None,
            vector_storage: None,
            quantization: None,
            wal: None,
        }
    }
}

impl Validate for QdrantConfig {
    fn validate_structured(&self) -> ValidationResult {
        let mut errors = Vec::new();

        if self.url.is_empty() {
            errors.push(ConfigValidationError::missing_field("url"));
        }
        if self.vector_size == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "vector_size",
                "must be greater than 0",
            ));
        }
        if self.timeout_ms == 0 {
            errors.push(ConfigValidationError::invalid_field(
                "timeout_ms",
                "must be greater than 0",
            ));
        }
        if self.max_retries > 10 {
            errors.push(ConfigValidationError::out_of_range(
                "max_retries",
                self.max_retries.to_string(),
                "0",
                "10",
            ));
        }
        if self.startup_timeout_secs == 0 || self.startup_timeout_secs > 300 {
            errors.push(ConfigValidationError::out_of_range(
                "startup_timeout_secs",
                self.startup_timeout_secs.to_string(),
                "1",
                "300",
            ));
        }
        if self.auto_start
            && self.url != "http://localhost:6333"
            && self.url != "http://127.0.0.1:6333"
        {
            errors.push(ConfigValidationError::dependency_conflict(
                "auto_start requires localhost URL (http://localhost:6333)",
            ));
        }
        let normalized = self.normalized_url();
        if !normalized.starts_with("http://") && !normalized.starts_with("https://") {
            errors.push(ConfigValidationError::invalid_field(
                "url",
                format!("invalid URL: {}", self.url),
            ));
        }
        if let Some(ref hnsw) = self.hnsw {
            if let Err(e) = hnsw.validate_structured() {
                errors.push(e);
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigValidationError::multiple(errors))
        }
    }
}

impl QdrantConfig {
    pub fn with_url(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Self::default()
        }
    }

    pub fn with_vector_size(vector_size: usize) -> Self {
        Self {
            vector_size,
            ..Self::default()
        }
    }

    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    pub fn timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    pub fn preset(mut self, preset: CollectionPreset) -> Self {
        self.preset = preset;
        self
    }

    pub fn enabled(mut self) -> Self {
        self.enabled = true;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub fn normalized_url(&self) -> String {
        self.parse_url(&self.url)
    }

    fn parse_url(&self, url: &str) -> String {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return "http://localhost:6333".to_string();
        }
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            return trimmed.to_string();
        }
        if trimmed.contains(':') {
            format!("http://{}", trimmed)
        } else {
            format!("http://{}:6333", trimmed)
        }
    }

    /// Get HNSW configuration based on preset or manual override
    pub fn get_hnsw_config(&self) -> Option<HnswConfig> {
        if let Some(ref hnsw) = self.hnsw {
            return Some(hnsw.clone());
        }
        match self.preset {
            CollectionPreset::Tiny => HnswConfig::tiny(),
            CollectionPreset::Small => Some(HnswConfig::small()),
            CollectionPreset::Medium => Some(HnswConfig::medium()),
            CollectionPreset::Large => Some(HnswConfig::large()),
        }
    }

    /// Get WAL configuration based on preset or manual override
    pub fn get_wal_config(&self) -> WalConfig {
        if let Some(ref wal) = self.wal {
            return wal.clone();
        }
        match self.preset {
            CollectionPreset::Tiny | CollectionPreset::Small => WalConfig::small(),
            CollectionPreset::Medium => WalConfig::medium(),
            CollectionPreset::Large => WalConfig::large(),
        }
    }

    /// Get vector storage configuration
    pub fn get_vector_storage_config(&self) -> VectorStorageConfig {
        self.vector_storage.clone().unwrap_or_default()
    }

    /// Check if subprocess management is enabled
    pub fn is_process_managed(&self) -> bool {
        self.auto_start
    }

    /// Get resolved binary path, falling back to auto-detection
    pub fn resolved_binary_path(&self) -> Option<String> {
        self.binary_path.clone()
    }

    /// Get resolved data directory
    pub fn resolved_data_dir(&self) -> Option<String> {
        self.data_dir.clone()
    }
}

/// BM25 algorithm configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Bm25AlgorithmConfig {
    /// k1 parameter (term frequency saturation)
    pub k1: f32,
    /// b parameter (document length normalization)
    pub b: f32,
}

impl Default for Bm25AlgorithmConfig {
    fn default() -> Self {
        // Optimized for code search based on bm25_parameter_sweep benchmark
        // (see docs/archive/bm25-parameter-tuning.md)
        // k1=1.8: Higher value increases term frequency impact for exact identifier matches
        // b=0.6: Best average MRR@10 across benchmark fixtures
        Self { k1: 1.8, b: 0.6 }
    }
}

/// Index manager configuration for Tantivy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IndexManagerConfig {
    /// Writer memory budget in bytes (default: 50MB)
    pub writer_memory_budget: usize,
    /// Number of writer threads (None for auto-detection)
    pub writer_num_threads: Option<usize>,
    /// Enable reader caching
    pub reader_cache_enabled: bool,
    /// Reload policy: "on_commit", "on_commit_with_delay", "manual"
    pub reload_policy: String,
}

impl Default for IndexManagerConfig {
    fn default() -> Self {
        Self {
            writer_memory_budget: 50_000_000,
            writer_num_threads: None,
            reader_cache_enabled: true,
            reload_policy: "on_commit_with_delay".to_string(),
        }
    }
}

/// BM25 client configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Bm25Config {
    /// Enable BM25 indexing
    pub enabled: bool,
    /// Local index path for embedded Tantivy
    pub index_path: Option<String>,
    /// Default index name
    pub index_name: String,
    /// Connection timeout in milliseconds
    pub timeout_ms: u64,
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Retry delay in milliseconds
    pub retry_delay_ms: u64,
    /// BM25 algorithm parameters
    pub algorithm: Bm25AlgorithmConfig,
    /// Index manager configuration
    pub index_manager: IndexManagerConfig,
    /// Maximum on-disk index size in bytes before writes fail fast.
    /// Zero means unlimited.
    pub max_index_bytes: u64,
}

impl Default for Bm25Config {
    fn default() -> Self {
        Self {
            enabled: false,
            index_path: None,
            index_name: "code_index".to_string(),
            timeout_ms: 5000,
            max_retries: 3,
            retry_delay_ms: 100,
            algorithm: Bm25AlgorithmConfig::default(),
            index_manager: IndexManagerConfig::default(),
            max_index_bytes: 0,
        }
    }
}

impl Bm25Config {
    pub fn with_index_name(mut self, index_name: impl Into<String>) -> Self {
        self.index_name = index_name.into();
        self
    }

    pub fn enabled(mut self) -> Self {
        self.enabled = true;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    pub fn with_index_path(mut self, index_path: impl Into<String>) -> Self {
        self.index_path = Some(index_path.into());
        self
    }

    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    pub fn with_max_index_bytes(mut self, max_index_bytes: u64) -> Self {
        self.max_index_bytes = max_index_bytes;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_qdrant_config() {
        let config = QdrantConfig::default();
        assert_eq!(config.url, "http://localhost:6333");
        assert_eq!(config.vector_size, 1024);
        assert!(config.enabled);
    }

    #[test]
    fn test_default_bm25_config() {
        let config = Bm25Config::default();
        assert!(!config.enabled);
        assert_eq!(config.index_name, "code_index");
    }

    #[test]
    fn test_hnsw_presets() {
        let small = HnswConfig::small();
        assert_eq!(small.m, 16);
        assert_eq!(small.ef_construct, 128);
        assert!(small.validate_structured().is_ok());

        let medium = HnswConfig::medium();
        assert_eq!(medium.m, 32);
        assert_eq!(medium.ef_construct, 256);

        let large = HnswConfig::large();
        assert_eq!(large.m, 64);
        assert_eq!(large.ef_construct, 512);
    }

    #[test]
    fn test_hnsw_validation() {
        let invalid = HnswConfig::new(1, 128, true);
        assert!(invalid.validate_structured().is_err());

        let invalid = HnswConfig::new(16, 5, true);
        assert!(invalid.validate_structured().is_err());

        let invalid_payload_m = HnswConfig {
            payload_m: Some(200),
            ..HnswConfig::medium()
        };
        assert!(invalid_payload_m.validate_structured().is_err());
    }

    #[test]
    fn test_quantization_validation() {
        assert!(QuantizationConfig::Disabled.validate_structured().is_ok());
        assert!(
            QuantizationConfig::Scalar(ScalarQuantizationConfig::default())
                .validate_structured()
                .is_ok()
        );
        let invalid = QuantizationConfig::Scalar(ScalarQuantizationConfig {
            quant_type: "int4".to_string(),
            ..Default::default()
        });
        assert!(invalid.validate_structured().is_err());
        assert!(
            QuantizationConfig::Product(ProductQuantizationConfig {
                compression: "x999".to_string(),
                ..Default::default()
            })
            .validate_structured()
            .is_err()
        );
    }
}
