//! Text preprocessing strategies for embedder

use serde::{Deserialize, Serialize};

/// Text preprocessing strategy trait
pub trait TextPreprocessor: Send + Sync {
    /// Process a single text
    fn process(&self, text: &str) -> String;

    /// Process multiple texts
    fn process_batch(&self, texts: &[&str]) -> Vec<String> {
        texts.iter().map(|text| self.process(text)).collect()
    }
}

/// Simple prefix preprocessor
#[derive(Debug, Clone)]
pub struct PrefixPreprocessor {
    prefix: String,
}

impl PrefixPreprocessor {
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }
}

impl TextPreprocessor for PrefixPreprocessor {
    fn process(&self, text: &str) -> String {
        format!("{}{}", self.prefix, text)
    }
}

/// Template-based preprocessor with placeholder substitution
#[derive(Debug, Clone)]
pub struct TemplatePreprocessor {
    template: String,
}

impl TemplatePreprocessor {
    pub fn new(template: impl Into<String>) -> Self {
        Self {
            template: template.into(),
        }
    }
}

impl TextPreprocessor for TemplatePreprocessor {
    fn process(&self, text: &str) -> String {
        self.template.replace("{text}", text)
    }
}

/// Nomic-Embed task types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NomicTaskType {
    SearchDocument,
    SearchQuery,
    Clustering,
    Classification,
}

impl NomicTaskType {
    pub fn as_prefix(&self) -> &'static str {
        match self {
            Self::SearchDocument => "search_document: ",
            Self::SearchQuery => "search_query: ",
            Self::Clustering => "clustering: ",
            Self::Classification => "classification: ",
        }
    }
}

/// Nomic-Embed preprocessor
#[derive(Debug, Clone)]
pub struct NomicPreprocessor {
    inner: PrefixPreprocessor,
}

impl NomicPreprocessor {
    pub fn new(task_type: NomicTaskType) -> Self {
        Self {
            inner: PrefixPreprocessor::new(task_type.as_prefix()),
        }
    }
}

#[cfg(test)]
impl NomicPreprocessor {
    pub fn search_document() -> Self {
        Self::new(NomicTaskType::SearchDocument)
    }

    pub fn search_query() -> Self {
        Self::new(NomicTaskType::SearchQuery)
    }
}

impl TextPreprocessor for NomicPreprocessor {
    fn process(&self, text: &str) -> String {
        self.inner.process(text)
    }
}

/// Stella task types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StellaTaskType {
    S2P,
    S2S,
}

/// Stella-EN-400M preprocessor
#[derive(Debug, Clone)]
pub struct StellaPreprocessor {
    inner: TemplatePreprocessor,
}

impl StellaPreprocessor {
    pub fn new(task_type: StellaTaskType) -> Self {
        Self {
            inner: TemplatePreprocessor::new(Self::get_template(task_type)),
        }
    }
}

#[cfg(test)]
impl StellaPreprocessor {
    pub fn s2p() -> Self {
        Self::new(StellaTaskType::S2P)
    }

    pub fn s2s() -> Self {
        Self::new(StellaTaskType::S2S)
    }
}

impl StellaPreprocessor {
    fn get_template(task_type: StellaTaskType) -> &'static str {
        match task_type {
            StellaTaskType::S2P => {
                "Instruct: Given a web search query, retrieve relevant passages that answer the query.\nQuery: {text}"
            }
            StellaTaskType::S2S => "Instruct: Retrieve semantically similar text.\nQuery: {text}",
        }
    }
}

impl TextPreprocessor for StellaPreprocessor {
    fn process(&self, text: &str) -> String {
        self.inner.process(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopPreprocessor;

    impl TextPreprocessor for NoopPreprocessor {
        fn process(&self, text: &str) -> String {
            text.to_string()
        }
    }

    #[test]
    fn test_noop_preprocessor() {
        let preprocessor = NoopPreprocessor;
        assert_eq!(preprocessor.process("Hello"), "Hello");
    }

    #[test]
    fn test_prefix_preprocessor() {
        let preprocessor = PrefixPreprocessor::new("prefix: ");
        assert_eq!(preprocessor.process("Hello"), "prefix: Hello");
    }

    #[test]
    fn test_template_preprocessor() {
        let preprocessor = TemplatePreprocessor::new("Query: {text}");
        assert_eq!(preprocessor.process("Hello"), "Query: Hello");
    }

    #[test]
    fn test_nomic_preprocessor() {
        let preprocessor = NomicPreprocessor::search_document();
        assert_eq!(
            preprocessor.process("Hello world"),
            "search_document: Hello world"
        );

        let preprocessor = NomicPreprocessor::search_query();
        assert_eq!(
            preprocessor.process("What is AI?"),
            "search_query: What is AI?"
        );
    }

    #[test]
    fn test_stella_preprocessor() {
        let preprocessor = StellaPreprocessor::s2p();
        let result = preprocessor.process("machine learning");
        assert!(result.contains("machine learning"));
        assert!(result.contains("Instruct:"));

        let preprocessor = StellaPreprocessor::s2s();
        let result = preprocessor.process("deep learning");
        assert!(result.contains("deep learning"));
        assert!(result.contains("Instruct:"));
    }

    #[test]
    fn test_batch_processing() {
        let preprocessor = NomicPreprocessor::search_document();
        let texts = vec!["Hello", "World"];
        let results = preprocessor.process_batch(&texts);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0], "search_document: Hello");
        assert_eq!(results[1], "search_document: World");
    }
}
