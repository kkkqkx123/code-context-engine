//! Project registry record types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRecord {
    pub id: i64,
    pub name: String,
    pub root_path: String,
    pub config_file_path: String,
    pub language: Option<String>,
    pub extensions: Option<String>,
    pub exclude_dirs: Option<String>,
    pub respect_gitignore: Option<bool>,
    pub ignore_patterns: Option<String>,
    pub last_indexed: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewProjectRecord {
    pub name: String,
    pub root_path: String,
    pub config_file_path: Option<String>,
    pub language: Option<String>,
    pub extensions: Option<String>,
    pub exclude_dirs: Option<String>,
    pub respect_gitignore: Option<bool>,
    pub ignore_patterns: Option<String>,
}

impl NewProjectRecord {
    pub fn new(name: String, root_path: String) -> Self {
        Self {
            name,
            root_path,
            config_file_path: Some(".cce/config.json".to_string()),
            language: None,
            extensions: None,
            exclude_dirs: None,
            respect_gitignore: None,
            ignore_patterns: None,
        }
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
            extensions: self.extensions,
            exclude_dirs: self.exclude_dirs,
            respect_gitignore: self.respect_gitignore,
            ignore_patterns: self.ignore_patterns,
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
    pub extensions: Option<String>,
    pub exclude_dirs: Option<String>,
    pub respect_gitignore: Option<bool>,
    pub ignore_patterns: Option<String>,
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

    pub fn with_extensions(mut self, extensions: String) -> Self {
        self.extensions = Some(extensions);
        self
    }

    pub fn with_exclude_dirs(mut self, exclude_dirs: String) -> Self {
        self.exclude_dirs = Some(exclude_dirs);
        self
    }

    pub fn with_respect_gitignore(mut self, respect_gitignore: bool) -> Self {
        self.respect_gitignore = Some(respect_gitignore);
        self
    }

    pub fn with_ignore_patterns(mut self, ignore_patterns: String) -> Self {
        self.ignore_patterns = Some(ignore_patterns);
        self
    }

    pub fn with_last_indexed(mut self, last_indexed: String) -> Self {
        self.last_indexed = Some(last_indexed);
        self
    }
}
