//! Generation manifest, GC plan, and admission audit types.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideDisposition {
    Replaced,
    Deleted,
}

impl OverrideDisposition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Replaced => "replaced",
            Self::Deleted => "deleted",
        }
    }

    pub fn parse(value: &str) -> Result<Self, cce_types::StorageError> {
        match value {
            "replaced" => Ok(Self::Replaced),
            "deleted" => Ok(Self::Deleted),
            other => Err(cce_types::StorageError::Query(format!(
                "invalid generation override disposition: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationOverride {
    pub file_path: String,
    pub disposition: OverrideDisposition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectIndexManifestState {
    Building,
    Active,
    Failed,
}

impl ProjectIndexManifestState {
    pub fn parse(value: &str) -> Result<Self, cce_types::StorageError> {
        match value {
            "building" => Ok(Self::Building),
            "active" => Ok(Self::Active),
            "failed" => Ok(Self::Failed),
            _ => Err(cce_types::StorageError::Query(format!(
                "invalid project index manifest state: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProjectIndexManifest {
    pub project_id: i64,
    pub publication_epoch: i64,
    pub data_epoch: i64,
    pub relation_epoch: i64,
    pub operation_id: String,
    pub state: ProjectIndexManifestState,
    pub input_fingerprint: Option<String>,
    pub candidate_ready: bool,
    pub parent_data_epoch: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GenerationGcPlan {
    pub stale_publication_epochs: Vec<i64>,
    pub stale_data_epochs: Vec<i64>,
    pub stale_relation_epochs: Vec<i64>,
    pub protected_data_epochs: Vec<i64>,
    pub protected_relation_epochs: Vec<i64>,
}

#[derive(Debug, Clone)]
pub struct AdmissionAuditRecord {
    pub token_fingerprint: String,
    pub projects: String,
    pub quota_bytes: Option<i64>,
    pub bytes_used: i64,
    pub admitted: i64,
    pub auth_rejections: i64,
    pub scope_rejections: i64,
    pub rate_rejections: i64,
    pub body_rejections: i64,
    pub quota_rejections: i64,
    pub last_used: Option<i64>,
    pub last_reject_reason: Option<String>,
}
