//! Maven parser for Java projects

use std::collections::HashSet;
use std::path::Path;

use super::super::super::error::ConfigParseError;
use super::super::super::types::UntypedDependency;
use quick_xml::Reader;
use quick_xml::events::Event;

/// Parse pom.xml file and return dependencies
pub(crate) fn parse_maven_file(
    pom_xml: &Path,
) -> Result<HashSet<UntypedDependency>, ConfigParseError> {
    let content = std::fs::read_to_string(pom_xml).map_err(|e| ConfigParseError::Io {
        path: pom_xml.to_path_buf(),
        source: e,
    })?;
    parse_pom_xml(&content, pom_xml)
}

pub(crate) fn parse_pom_xml(
    content: &str,
    path: &Path,
) -> Result<HashSet<UntypedDependency>, ConfigParseError> {
    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);

    let mut dependencies = HashSet::new();
    let mut buf = Vec::new();
    let mut in_dependency = false;
    let mut current_artifact_id = None;
    let mut current_element = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = std::str::from_utf8(e.name().as_ref())
                    .unwrap_or("")
                    .to_lowercase();
                current_element = Some(name.clone());

                if name == "dependency" {
                    in_dependency = true;
                    current_artifact_id = None;
                }
            }
            Ok(Event::Empty(_e)) => {}
            Ok(Event::Text(e)) => {
                if in_dependency {
                    if let Ok(text) = e.unescape() {
                        let text = text.to_string();
                        if let Some("artifactid") = current_element.as_deref() {
                            current_artifact_id = Some(text);
                        }
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = std::str::from_utf8(e.name().as_ref())
                    .unwrap_or("")
                    .to_lowercase();

                if name == "dependency" {
                    if let Some(artifact) = current_artifact_id.take() {
                        dependencies.insert(UntypedDependency::new(artifact, "external"));
                    }
                    in_dependency = false;
                }
                current_element = None;
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(ConfigParseError::Parse {
                    path: path.to_path_buf(),
                    build_system: "maven".to_string(),
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
    use crate::config_parser::trait_def::LanguageParser;
    use cce_types::Language;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_parse_simple_pom() {
        let temp_dir = TempDir::new().expect("temp dir failed");
        let pom = temp_dir.path().join("pom.xml");
        let content = r#"<project>
  <groupId>com.example</groupId>
  <artifactId>test-project</artifactId>
  <version>1.0.0</version>
  <dependencies>
    <dependency>
      <groupId>org.springframework</groupId>
      <artifactId>spring-core</artifactId>
      <version>6.0.0</version>
    </dependency>
  </dependencies>
</project>"#;
        let mut file = std::fs::File::create(&pom).expect("create failed");
        file.write_all(content.as_bytes()).expect("write failed");

        let deps = parse_maven_file(&pom).expect("parse failed");
        assert!(deps.iter().any(|d| d.name == "spring-core"));

        let parser = crate::config_parser::parsers::java::JavaParser;
        let outcome = parser
            .try_parse(temp_dir.path(), temp_dir.path())
            .unwrap()
            .unwrap();
        assert!(outcome.dependencies.iter().any(|d| d.name == "spring-core"));

        let mut build_parser = BuildConfigParser::new();
        build_parser
            .scan_project(temp_dir.path(), 0)
            .expect("scan failed");
        let packages = build_parser.packages_for_language(Language::Java);
        assert!(packages.contains("spring-core"));
    }
}
