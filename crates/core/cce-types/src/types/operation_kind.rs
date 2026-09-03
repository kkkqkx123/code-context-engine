use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationKind {
    #[serde(rename = "full_index")]
    FullIndex,
    #[serde(rename = "hot_update")]
    HotUpdate,
    #[serde(rename = "incremental")]
    Incremental,
    #[serde(rename = "config_change")]
    ConfigChange,
}

impl OperationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FullIndex => "full_index",
            Self::HotUpdate => "hot_update",
            Self::Incremental => "incremental",
            Self::ConfigChange => "config_change",
        }
    }
}

impl std::fmt::Display for OperationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for OperationKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "full_index" | "full" | "FullIndexing" => Ok(Self::FullIndex),
            "hot_update" | "hot" | "HotUpdate" => Ok(Self::HotUpdate),
            "incremental" | "IncrementalUpdate" => Ok(Self::Incremental),
            "config_change" | "ConfigChange" => Ok(Self::ConfigChange),
            _ => Err(format!("Unknown operation kind: {s}")),
        }
    }
}
