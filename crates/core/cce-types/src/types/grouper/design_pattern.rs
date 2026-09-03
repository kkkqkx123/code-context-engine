use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetterSetterSummary {
    pub properties: Vec<String>,
}

impl GetterSetterSummary {
    pub fn new(properties: Vec<String>) -> Self {
        Self { properties }
    }

    pub fn empty() -> Self {
        Self {
            properties: Vec::new(),
        }
    }
}
