//! Specialized summary generators for different file categories
//!
//! Provides custom summary generation for specific file types like tests, configs, etc.
use crate::summary::strategy::{FileCategory, ImportanceLevel, TestType};
use crate::summary::types::FileSummary;
use cce_types::{EntityKind, ParsedFile};

fn attach_file_doc_comment(summary: &mut FileSummary, parsed_file: &ParsedFile) {
    summary.file_doc_comment = parsed_file.file_doc_comment.clone();
}

/// Generate specialized summary for test files
pub fn generate_test_file_summary(parsed_file: &ParsedFile) -> FileSummary {
    let file_name = std::path::Path::new(&parsed_file.path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();

    let test_type = TestType::detect(parsed_file);

    // Count test functions
    let test_count = parsed_file
        .entities
        .iter()
        .filter(|e| {
            e.kind == EntityKind::Function
                && (e.name.starts_with("test_") || e.signature.contains("#test"))
        })
        .count();

    // Extract test target from path
    let target = file_name
        .trim_end_matches("_test.rs")
        .trim_end_matches(".test.rs")
        .trim_end_matches("_spec.rs")
        .trim_end_matches(".spec.ts")
        .to_string();

    let summary_text = match test_type {
        TestType::Unit => format!(
            "Unit test file for {} - contains {} test functions",
            target, test_count
        ),
        TestType::Integration => format!(
            "Integration test file for {} - contains {} test scenarios",
            target, test_count
        ),
        TestType::E2E => format!("End-to-end test file - tests {} flows", target),
        TestType::Benchmark => format!("Benchmark tests for {}", target),
    };

    let mut summary = FileSummary::new(&parsed_file.path)
        .with_language(parsed_file.language.to_string())
        .with_category(FileCategory::determine(parsed_file))
        .with_summary(summary_text)
        .with_tags(vec![
            "test".to_string(),
            test_type.as_str().to_string(),
            parsed_file.language.to_string().to_lowercase(),
        ])
        .with_importance_level(ImportanceLevel::Low)
        .with_line_count(parsed_file.source.lines().count() as u32);
    summary.entity_count = parsed_file.entities.len() as u32;
    attach_file_doc_comment(&mut summary, parsed_file);

    summary
}

/// Language label for document/config files, mirroring the naming used by
/// `DocSummary::to_file_summary` so both summary sources agree.
fn document_language_name(path: &str) -> String {
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match extension.as_deref() {
        Some("md") | Some("markdown") => "markdown".to_string(),
        Some("xml") => "xml".to_string(),
        Some(ext @ ("json" | "toml" | "yaml" | "yml")) => ext.to_string(),
        _ => "text".to_string(),
    }
}

/// Generate specialized summary for config files
pub fn generate_config_file_summary(parsed_file: &ParsedFile) -> FileSummary {
    let file_name = std::path::Path::new(&parsed_file.path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();

    let config_type = if file_name.contains("database") || file_name.contains("db") {
        "database"
    } else if file_name.contains("app") || file_name.contains("application") {
        "application"
    } else if file_name.contains("build")
        || file_name == "Cargo.toml"
        || file_name == "package.json"
    {
        "build"
    } else if file_name.starts_with(".") || file_name.contains("env") {
        "environment"
    } else {
        "configuration"
    };

    let summary_text = format!(
        "{} configuration file - defines project settings",
        config_type
    );

    let mut summary = FileSummary::new(&parsed_file.path)
        .with_language(parsed_file.language.to_string())
        .with_category(FileCategory::Config)
        .with_summary(summary_text)
        .with_tags(vec!["config".to_string(), config_type.to_string()])
        .with_importance_level(ImportanceLevel::Low)
        .with_line_count(parsed_file.source.lines().count() as u32);
    summary.entity_count = 0;
    attach_file_doc_comment(&mut summary, parsed_file);

    summary
}

/// Generate specialized summary for documentation files
pub fn generate_documentation_summary(parsed_file: &ParsedFile) -> FileSummary {
    let file_name = std::path::Path::new(&parsed_file.path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();

    // Try to extract title from first heading
    let title = parsed_file
        .source
        .lines()
        .find(|l| l.starts_with("# "))
        .map(|l| l.trim_start_matches("# ").to_string())
        .unwrap_or_else(|| "Untitled".to_string());

    let (doc_type, summary_text) = if file_name.contains("readme") {
        ("readme", format!("Project documentation - {}", title))
    } else if file_name.contains("changelog") {
        (
            "changelog",
            "Changelog - records version history".to_string(),
        )
    } else if file_name.contains("license") {
        ("license", "License file - defines usage terms".to_string())
    } else if file_name.contains("contributing") {
        ("contributing", "Contributing guidelines".to_string())
    } else {
        ("documentation", format!("Documentation file - {}", title))
    };

    let mut summary = FileSummary::new(&parsed_file.path)
        .with_language(document_language_name(&parsed_file.path))
        .with_category(FileCategory::Documentation)
        .with_summary(summary_text)
        .with_tags(vec!["documentation".to_string(), doc_type.to_string()])
        .with_importance_level(ImportanceLevel::Medium)
        .with_line_count(parsed_file.source.lines().count() as u32);
    summary.entity_count = 0;
    attach_file_doc_comment(&mut summary, parsed_file);

    summary
}

/// Generate specialized summary for generated files
pub fn generate_generated_file_summary(parsed_file: &ParsedFile) -> FileSummary {
    // Try to detect generator
    let generator = parsed_file.source.lines().take(10).find_map(|line| {
        let lower = line.to_lowercase();
        if lower.contains("generated by") {
            line.split("generated by")
                .nth(1)
                .map(|s| s.trim().to_string())
        } else if lower.contains("code generated by") {
            line.split("code generated by")
                .nth(1)
                .map(|s| s.trim().to_string())
        } else {
            None
        }
    });

    let summary_text = match generator {
        Some(g) => format!("Auto-generated code by {} - do not edit manually", g),
        None => "Auto-generated file - do not edit manually".to_string(),
    };

    let mut summary = FileSummary::new(&parsed_file.path)
        .with_language(parsed_file.language.to_string())
        .with_category(FileCategory::Code)
        .with_summary(summary_text)
        .with_tags(vec!["generated".to_string()])
        .with_importance_level(ImportanceLevel::Low)
        .with_line_count(parsed_file.source.lines().count() as u32);
    summary.entity_count = parsed_file.entities.len() as u32;
    attach_file_doc_comment(&mut summary, parsed_file);

    summary
}

/// Generate a lightweight specialized summary for generic text content
/// (logs, `.txt`, unknown extensions — [`FileCategory::Other`]).
///
/// These files carry no structural semantics, so the summary stays at the
/// shape level (line/byte counts) instead of pretending deeper information.
fn generate_other_file_summary(parsed_file: &ParsedFile) -> FileSummary {
    let line_count = parsed_file.source.lines().count() as u32;
    let kind = if parsed_file.path.to_lowercase().ends_with(".log") {
        "log"
    } else {
        "text"
    };
    let summary_text = format!("{kind} file - {line_count} lines of unstructured content");

    let mut summary = FileSummary::new(&parsed_file.path)
        .with_language(parsed_file.language.to_string())
        .with_category(FileCategory::Other)
        .with_summary(summary_text)
        .with_tags(vec![kind.to_string()])
        .with_importance_level(ImportanceLevel::Low)
        .with_line_count(line_count);
    summary.entity_count = 0;
    attach_file_doc_comment(&mut summary, parsed_file);

    summary
}

/// Generate specialized summary for schema files
pub fn generate_schema_file_summary(parsed_file: &ParsedFile) -> FileSummary {
    let schema_type = if parsed_file.path.ends_with(".proto") {
        "Protobuf"
    } else if parsed_file.path.ends_with(".graphql") {
        "GraphQL"
    } else if parsed_file.path.ends_with(".thrift") {
        "Thrift"
    } else if parsed_file.path.ends_with(".avsc") {
        "Avro"
    } else {
        "Schema"
    };

    let type_count = parsed_file.entities.len();

    let summary_text = format!(
        "{} schema definition - defines {} types/messages",
        schema_type, type_count
    );

    let mut summary = FileSummary::new(&parsed_file.path)
        .with_language(parsed_file.language.to_string())
        .with_category(FileCategory::Schema)
        .with_summary(summary_text)
        .with_tags(vec!["schema".to_string(), "api-definition".to_string()])
        .with_importance_level(ImportanceLevel::Medium)
        .with_line_count(parsed_file.source.lines().count() as u32);
    summary.entity_count = type_count as u32;
    attach_file_doc_comment(&mut summary, parsed_file);

    summary
}

/// Generate specialized summary based on file routing.
///
/// Dispatch order: test files first, then the content-type category
/// (config/documentation/schema), then generated files. Test and generated
/// markers are orthogonal to the content type: test/generated files carry
/// the category of their content (e.g. a generated `.rs` file is `Code`).
pub fn generate_specialized_summary(parsed_file: &ParsedFile) -> FileSummary {
    let mut summary = if FileCategory::is_test_file(&parsed_file.path) {
        generate_test_file_summary(parsed_file)
    } else {
        let category = FileCategory::determine(parsed_file);
        match category {
            FileCategory::Config => generate_config_file_summary(parsed_file),
            FileCategory::Documentation => generate_documentation_summary(parsed_file),
            FileCategory::Schema => generate_schema_file_summary(parsed_file),
            // Generic text gets its own lightweight summary; generated files
            // are the only `Code`-category files routed here.
            FileCategory::Other => generate_other_file_summary(parsed_file),
            FileCategory::Code => {
                if FileCategory::is_generated_file(&parsed_file.path, &parsed_file.source) {
                    generate_generated_file_summary(parsed_file)
                } else {
                    panic!("Code files should not use specialized summary generation")
                }
            }
        }
    };
    // Content-type category (test/generated files keep their content type)
    summary.category = Some(FileCategory::determine(parsed_file));
    // Specialized summaries carry only the path-level marker: their files are
    // classified by path heuristics (see `FileCategory::determine`) and no
    // group aggregation is available at this stage.
    summary.test_info =
        cce_types::TestInfo::from_path(Some(&parsed_file.language), &parsed_file.path);
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use cce_types::{Entity, EntityId, EntityKind, Language, ParsedFile};

    #[test]
    fn test_generate_specialized_summary_test_info_path_rule() {
        // Test-category files carry a path-level Test marker.
        let test_file = ParsedFile::new(Language::Rust, "src/lib_test.rs".to_string(), "");
        let summary = generate_specialized_summary(&test_file);
        assert!(summary.test_info.is_test());

        // Config-category files carry the path rule result (unknown here).
        let config_file = ParsedFile::new(Language::Rust, "settings.toml".to_string(), "");
        let summary = generate_specialized_summary(&config_file);
        assert!(summary.test_info.is_unknown());
        assert_eq!(summary.category, Some(FileCategory::Config));

        // Generated code files carry the code category.
        let generated_file = ParsedFile::new(
            Language::Rust,
            "src/generated.rs".to_string(),
            "// code generated by protoc\npub struct Foo;".to_string(),
        );
        let summary = generate_specialized_summary(&generated_file);
        assert_eq!(summary.category, Some(FileCategory::Code));
    }

    #[test]
    fn test_generate_test_file_summary() {
        let mut file = ParsedFile::new(Language::Rust, "src/lib_test.rs".to_string(), "");
        file.file_doc_comment = Some("Test file docs".to_string());

        // Add test functions
        for i in 0..3 {
            let entity = Entity {
                id: EntityId(i),
                kind: EntityKind::Function,
                name: format!("test_case_{}", i),
                signature: format!("fn test_case_{}()", i),
                parameters: Vec::new(),
                return_type: None,
                span: cce_types::Span::default(),
                depth: 0,
                parent: None,
                children: Vec::new(),
                doc_comment: None,
                modifiers: Vec::new(),
                attributes: std::collections::HashMap::new(),
                metadata: std::collections::HashMap::new(),
                is_stdlib: false,
                subtype: None,
                stdlib_category: None,
            };
            file.add_entity(entity);
        }

        let summary = generate_test_file_summary(&file);
        assert!(summary.summary_text.contains("Unit test file"));
        assert!(summary.summary_text.contains("3 test functions"));
        assert!(summary.tags.contains(&"test".to_string()));
        assert_eq!(summary.file_doc_comment.as_deref(), Some("Test file docs"));
    }

    #[test]
    fn test_generate_config_file_summary() {
        let file = ParsedFile::new(Language::Unknown, "config.yaml".to_string(), "");

        let summary = generate_config_file_summary(&file);
        assert!(summary.summary_text.contains("configuration file"));
        assert_eq!(summary.importance_level, ImportanceLevel::Low);
    }

    #[test]
    fn test_generate_documentation_summary() {
        let file = ParsedFile::new(
            Language::Unknown,
            "README.md".to_string(),
            "# My Project\n\nDescription here.".to_string(),
        );

        let summary = generate_documentation_summary(&file);
        assert!(summary.summary_text.contains("Project documentation"));
        assert!(summary.summary_text.contains("My Project"));
    }

    #[test]
    fn test_generate_generated_file_summary() {
        let file = ParsedFile::new(
            Language::Rust,
            "src/generated.rs".to_string(),
            "// Code generated by protoc\nfn main() {}".to_string(),
        );

        let summary = generate_generated_file_summary(&file);
        assert!(summary.summary_text.contains("protoc"));
        assert!(summary.summary_text.contains("Auto-generated"));
    }

    #[test]
    fn test_generate_schema_file_summary() {
        let file = ParsedFile::new(Language::Unknown, "api.proto".to_string(), "");

        let summary = generate_schema_file_summary(&file);
        assert!(summary.summary_text.contains("Protobuf"));
        assert!(summary.tags.contains(&"schema".to_string()));
    }
}
