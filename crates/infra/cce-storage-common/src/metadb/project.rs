//! Project registry record types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRecord {
    pub id: i64,
    pub name: String,
    pub root_path: String,
    pub config_file_path: String,
    pub language: Option<String>,
    /// Canonical project profile document (ProjectAppConfig as JSON).
    ///
    /// This is the single source of truth for project-level effective
    /// configuration. Scan lists are derived from the profile at read time
    /// and never stored as separate columns.
    pub profile_json: String,
    /// Monotonic profile version, bumped on every profile write.
    pub profile_version: i64,
    pub last_indexed: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl ProjectRecord {
    pub fn profile_version_u64(&self) -> u64 {
        self.profile_version.max(0) as u64
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewProjectRecord {
    pub name: String,
    pub root_path: String,
    pub config_file_path: Option<String>,
    pub language: Option<String>,
    /// Initial profile document. None means empty profile (`{}`).
    pub profile_json: Option<String>,
}

impl NewProjectRecord {
    pub fn new(name: String, root_path: String) -> Self {
        Self {
            name,
            root_path,
            config_file_path: Some(".cce/config.json".to_string()),
            language: None,
            profile_json: None,
        }
    }

    pub fn with_profile_json(mut self, profile_json: String) -> Self {
        self.profile_json = Some(profile_json);
        self
    }

    pub fn build(self) -> ProjectRecord {
        use chrono::Utc;
        let now = Utc::now().timestamp();

        ProjectRecord {
            id: 0,
            name: self.name,
            root_path: self.root_path,
            config_file_path: self
                .config_file_path
                .unwrap_or_else(|| ".cce/config.json".to_string()),
            language: self.language,
            profile_json: self.profile_json.unwrap_or_else(|| "{}".to_string()),
            profile_version: 0,
            last_indexed: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectUpdateRecord {
    pub name: Option<String>,
    pub root_path: Option<String>,
    pub config_file_path: Option<String>,
    pub language: Option<String>,
    /// Replacement profile document. When set, the repository must also
    /// bump `profile_version` atomically in the same statement.
    pub profile_json: Option<String>,
    pub last_indexed: Option<String>,
}

impl ProjectUpdateRecord {
    pub fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    pub fn with_root_path(mut self, root_path: String) -> Self {
        self.root_path = Some(root_path);
        self
    }

    pub fn with_config_file_path(mut self, config_file_path: String) -> Self {
        self.config_file_path = Some(config_file_path);
        self
    }

    pub fn with_language(mut self, language: String) -> Self {
        self.language = Some(language);
        self
    }

    pub fn with_profile_json(mut self, profile_json: String) -> Self {
        self.profile_json = Some(profile_json);
        self
    }

    pub fn with_last_indexed(mut self, last_indexed: String) -> Self {
        self.last_indexed = Some(last_indexed);
        self
    }
}
