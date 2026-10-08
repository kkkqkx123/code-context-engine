//! .NET parser for C# projects

use std::collections::HashSet;
use std::path::Path;

use cce_types::language::Language;

use super::super::error::ConfigParseError;
use super::super::trait_def::{LanguageParser, ParseOutcome};
use super::super::types::UntypedDependency;
use quick_xml::Reader;
use quick_xml::events::Event;

/// .NET parser for C# projects
pub struct DotNetParser;

impl LanguageParser for DotNetParser {
    fn try_parse(
        &self,
        project_root: &Path,
        dir: &Path,
    ) -> Result<Option<ParseOutcome>, ConfigParseError> {
        let csproj_files: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| ConfigParseError::Io {
                path: dir.to_path_buf(),
                source: e,
            })?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "csproj") {
                    Some(path)
                } else {
                    None
                }
            })
            .collect();

        if csproj_files.is_empty() {
            return Ok(None);
        }

        let mut all_deps = HashSet::new();
        let mut first_rel = String::new();
        let mut has_success = false;

        for csproj_path in &csproj_files {
            match parse_csproj(csproj_path) {
                Ok(deps) => {
                    if !deps.is_empty() {
                        has_success = true;
                        if first_rel.is_empty() {
                            first_rel = cce_types::path::relativize(project_root, csproj_path);
                        }
                        all_deps.extend(deps);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to parse {}: {}", csproj_path.display(), e);
                }
            }
        }

        if !has_success || all_deps.is_empty() {
            return Ok(None);
        }

        Ok(Some(ParseOutcome {
            dependencies: all_deps,
            config_file: first_rel,
        }))
    }

    fn supported_languages(&self) -> Vec<Language> {
        vec![Language::CSharp]
    }

    fn supported_config_files(&self) -> &[&str] {
        &[]
    }

    fn supports_file(&self, filename: &str) -> bool {
        filename.ends_with(".csproj")
            || filename.ends_with(".fsproj")
            || filename.ends_with(".vbproj")
    }

    fn name(&self) -> &str {
        ".NET"
    }
}

fn parse_csproj(path: &Path) -> Result<HashSet<UntypedDependency>, ConfigParseError> {
    let content = std::fs::read_to_string(path).map_err(|e| ConfigParseError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;

    let mut reader = Reader::from_str(&content);
    reader.config_mut().trim_text(true);

    let mut dependencies = HashSet::new();
    let mut buf = Vec::new();
    let mut current_package = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = std::str::from_utf8(e.name().as_ref())
                    .unwrap_or("")
                    .to_lowercase();

                if name == "packagereference" {
                    for attr in e.attributes().flatten() {
                        let attr_name = std::str::from_utf8(attr.key.as_ref())
                            .unwrap_or("")
                            .to_lowercase();
                        if attr_name == "include" {
                            if let Ok(value) = attr.unescape_value() {
                                current_package = Some(value.to_string());
                            }
                        }
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = std::str::from_utf8(e.name().as_ref())
                    .unwrap_or("")
                    .to_lowercase();

                if name == "packagereference" {
                    if let Some(pkg_name) = current_package.take() {
                        dependencies.insert(UntypedDependency::new(pkg_name, "external"));
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                let name = std::str::from_utf8(e.name().as_ref())
                    .unwrap_or("")
                    .to_lowercase();

                if name == "packagereference" {
                    for attr in e.attributes().flatten() {
                        let attr_name = std::str::from_utf8(attr.key.as_ref())
                            .unwrap_or("")
                            .to_lowercase();
                        if attr_name == "include" {
                            if let Ok(value) = attr.unescape_value() {
                                dependencies
                                    .insert(UntypedDependency::new(value.to_string(), "external"));
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(ConfigParseError::Parse {
                    path: path.to_path_buf(),
                    build_system: "dotnet".to_string(),
                    reason: format!("XML parse error: {}", e),
                });
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(dependencies)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_parser::detector::BuildConfigParser;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_csproj(dir: &TempDir, name: &str, content: &str) {
        let csproj = dir.path().join(name);
        let mut file = std::fs::File::create(&csproj).expect("create failed");
        file.write_all(content.as_bytes()).expect("write failed");
    }

    #[test]
    fn test_parse_simple_csproj() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let content = r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
    <AssemblyName>TestProject</AssemblyName>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="Newtonsoft.Json" Version="13.0.3" />
    <PackageReference Include="Microsoft.Extensions.Logging" Version="8.0.0" />
  </ItemGroup>
</Project>"#;
        create_test_csproj(&temp_dir, "TestProject.csproj", content);

        let parser = DotNetParser;
        let outcome = parser
            .try_parse(temp_dir.path(), temp_dir.path())
            .unwrap()
            .unwrap();
        assert_eq!(outcome.dependencies.len(), 2);
        assert!(
            outcome
                .dependencies
                .iter()
                .any(|d| d.name == "Newtonsoft.Json")
        );

        let mut build_parser = BuildConfigParser::new();
        build_parser
            .scan_project(temp_dir.path(), 0)
            .expect("parse failed");
        let packages = build_parser.packages_for_language(Language::CSharp);
        assert_eq!(packages.len(), 2);
        assert!(packages.contains("Newtonsoft.Json"));
        assert!(packages.contains("Microsoft.Extensions.Logging"));
    }

    #[test]
    fn test_parse_no_csproj_files() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let parser = DotNetParser;
        let result = parser.try_parse(temp_dir.path(), temp_dir.path()).unwrap();
        assert!(result.is_none());

        let mut build_parser = BuildConfigParser::new();
        build_parser
            .scan_project(temp_dir.path(), 0)
            .expect("parse failed");
        let packages = build_parser.packages_for_language(Language::CSharp);
        assert!(packages.is_empty());
    }

    #[test]
    fn test_dotnet_supports_file() {
        let parser = DotNetParser;
        assert!(parser.supports_file("test.csproj"));
        assert!(parser.supports_file("test.fsproj"));
        assert!(!parser.supports_file("test.txt"));
        assert_eq!(parser.name(), ".NET");
    }
}
