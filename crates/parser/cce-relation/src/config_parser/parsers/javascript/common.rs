//! Common utilities for JavaScript/TypeScript parsers

use std::collections::HashSet;

use super::super::super::types::UntypedDependency;

/// Parse package.json content and convert to dependencies
///
/// Returns parsed dependency set or error message
pub fn parse_package_json_deps(content: &str) -> Result<HashSet<UntypedDependency>, String> {
    let parsed: serde_json::Value = serde_json::from_str(content).map_err(|e| e.to_string())?;

    let mut dependencies = HashSet::new();

    // Parse dependencies
    if let Some(deps) = parsed.get("dependencies").and_then(|v| v.as_object()) {
        for name in deps.keys() {
            dependencies.insert(UntypedDependency::new(name, "external"));
        }
    }

    // Parse devDependencies
    if let Some(deps) = parsed.get("devDependencies").and_then(|v| v.as_object()) {
        for name in deps.keys() {
            dependencies.insert(UntypedDependency::new(name, "dev"));
        }
    }

    // Parse peerDependencies (as regular dependencies)
    if let Some(deps) = parsed.get("peerDependencies").and_then(|v| v.as_object()) {
        for name in deps.keys() {
            dependencies.insert(UntypedDependency::new(name, "external"));
        }
    }

    Ok(dependencies)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_package_json() {
        let content = r#"{
    "name": "test-project",
    "version": "1.0.0",
    "dependencies": {
        "express": "^4.18.0",
        "lodash": "^4.17.21"
    }
}"#;
        let dependencies = parse_package_json_deps(content).expect("parse failed");

        assert_eq!(dependencies.len(), 2);

        let dep_names: Vec<&str> = dependencies.iter().map(|d| d.name.as_str()).collect();
        assert!(dep_names.contains(&"express"));
        assert!(dep_names.contains(&"lodash"));
    }

    #[test]
    fn test_parse_package_json_with_dev_deps() {
        let content = r#"{
    "name": "test-project",
    "dependencies": {
        "express": "^4.18.0"
    },
    "devDependencies": {
        "jest": "^29.0.0",
        "typescript": "^5.0.0"
    }
}"#;
        let dependencies = parse_package_json_deps(content).expect("parse failed");

        assert_eq!(dependencies.len(), 3);

        let dev_deps: Vec<&UntypedDependency> = dependencies
            .iter()
            .filter(|d| d.package_type == "dev")
            .collect();
        assert_eq!(dev_deps.len(), 2);

        let dev_names: Vec<&str> = dev_deps.iter().map(|d| d.name.as_str()).collect();
        assert!(dev_names.contains(&"jest"));
        assert!(dev_names.contains(&"typescript"));
    }

    #[test]
    fn test_parse_package_json_with_peer_deps() {
        let content = r#"{
    "name": "test-library",
    "dependencies": {
        "lodash": "^4.17.21"
    },
    "peerDependencies": {
        "react": ">=16.8.0",
        "react-dom": ">=16.8.0"
    }
}"#;
        let dependencies = parse_package_json_deps(content).expect("parse failed");

        assert_eq!(dependencies.len(), 3);

        let peer_dep = dependencies
            .iter()
            .find(|d| d.name == "react")
            .expect("react not found");
        assert!(peer_dep.package_type == "external"); // Peer deps are not marked as dev
    }

    #[test]
    fn test_parse_package_json_empty_deps() {
        let content = r#"{
    "name": "empty-project",
    "version": "1.0.0"
}"#;
        let dependencies = parse_package_json_deps(content).expect("parse failed");

        assert!(dependencies.is_empty());
    }

    #[test]
    fn test_parse_package_json_invalid() {
        let content = "{ invalid json }";
        assert!(parse_package_json_deps(content).is_err());
    }
}
